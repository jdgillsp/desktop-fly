// Material-aware dielectric shading on a transparent DirectComposition overlay.
// Finishes travel with each vertex, keeping the single ordered mesh draw while
// distinguishing wet skin and glass from dry bark, sand and chitin.

struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    params: vec4<f32>,   // x = ambient, y = shadow strength, z = ground z, w = unused
    // Direction from the scene toward the camera, in world space. Used to be
    // assumed to be +z, which is only true looking straight down; a tilted
    // habitat camera makes specular and rim light wrong without it.
    view_dir: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var skin_color: texture_2d<f32>;
@group(0) @binding(2) var skin_height: texture_2d<f32>;
@group(0) @binding(3) var skin_sampler: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
    @location(3) material: vec4<f32>,
    @location(4) texcoord: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) material: vec4<f32>,
    @location(4) texcoord: vec4<f32>,
) -> VsOut {
    var out: VsOut;
    out.clip = u.view_proj * vec4<f32>(pos, 1.0);
    out.normal = normal;
    out.color = color;
    out.world = pos;
    out.material = material;
    out.texcoord = texcoord;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.texcoord.xy;
    let du = dpdx(uv);
    let dv = dpdy(uv);
    let px = dpdx(in.world);
    let py = dpdy(in.world);
    let base_n = normalize(in.normal);
    let weight = clamp(in.texcoord.z, 0.0, 1.0);
    // Explicit gradients are valid across fragments with different materials.
    let albedo_linear = textureSampleGrad(skin_color, skin_sampler, uv, du, dv).rgb;
    // Existing unorm output shades display-encoded vertex colors. Restore that
    // convention after sRGB texture decoding, preserving untextured appearance.
    let albedo = select(12.92 * albedo_linear,
        1.055 * pow(albedo_linear, vec3<f32>(1.0 / 2.4)) - 0.055,
        albedo_linear > vec3<f32>(0.0031308));
    let base_color = mix(in.color.rgb, albedo, weight);
    let texel = 1.0 / vec2<f32>(textureDimensions(skin_height));
    let hu0 = textureSampleGrad(skin_height, skin_sampler, uv - vec2<f32>(texel.x, 0.0), du, dv).r;
    let hu1 = textureSampleGrad(skin_height, skin_sampler, uv + vec2<f32>(texel.x, 0.0), du, dv).r;
    let hv0 = textureSampleGrad(skin_height, skin_sampler, uv - vec2<f32>(0.0, texel.y), du, dv).r;
    let hv1 = textureSampleGrad(skin_height, skin_sampler, uv + vec2<f32>(0.0, texel.y), du, dv).r;
    let dhdu = (hu1 - hu0) / (2.0 * texel.x);
    let dhdv = (hv1 - hv0) / (2.0 * texel.y);
    // World-space surface gradient preserves mirrored UV handedness and handles
    // degenerate projections without normalizing a zero-length tangent.
    let rx = cross(py, base_n);
    let ry = cross(base_n, px);
    let det = dot(px, rx);
    let inv_det = select(0.0, sign(det) / max(abs(det), 1e-12), abs(det) > 1e-12);
    let grad = ((dhdu * du.x + dhdv * du.y) * rx
        + (dhdu * dv.x + dhdv * dv.y) * ry) * inv_det;
    let n = normalize(base_n - grad * in.texcoord.w * weight);
    let l = normalize(u.light_dir.xyz);
    let ndotl = dot(n, l);
    let wrap = clamp(in.material.z, 0.0, 0.4);
    let lambert = max((ndotl + wrap) / (1.0 + wrap), 0.0);

    // Half-vector specular. The camera is orthographic, so one view vector
    // does for the whole frame.
    let view = normalize(u.view_dir.xyz);
    let half_v = normalize(l + view);
    let roughness = clamp(in.material.x, 0.08, 1.0);
    let exponent = clamp(2.0 / pow(roughness, 4.0) - 2.0, 2.0, 256.0);
    // Derivative broadening keeps tiny wet eyes from blinking as they cross a
    // pixel, and limits sharp highlights on thin moving legs and fin rays.
    let normal_variance = dot(dpdx(n), dpdx(n)) + dot(dpdy(n), dpdy(n));
    let filtered_exponent = exponent / (1.0 + exponent * normal_variance * 0.35);
    let spec = pow(max(dot(n, half_v), 0.0), filtered_exponent)
        * in.material.y * smoothstep(-0.08, 0.20, ndotl);

    let ambient = u.params.x;
    // A restrained broad fill follows the viewing side, like the Blender
    // study's softbox: lateral eyes and raised faces retain readable detail.
    let fill_dir = normalize(view + vec3<f32>(0.35, -0.20, 0.45));
    let fill = 0.18 * max(dot(n, fill_dir), 0.0);
    let fill_spec = pow(max(dot(n, normalize(fill_dir + view)), 0.0),
        min(filtered_exponent, 96.0)) * in.material.y * 0.22;
    var rgb = base_color * (ambient + (1.0 - ambient) * (lambert + fill))
        + vec3<f32>(spec + fill_spec);

    // A faint rim so the silhouette survives against any wallpaper, light or
    // dark. Without this the fly disappears over dark windows.
    let rim = pow(1.0 - max(dot(n, view), 0.0), 3.0);
    rgb += vec3<f32>(0.35, 0.38, 0.45) * rim * in.material.w;

    let a = in.color.a;
    // Invisible cuticle/skin fragments must not occlude later transparent surfaces.
    if (a <= 0.001) { discard; }
    // Premultiplied alpha: the composition swapchain expects RGB already
    // multiplied through, and getting it wrong haloes every edge.
    return vec4<f32>(rgb * a, a);
}

// ---------------------------------------------------------------------------
// Contact shadow: the fly's silhouette flattened onto the desktop plane and
// offset by its altitude, so height reads even though there is nothing to cast
// onto. Cheaper than a shadow map and, on a transparent overlay, more correct —
// there is no receiver geometry to render into.
// ---------------------------------------------------------------------------

@vertex
fn vs_shadow(
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) material: vec4<f32>,
    @location(4) texcoord: vec4<f32>,
) -> VsOut {
    var out: VsOut;
    // Project to z = ground, sliding away from the light with height.
    let drop = pos.z * 0.35;
    let flat = vec3<f32>(pos.x + drop, pos.y - drop, u.params.z);
    out.clip = u.view_proj * vec4<f32>(flat, 1.0);
    out.normal = normal;
    out.color = color;
    out.world = flat;
    out.material = material;
    out.texcoord = texcoord;
    return out;
}

@fragment
fn fs_shadow(in: VsOut) -> @location(0) vec4<f32> {
    let a = u.params.y;
    return vec4<f32>(0.0, 0.0, 0.0, a);
}

// ---------------------------------------------------------------------------
// The connectome glowing inside a glass body. Unlit and additive: these are
// neurons firing, not surfaces catching light.
// ---------------------------------------------------------------------------

@fragment
fn fs_neuron(in: VsOut) -> @location(0) vec4<f32> {
    // The billboard's corner offset rides in the unused normal, so these read
    // as round points of light rather than a grid of squares.
    let uv = in.normal.xy;
    let d = dot(uv, uv);
    if (d > 1.0) {
        discard;
    }
    let falloff = 1.0 - smoothstep(0.0, 1.0, d);
    let a = in.color.a * falloff;
    return vec4<f32>(in.color.rgb * a, a);
}
