// Blinn-Phong-ish shading approximating the SceneKit rig in main.swift:buildScene
// (directional key at intensity 1000 with euler (-0.35, 0.30, 0), ambient 550),
// rendered onto a fully transparent background for the DirectComposition
// overlay.

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

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
};

@vertex
fn vs_main(
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
) -> VsOut {
    var out: VsOut;
    out.clip = u.view_proj * vec4<f32>(pos, 1.0);
    out.normal = normal;
    out.color = color;
    out.world = pos;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let l = normalize(u.light_dir.xyz);
    let lambert = max(dot(n, l), 0.0);

    // Half-vector specular. The camera is orthographic, so one view vector
    // does for the whole frame.
    let view = normalize(u.view_dir.xyz);
    let half_v = normalize(l + view);
    let spec = pow(max(dot(n, half_v), 0.0), 24.0) * 0.35;

    let ambient = u.params.x;
    var rgb = in.color.rgb * (ambient + (1.0 - ambient) * lambert) + vec3<f32>(spec);

    // A faint rim so the silhouette survives against any wallpaper, light or
    // dark. Without this the fly disappears over dark windows.
    let rim = pow(1.0 - max(dot(n, view), 0.0), 3.0);
    rgb += vec3<f32>(0.35, 0.38, 0.45) * rim * 0.25;

    let a = in.color.a;
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
) -> VsOut {
    var out: VsOut;
    // Project to z = ground, sliding away from the light with height.
    let drop = pos.z * 0.35;
    let flat = vec3<f32>(pos.x + drop, pos.y - drop, u.params.z);
    out.clip = u.view_proj * vec4<f32>(flat, 1.0);
    out.normal = normal;
    out.color = color;
    out.world = flat;
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
