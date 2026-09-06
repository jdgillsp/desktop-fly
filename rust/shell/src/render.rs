//! wgpu renderer for the overlay.
//!
//! Deliberately small: one dynamic vertex buffer rebuilt each frame, two
//! pipelines (body, contact shadow), one uniform buffer. See `flybody.rs` for
//! why CPU-side transform is the right call at this scene size.

use crate::math::{self, Mat4};
use crate::mesh::{Mesh, Vertex};

/// Everything one frame needs drawn. A struct rather than eight parameters:
/// habitat mode added a camera, a ground plane and a sub-range, and at the call
/// site `render(s, m, n, true, 0..n, cam, -0.5)` says nothing about which
/// argument is which.
pub struct Frame<'a> {
    /// The whole buffer to draw, in order. In habitat mode this is the tank,
    /// then the creature, then the front glass — index order is draw order, and
    /// that is what makes the glass blend over the animal.
    pub mesh: &'a Mesh,
    pub neurons: Option<&'a Mesh>,
    pub shadows: bool,
    /// The creature's own slice of `mesh`. A tank that cast a shadow would
    /// paint a large black shape over the desktop, so the shadow pass draws
    /// only this range. The whole buffer, in free roam.
    pub creature: std::ops::Range<u32>,
    /// `TopDown` reproduces the original matrices exactly; habitat mode tilts.
    pub camera: crate::camera::Camera,
    /// The plane shadows land on: the desktop in free roam, the tank floor
    /// inside an enclosure.
    pub ground_z: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: Mat4,
    light_dir: [f32; 4],
    params: [f32; 4],
    view_dir: [f32; 4],
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    /// Additive, depth-testing-off: the connectome glowing inside the shell.
    neuron_pipeline: wgpu::RenderPipeline,
    nvbuf: wgpu::Buffer,
    nibuf: wgpu::Buffer,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    vbuf_cap: usize,
    ibuf_cap: usize,
    ubuf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    depth: wgpu::TextureView,
    pub alpha_ok: bool,
    /// Physical pixels per logical scene unit; keeps the creature the same
    /// apparent size on a scaled display.
    pub scale: f32,
}

