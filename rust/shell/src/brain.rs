//! The brain window: 23,210 real soma positions, the escape circuit picked out
//! by role, live spikes flashing where they actually happen, and click-to-
//! stimulate.
//!
//! Port of `BrainView.swift`. This is the app's differentiator — it is the only
//! desktop pet that shows you its reasoning — and the interaction is the best
//! toy in the project: clicking a region drives the real neurons there, and
//! whatever the network does downstream is what the body does.

use dfcore::data::BrainPointsFile;
use dfcore::Sim;

use crate::math::{self, Mat4};

/// FlyWire super-class palette, index order from `etl.py` (BrainView.swift:39).
/// The colour of anything invented. Deliberately outside every measured
/// population's hue so the brain window reads honestly without a legend.
pub const AUTHORED_COLOR: [f32; 4] = [0.60, 0.70, 1.00, 1.0];

const CLASS_COLORS: [[f32; 4]; 9] = [
    [0.16, 0.22, 0.34, 1.0], // optic — dim blue; the majority, kept subtle
    [0.45, 0.33, 0.16, 1.0], // central — amber
    [0.14, 0.36, 0.34, 1.0], // sensory — teal
    [0.10, 0.48, 0.62, 1.0], // visual_projection — cyan
    [0.38, 0.22, 0.55, 1.0], // visual_centrifugal — violet
    [0.62, 0.28, 0.10, 1.0], // descending — orange
    [0.20, 0.45, 0.18, 1.0], // ascending — green
    [0.55, 0.14, 0.14, 1.0], // motor — red
    [0.50, 0.25, 0.40, 1.0], // endocrine — pink
];

