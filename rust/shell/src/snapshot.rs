//! Offscreen render to a PNG — the equivalent of the Swift build's
//! `--snapshot` (main.swift:80), and listed as a diagnostic in CLAUDE.md.
//!
//! It also separates two failure modes that look identical on screen: "the
//! renderer is wrong" and "the overlay is not compositing". This path touches
//! no window and no swapchain, so if the fly appears here and not on the
//! desktop, the bug is in the overlay.

use crate::math;
use crate::mesh::Vertex;
use crate::runtime::Runtime;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: math::Mat4,
    light_dir: [f32; 4],
    params: [f32; 4],
}

/// Renders one frame of the creature into `path`, on a checkerboard so alpha
/// is visible. `alt` lifts a flier into flight for testing the altitude scale.
///
/// Goes through the same [`Runtime`] the desktop uses, so this diagnostic
/// exercises the generic path rather than a private copy of the fly's.
pub fn render_to_png(
    rt: &mut dyn Runtime,
    path: &str,
    width: u32,
    height: u32,
    alt: f32,
    walking_frames: u32,
    glass: bool,
) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        #[cfg(target_os = "windows")]
        backends: wgpu::Backends::DX12,
        #[cfg(not(target_os = "windows"))]
        backends: wgpu::Backends::PRIMARY,
        flags: wgpu::InstanceFlags::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: Default::default(),
        display: None,
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .expect("no adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("snapshot"),
        required_features: wgpu::Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("device");

    // Pose the creature and build its geometry through the generic path. The
    // circuit is run for real inside `snapshot_pose`, so the glass body shows
    // real activity rather than a decorative sparkle.
    rt.snapshot_pose(alt, walking_frames, (width as f32, height as f32));
    let geometry = rt.build(glass);
    let frame = geometry.body.clone();
    let neuron_mesh = geometry
        .neurons
        .filter(|m| !m.indices.is_empty())
        .cloned();
    let have_neurons = neuron_mesh.is_some();
    let neuron_mesh = neuron_mesh.unwrap_or_default();
    println!(
        "snapshot: {} verts, {} indices, {} neurons lit",
        frame.verts.len(),
        frame.indices.len(),
        neuron_mesh.verts.len() / 4
    );

    let format = wgpu::TextureFormat::Rgba8Unorm;
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let depth = device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default());

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
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
        label: None,
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4],
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });

    let neuron_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("neurons"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4],
            })],
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

    use wgpu::util::DeviceExt;
    let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&frame.verts),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let (nvbuf, nibuf) = if have_neurons {
        (
            Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&neuron_mesh.verts),
                usage: wgpu::BufferUsages::VERTEX,
            })),
            Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&neuron_mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            })),
        )
    } else {
        (None, None)
    };
    let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&frame.indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let half_w = width as f32 / 2.0;
    let half_h = height as f32 / 2.0;
    queue.write_buffer(
        &ubuf,
        0,
        bytemuck::bytes_of(&Uniforms {
            view_proj: math::mul(
                math::ortho(half_w, half_h, 1.0, 600.0),
                math::translate(0.0, 0.0, -300.0),
            ),
            light_dir: [0.30, 0.62, 0.72, 0.0],
            params: [0.42, 0.22, -0.5, 0.0],
        }),
    );

    // Read-back buffer; wgpu requires 256-byte row alignment.
    let bpp = 4u32;
    let unpadded = width * bpp;
    let padded = unpadded.div_ceil(256) * 256;
    let out_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (padded * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
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
                view: &depth,
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
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_vertex_buffer(0, vbuf.slice(..));
        pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..frame.indices.len() as u32, 0, 0..1);

        if let (Some(nv), Some(ni)) = (&nvbuf, &nibuf) {
            pass.set_pipeline(&neuron_pipeline);
            pass.set_vertex_buffer(0, nv.slice(..));
            pass.set_index_buffer(ni.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..neuron_mesh.indices.len() as u32, 0, 0..1);
        }
    }
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &out_buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(enc.finish()));

    let slice = out_buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let data = slice.get_mapped_range().expect("map read-back buffer");

    // Composite over a checkerboard so transparency is visible in the PNG.
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    let mut opaque_px = 0u64;
    for y in 0..height {
        for x in 0..width {
            let si = (y * padded + x * bpp) as usize;
            let (r, g, b, a) = (
                data[si] as f32 / 255.0,
                data[si + 1] as f32 / 255.0,
                data[si + 2] as f32 / 255.0,
                data[si + 3] as f32 / 255.0,
            );
            if a > 0.02 {
                opaque_px += 1;
            }
            let check = if ((x / 16) + (y / 16)) % 2 == 0 {
                0.82
            } else {
                0.68
            };
            // Source is premultiplied, so: out = src + dst * (1 - a).
            let di = ((y * width + x) * 4) as usize;
            rgba[di] = (((r + check * (1.0 - a)).clamp(0.0, 1.0)) * 255.0) as u8;
            rgba[di + 1] = (((g + check * (1.0 - a)).clamp(0.0, 1.0)) * 255.0) as u8;
            rgba[di + 2] = (((b + check * (1.0 - a)).clamp(0.0, 1.0)) * 255.0) as u8;
            rgba[di + 3] = 255;
        }
    }
    drop(data);
    out_buf.unmap();
    println!("snapshot: {opaque_px} non-transparent pixels drawn");

    let file = std::fs::File::create(path).expect("create png");
    let w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .expect("png header")
        .write_image_data(&rgba)
        .expect("png data");
    println!("wrote {path}");
}