fn make_depth(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

impl Renderer {
    /// Takes an existing device so the brain window can share it — two wgpu
    /// devices on one GPU would double the memory for no benefit.
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter: &wgpu::Adapter,
        surface: &wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> Self {
        let caps = surface.get_capabilities(adapter);
        let alpha_mode = if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else {
            wgpu::CompositeAlphaMode::Auto
        };
        let alpha_ok = alpha_mode == wgpu::CompositeAlphaMode::PreMultiplied;
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
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fly"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let ubuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
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

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4],
        };

        let make_pipeline = |vs: &str, fs: &str, depth_write: bool, cull: Option<wgpu::Face>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(vs),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[Some(vertex_layout.clone())],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: cull,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };

        // Wings are translucent and legs are thin, so back-face culling is off
        // for the body: SceneKit's default is double-sided too.
        let pipeline = make_pipeline("vs_main", "fs_main", true, None);
        let shadow_pipeline = make_pipeline("vs_shadow", "fs_shadow", false, None);

        // The neuron field is additive and ignores depth, so the connectome
        // reads *through* the translucent shell rather than being occluded by it.
        let neuron_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("neurons"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(vertex_layout.clone())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_neuron"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        let depth = make_depth(&device, &config);
        let vbuf_cap = 1 << 16;
        let ibuf_cap = 1 << 17;
        let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("verts"),
            size: (vbuf_cap * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ibuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("indices"),
            size: (ibuf_cap * 4) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let nvbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("neuron verts"),
            size: (vbuf_cap * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let nibuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("neuron indices"),
            size: (ibuf_cap * 4) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Renderer {
            device,
            queue,
            config,
            pipeline,
            shadow_pipeline,
            neuron_pipeline,
            nvbuf,
            nibuf,
            vbuf,
            ibuf,
            vbuf_cap,
            ibuf_cap,
            ubuf,
            bind_group,
            depth,
            alpha_ok,
            scale: 1.0,
        }
    }

    pub fn resize(&mut self, surface: &wgpu::Surface<'static>, w: u32, h: u32) {
        self.config.width = w.max(1);
        self.config.height = h.max(1);
        surface.configure(&self.device, &self.config);
        self.depth = make_depth(&self.device, &self.config);
    }

    pub fn render(&mut self, surface: &wgpu::Surface<'static>, frame: &Frame) {
        let Frame {
            mesh,
            neurons,
            shadows,
            creature,
            camera,
            ground_z,
        } = frame;
        let (shadows, camera, ground_z) = (*shadows, *camera, *ground_z);
        let neurons = *neurons;
        if mesh.indices.is_empty() {
            return;
        }
        // Skip the frame rather than panic. A dropped frame is a blink; an
        // assert here takes down a pet that is supposed to sit on the desktop
        // for days.
        if mesh.verts.len() > self.vbuf_cap || mesh.indices.len() > self.ibuf_cap {
            eprintln!(
                "frame geometry exceeds buffers ({} verts, {} indices) - skipping",
                mesh.verts.len(),
                mesh.indices.len()
            );
            return;
        }
        self.queue
            .write_buffer(&self.vbuf, 0, bytemuck::cast_slice(&mesh.verts));
        self.queue
            .write_buffer(&self.ibuf, 0, bytemuck::cast_slice(&mesh.indices));
        let neuron_count = match neurons {
            Some(n) if !n.indices.is_empty() && n.verts.len() <= self.vbuf_cap => {
                self.queue
                    .write_buffer(&self.nvbuf, 0, bytemuck::cast_slice(&n.verts));
                self.queue
                    .write_buffer(&self.nibuf, 0, bytemuck::cast_slice(&n.indices));
                n.indices.len() as u32
            }
            _ => 0,
        };

        // The scene is in logical units, the surface in physical pixels, so the
        // orthographic extent has to be divided through by the DPI scale — or
        // the creature renders at 1 scene unit per physical pixel and shrinks
        // on a scaled display.
        let half_w = self.config.width as f32 / 2.0 / self.scale;
        let half_h = self.config.height as f32 / 2.0 / self.scale;
        let proj = math::ortho(half_w, half_h, 1.0, camera.far());
        let view = camera.view();
        let vd = camera.view_dir();
        self.queue.write_buffer(
            &self.ubuf,
            0,
            bytemuck::bytes_of(&Uniforms {
                view_proj: math::mul(proj, view),
                // Matches the SceneKit key light's direction.
                light_dir: [0.30, 0.62, 0.72, 0.0],
                params: [0.42, 0.22, ground_z, 0.0],
                view_dir: [vd[0], vd[1], vd[2], 0.0],
            }),
        );

        use wgpu::CurrentSurfaceTexture as Cst;
        let frame = match surface.get_current_texture() {
            Cst::Success(f) | Cst::Suboptimal(f) => f,
            Cst::Outdated | Cst::Lost => {
                surface.configure(&self.device, &self.config);
                return;
            }
            Cst::Timeout | Cst::Occluded => return,
            Cst::Validation => {
                eprintln!("surface validation error");
                return;
            }
        };
        let view_tex = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view_tex,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Alpha 0: everything we do not draw is the desktop.
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vbuf.slice(..));
            pass.set_index_buffer(self.ibuf.slice(..), wgpu::IndexFormat::Uint32);

            if shadows {
                pass.set_pipeline(&self.shadow_pipeline);
                pass.draw_indexed(creature.clone(), 0, 0..1);
            }
            pass.set_pipeline(&self.pipeline);
            pass.draw_indexed(0..mesh.indices.len() as u32, 0, 0..1);

            if neuron_count > 0 {
                pass.set_pipeline(&self.neuron_pipeline);
                pass.set_vertex_buffer(0, self.nvbuf.slice(..));
                pass.set_index_buffer(self.nibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..neuron_count, 0, 0..1);
            }
        }
        self.queue.submit(Some(enc.finish()));
        self.queue.present(frame);
    }
}
