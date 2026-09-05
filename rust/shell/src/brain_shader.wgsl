// Brain-window point cloud: 23,210 real soma positions, drawn as screen-facing
// billboards. Modern D3D12/Vulkan have no gl_PointSize equivalent, so
// SceneKit's `minimumPointScreenSpaceRadius` becomes an expanded quad.

struct Uniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    // x = cloud point radius (clip units), y = circuit point radius, z = aspect
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_point(
    @location(0) center: vec3<f32>,
    @location(1) color: vec4<f32>,
    // corner in [-1,1]^2, plus a per-point radius scale in .z
    @location(2) corner: vec3<f32>,
) -> VsOut {
    var out: VsOut;
    let world = u.model * vec4<f32>(center, 1.0);
    var clip = u.view_proj * world;
    let r = u.params.x * corner.z;
    // Aspect-correct so points stay round in a non-square window.
    clip.x += corner.x * r / u.params.z;
    clip.y += corner.y * r;
    out.clip = clip;
    out.color = color;
    out.uv = corner.xy;
    return out;
}

@fragment
fn fs_point(in: VsOut) -> @location(0) vec4<f32> {
    // Round, soft-edged sprite. Discarding the corners is what stops the cloud
    // looking like a field of squares.
    let d = dot(in.uv, in.uv);
    if (d > 1.0) {
        discard;
    }
    let falloff = 1.0 - smoothstep(0.25, 1.0, d);
    let a = in.color.a * falloff;
    // Additive, like the SceneKit material (blendMode .add): overlapping somas
    // accumulate into the bright regions of the brain.
    return vec4<f32>(in.color.rgb * a, a);
}
