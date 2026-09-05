// Spike 0 shader: a lit shape rendered onto a fully transparent background.
// The whole point of the spike is that fragments we don't draw stay at alpha 0
// and the desktop shows through, while drawn fragments composite correctly.

struct Uniforms {
    mvp: mat4x4<f32>,
    model: mat4x4<f32>,
    tint: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.clip = u.mvp * vec4<f32>(pos, 1.0);
    out.normal = normalize((u.model * vec4<f32>(normal, 0.0)).xyz);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Key light roughly matching the SceneKit rig in main.swift:buildScene
    let key = normalize(vec3<f32>(0.35, 0.75, 0.55));
    let n = normalize(in.normal);
    let lambert = max(dot(n, key), 0.0);
    let ambient = 0.35;
    let lit = u.tint.rgb * (ambient + 0.85 * lambert);

    // Rim term so the silhouette reads against any desktop wallpaper, light or dark.
    let view = vec3<f32>(0.0, 0.0, 1.0);
    let rim = pow(1.0 - max(dot(n, view), 0.0), 2.5);
    let color = lit + vec3<f32>(0.5, 0.65, 0.9) * rim * 0.6;

    let alpha = u.tint.a;
    // Premultiplied alpha: the composition swapchain expects RGB already
    // multiplied by A. Getting this wrong is the classic transparent-overlay
    // bug (bright halos around every edge), so the spike does it explicitly.
    return vec4<f32>(color * alpha, alpha);
}