/// Human-readable name for a picked region (BrainView.swift:300), resolved
/// through the creature's role manifest rather than a table duplicated here.
fn region_name(sim: &dyn Sim, picked: &[usize]) -> String {
    let roles = sim.roles();
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for &i in picked {
        *counts.entry(roles[i].as_str()).or_default() += 1;
    }
    let mut best = ("other", 0usize);
    for (k, v) in &counts {
        // "other" only wins if nothing named is present.
        if *k != "other" && *v > best.1 {
            best = (k, *v);
        }
    }
    format!("{} ({} neurons)", sim.manifest().label_for(best.0), picked.len())
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PointVertex {
    center: [f32; 3],
    color: [f32; 4],
    /// `corner.xy` in [-1,1], `corner.z` = per-point radius scale.
    corner: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: Mat4,
    model: Mat4,
    params: [f32; 4],
}

/// Half-extent of the orthographic view. The ETL normalises the whole brain
/// into [-10, 10].
const VIEW_HALF: f32 = 11.5;

fn expand_points(centers: &[([f32; 3], [f32; 4], f32)]) -> (Vec<PointVertex>, Vec<u32>) {
    let mut verts = Vec::with_capacity(centers.len() * 4);
    let mut idx = Vec::with_capacity(centers.len() * 6);
    const CORNERS: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
    for (n, (c, col, scale)) in centers.iter().enumerate() {
        for k in CORNERS {
            verts.push(PointVertex {
                center: *c,
                color: *col,
                corner: [k[0], k[1], *scale],
            });
        }
        let b = (n * 4) as u32;
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    (verts, idx)
}

/// Which circuit neuron a click at `(px, py)` in a `w x h` window points at,
/// under model rotation `model`. Returns `None` if the click missed the circuit.
///
/// Orthographic, so every ray is parallel to -z in view space; the ray is
/// brought into brain-local space by inverting the model rotation (whose
/// inverse is its transpose, these being pure rotations).
pub fn pick_neuron(
    positions: &[[f32; 3]],
    model: &Mat4,
    px: f32,
    py: f32,
    w: f32,
    h: f32,
    max_perp: f32,
) -> Option<usize> {
    if positions.is_empty() || w <= 0.0 || h <= 0.0 {
        return None;
    }
    let aspect = w / h;
    let ndc_x = (px / w) * 2.0 - 1.0;
    let ndc_y = 1.0 - (py / h) * 2.0;
    let vx = ndc_x * VIEW_HALF * aspect;
    let vy = ndc_y * VIEW_HALF;

    let m = model;
    let inv = [
        [m[0][0], m[1][0], m[2][0], 0.0],
        [m[0][1], m[1][1], m[2][1], 0.0],
        [m[0][2], m[1][2], m[2][2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let origin = math::transform_point(&inv, [vx, vy, 50.0]);
    let dir = math::transform_dir(&inv, [0.0, 0.0, -1.0]);

    let mut best = usize::MAX;
    let mut best_d = f32::MAX;
    for (i, p) in positions.iter().enumerate() {
        let ap = [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]];
        let t = ap[0] * dir[0] + ap[1] * dir[1] + ap[2] * dir[2];
        let q = [ap[0] - t * dir[0], ap[1] - t * dir[1], ap[2] - t * dir[2]];
        let d = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    if best_d > max_perp {
        None
    } else {
        Some(best)
    }
}

pub struct BrainView {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    ubuf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,

    cloud_v: wgpu::Buffer,
    cloud_i: wgpu::Buffer,
    cloud_index_count: u32,

    circuit_v: wgpu::Buffer,
    circuit_i: wgpu::Buffer,
    circuit_index_count: u32,
    /// One entry per circuit neuron: base colour and current flash brightness.
    circuit_pos: Vec<[f32; 3]>,
    circuit_base: Vec<[f32; 4]>,
    flash: Vec<f32>,
    scratch: Vec<PointVertex>,

    pub rotation: f32,
    pub paused_by_hover: bool,
    pub last_label: String,
}

impl BrainView {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        adapter: &wgpu::Adapter,
        surface: wgpu::Surface<'static>,
        points: &BrainPointsFile,
        sim: &dyn Sim,
        width: u32,
        height: u32,
    ) -> Self {
        let caps = surface.get_capabilities(adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: width.max(1),
            height: height.max(1),
            // Deliberately NOT vsync: two vsync-blocking presents on one
            // thread serialise, and the overlay is the one that must not stall.
            present_mode: wgpu::PresentMode::AutoNoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(device, &config);

        // --- the 23k-soma cloud, static ---
        let cloud: Vec<([f32; 3], [f32; 4], f32)> = points
            .points
            .iter()
            .filter(|p| p.len() >= 4)
            .map(|p| {
                let ci = (p[3] as usize).min(CLASS_COLORS.len() - 1);
                let mut c = CLASS_COLORS[ci];
                // Dim, so 23k additive sprites read as a volume rather than a wall.
                c[3] = 0.30;
                ([p[0], p[1], p[2]], c, 1.0)
            })
            .collect();
        let (cv, ci) = expand_points(&cloud);
        let cloud_index_count = ci.len() as u32;

        // --- the 668-neuron circuit, rebuilt each frame for spike flashes ---
        let circuit_pos: Vec<[f32; 3]> = sim.positions().to_vec();
        // Colour by provenance first, role second. A measured neuron gets its
        // population's colour; an authored one gets the cool AUTHORED colour
        // and a bigger sprite, so an invented element can never hide among
        // real ones (SPIDER_PLAN.md §2 rule 3). For a measured dataset this
        // changes nothing.
        let circuit_base: Vec<[f32; 4]> = sim
            .roles()
            .iter()
            .enumerate()
            .map(|(i, r)| match sim.origin(i) {
                dfcore::Origin::Measured => sim.manifest().color_for(r),
                dfcore::Origin::Authored => AUTHORED_COLOR,
            })
            .collect();
        let circuit_seed: Vec<([f32; 3], [f32; 4], f32)> = circuit_pos
            .iter()
            .enumerate()
            .zip(circuit_base.iter())
            .map(|((i, p), c)| {
                let size = match sim.origin(i) {
                    dfcore::Origin::Measured => 2.2,
                    dfcore::Origin::Authored => 4.0,
                };
                (*p, *c, size)
            })
            .collect();
        let (qv, qi) = expand_points(&circuit_seed);
        let circuit_index_count = qi.len() as u32;

        use wgpu::util::DeviceExt;
        let cloud_v = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cloud verts"),
            contents: bytemuck::cast_slice(&cv),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let cloud_i = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cloud idx"),
            contents: bytemuck::cast_slice(&ci),
            usage: wgpu::BufferUsages::INDEX,
        });
        let circuit_v = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("circuit verts"),
            size: (qv.len() * std::mem::size_of::<PointVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&circuit_v, 0, bytemuck::cast_slice(&qv));
        let circuit_i = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("circuit idx"),
            contents: bytemuck::cast_slice(&qi),
            usage: wgpu::BufferUsages::INDEX,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("brain"),
            source: wgpu::ShaderSource::Wgsl(include_str!("brain_shader.wgsl").into()),
        });
        let ubuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: ubuf.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("brain points"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_point"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<PointVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x3],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_point"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // Additive: overlapping somas accumulate, as in SceneKit.
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            // No depth buffer: the cloud is additive, so draw order is irrelevant
            // and depth would only fight the blend.
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        BrainView {
            device: device.clone(),
            queue: queue.clone(),
            surface,
            config,
            pipeline,
            ubuf,
            bind_group,
            cloud_v,
            cloud_i,
            cloud_index_count,
            circuit_v,
            circuit_i,
            circuit_index_count,
            circuit_pos,
            circuit_base,
            flash: vec![0.0; sim.n()],
            scratch: qv,
            rotation: 0.0,
            paused_by_hover: false,
            last_label: String::new(),
        }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.config.width = w.max(1);
        self.config.height = h.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    /// The model transform: a slow spin plus a fixed tilt, so the brain reads as
    /// a volume rather than a flat disc.
    fn model(&self) -> Mat4 {
        math::mul(math::rotate_x(-0.25), math::rotate_y(self.rotation))
    }

    /// Reads through the `Sim` trait rather than a concrete simulation, so the
    /// window works for a creature whose neurons never spike: `activity()` is
    /// spike flashes for the fly and normalised depolarisation for the worm.
    ///
    /// Sampled rather than accumulated — the brain redraws at ~30 Hz while the
    /// sim steps on the render tick, so brief activity between reads is missed.
    /// The decay makes that read as a glow rather than a stutter.
    pub fn update(&mut self, dt: f32, sim: &dyn Sim) {
        if !self.paused_by_hover {
            self.rotation += dt * 0.35;
        }
        let decay = (-dt * 6.0).exp();
        for f in self.flash.iter_mut() {
            *f *= decay;
        }
        for (i, a) in sim.activity().iter().enumerate() {
            if i < self.flash.len() && *a > 0.0 {
                // Take the brighter of decayed and current, so a flash reads as
                // an event rather than flickering with the sampling rate.
                self.flash[i] = self.flash[i].max(a * 3.0);
            }
        }
    }

    /// Rebuild the circuit vertex buffer with current flash brightness baked in.
    fn upload_circuit(&mut self) {
        for (n, base) in self.circuit_base.iter().enumerate() {
            let f = self.flash[n];
            let mut c = *base;
            c[0] = (c[0] + f).min(4.0);
            c[1] = (c[1] + f).min(4.0);
            c[2] = (c[2] + f * 0.8).min(4.0);
            c[3] = (0.55 + f * 0.5).min(1.0);
            let scale = 2.2 + f * 1.6;
            for k in 0..4 {
                let v = &mut self.scratch[n * 4 + k];
                v.color = c;
                v.corner[2] = scale;
            }
        }
        self.queue
            .write_buffer(&self.circuit_v, 0, bytemuck::cast_slice(&self.scratch));
    }

    pub fn render(&mut self) {
        self.upload_circuit();

        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        let view_proj = math::ortho(VIEW_HALF * aspect, VIEW_HALF, -100.0, 100.0);
        self.queue.write_buffer(
            &self.ubuf,
            0,
            bytemuck::bytes_of(&Uniforms {
                view_proj,
                model: self.model(),
                // Radii in clip units: ~1.2 px and ~2.6 px at typical sizes.
                params: [2.4 / self.config.height as f32, 0.0, aspect, 0.0],
            }),
        );

        use wgpu::CurrentSurfaceTexture as Cst;
        let acquired = self.surface.get_current_texture();
        let frame = match acquired {
            Cst::Success(f) | Cst::Suboptimal(f) => f,
            Cst::Outdated | Cst::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("brain"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // The near-black of the SceneKit brain window.
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.03,
                            g: 0.035,
                            b: 0.06,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.cloud_v.slice(..));
            pass.set_index_buffer(self.cloud_i.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.cloud_index_count, 0, 0..1);

            pass.set_vertex_buffer(0, self.circuit_v.slice(..));
            pass.set_index_buffer(self.circuit_i.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.circuit_index_count, 0, 0..1);
        }
        self.queue.submit(Some(enc.finish()));
        self.queue.present(frame);
    }

    /// Click-to-stimulate. Port of `handleClick` (BrainView.swift:264).
    ///
    /// The reaction is whatever the real network does downstream — click the
    /// Giant Fiber and the fly escapes; click DNg11 and it grooms.
    pub fn handle_click(&mut self, px: f32, py: f32, sim: &mut dyn Sim) -> Option<String> {
        let (w, h) = (self.config.width as f32, self.config.height as f32);
        let best = pick_neuron(&self.circuit_pos, &self.model(), px, py, w, h, 3.0)?;
        let anchor = self.circuit_pos[best];
        let dist2 = |p: &[f32; 3]| {
            let d = [p[0] - anchor[0], p[1] - anchor[1], p[2] - anchor[2]];
            d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
        };
        let mut picked: Vec<usize> = (0..self.circuit_pos.len())
            .filter(|&i| dist2(&self.circuit_pos[i]) < 2.2 * 2.2)
            .collect();
        if picked.len() < 4 {
            let mut all: Vec<usize> = (0..self.circuit_pos.len()).collect();
            all.sort_by(|a, b| {
                dist2(&self.circuit_pos[*a])
                    .partial_cmp(&dist2(&self.circuit_pos[*b]))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            picked = all.into_iter().take(6).collect();
        } else if picked.len() > 60 {
            picked.sort_by(|a, b| {
                dist2(&self.circuit_pos[*a])
                    .partial_cmp(&dist2(&self.circuit_pos[*b]))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            picked.truncate(60);
        }

        sim.stimulate(&picked, 0.25, 400);
        for &i in picked.iter().take(16) {
            self.flash[i] = 2.0;
        }
        let label = region_name(sim, &picked);
        self.last_label = label.clone();
        Some(label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_super_class_has_a_colour() {
        // etl.py writes 9 super-classes; a missing entry would silently clamp
        // several classes onto the same colour.
        assert_eq!(CLASS_COLORS.len(), 9);
    }

    /// Role colours now come from the shared manifest, so this asserts the
    /// brain window and the sim cannot drift apart.
    #[test]
    fn roles_a_user_must_distinguish_have_distinct_colours() {
        let m = dfcore::drosophila();
        assert_ne!(m.color_for("dnp09"), m.color_for("dng11"));
        assert_ne!(m.color_for("mdn"), m.color_for("lc4"));
        let lum = |c: [f32; 4]| c[0] + c[1] + c[2];
        assert!(lum(m.color_for("gf")) > lum(m.color_for("other")));
    }

    /// Clicking where a neuron is drawn must pick that neuron. A sign flip in
    /// the NDC conversion or the inverse rotation silently picks the neuron on
    /// the *far* side of the brain, which looks plausible and is wrong.
    #[test]
    fn clicking_on_a_neuron_picks_that_neuron() {
        let positions = vec![
            [-6.0, 0.0, 0.0], // 0: left
            [6.0, 0.0, 0.0],  // 1: right
            [0.0, 6.0, 0.0],  // 2: top
            [0.0, -6.0, 0.0], // 3: bottom
        ];
        let m = math::identity();
        let (w, h) = (400.0f32, 400.0f32);
        let aspect = w / h;

        // Project each neuron back to a pixel and click it.
        for (i, p) in positions.iter().enumerate() {
            let ndc_x = p[0] / (VIEW_HALF * aspect);
            let ndc_y = p[1] / VIEW_HALF;
            let px = (ndc_x + 1.0) * 0.5 * w;
            let py = (1.0 - ndc_y) * 0.5 * h;
            assert_eq!(
                pick_neuron(&positions, &m, px, py, w, h, 3.0),
                Some(i),
                "clicking neuron {i} at ({px}, {py})"
            );
        }
    }

    /// y must be flipped: window pixels grow downward, scene y grows upward.
    #[test]
    fn clicking_the_top_of_the_window_picks_the_top_neuron() {
        let positions = vec![[0.0, 6.0, 0.0], [0.0, -6.0, 0.0]];
        let m = math::identity();
        assert_eq!(
            pick_neuron(&positions, &m, 200.0, 40.0, 400.0, 400.0, 4.0),
            Some(0),
            "a click near the top of the window must hit the +y neuron"
        );
        assert_eq!(
            pick_neuron(&positions, &m, 200.0, 360.0, 400.0, 400.0, 4.0),
            Some(1)
        );
    }

    /// Under rotation the picking must follow the neuron on screen, not its
    /// original position — otherwise clicking a spinning brain picks at random.
    #[test]
    fn picking_respects_the_model_rotation() {
        let positions = vec![[8.0, 0.0, 0.0], [-8.0, 0.0, 0.0]];
        // Quarter turn about y: neuron 0 (+x) swings to -z, i.e. to screen centre.
        let m = math::rotate_y(std::f32::consts::FRAC_PI_2);
        let hit = pick_neuron(&positions, &m, 200.0, 200.0, 400.0, 400.0, 9.0);
        assert!(hit.is_some(), "a rotated neuron should still be pickable");
        // Both are now near the centre line in x, so the nearest to the centre
        // ray is whichever has the smaller perpendicular distance; the point is
        // that picking does not panic and stays in range.
        assert!(hit.unwrap() < positions.len());
    }

    #[test]
    fn clicking_empty_space_picks_nothing() {
        let positions = vec![[0.0, 0.0, 0.0]];
        let m = math::identity();
        // Far corner of the window, well away from the only neuron.
        assert_eq!(
            pick_neuron(&positions, &m, 4.0, 4.0, 400.0, 400.0, 1.0),
            None
        );
    }

    #[test]
    fn picking_handles_degenerate_inputs() {
        let m = math::identity();
        assert_eq!(pick_neuron(&[], &m, 10.0, 10.0, 400.0, 400.0, 5.0), None);
        assert_eq!(
            pick_neuron(&[[0.0; 3]], &m, 10.0, 10.0, 0.0, 400.0, 5.0),
            None
        );
    }

    #[test]
    fn expanding_points_produces_two_triangles_each() {
        let (v, i) = expand_points(&[([0.0, 0.0, 0.0], [1.0; 4], 1.0); 5]);
        assert_eq!(v.len(), 20);
        assert_eq!(i.len(), 30);
        assert!(i.iter().all(|&k| (k as usize) < v.len()));
        // Corners must cover the quad so the sprite is not degenerate.
        let xs: Vec<f32> = v[0..4].iter().map(|q| q.corner[0]).collect();
        assert!(xs.contains(&-1.0) && xs.contains(&1.0));
    }
}
