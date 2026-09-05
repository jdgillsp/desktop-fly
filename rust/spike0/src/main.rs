//! Spike 0 — prove the crux of the Windows port (PORT_PLAN.md §2, §7).
//!
//! Success criteria, all of which must hold simultaneously:
//!   1. transparent      — desktop shows through where nothing is drawn
//!   2. click-through    — mouse events reach the windows underneath
//!   3. always-on-top    — stays above normal windows
//!   4. no taskbar entry — it is a pet, not an app
//!   5. GPU-composited   — hardware path, not an UpdateLayeredWindow CPU blit
//!   6. cheap            — a few percent of one core at display refresh
//!
//! On Windows this runs through DirectComposition via wgpu's
//! `Dx12SwapchainKind::DxgiFromVisual`, which is the documented path that
//! supports transparent windows. `WS_EX_NOREDIRECTIONBITMAP`
//! (`with_no_redirection_bitmap`) is required alongside it.
//!
//! Run:  spike0 [--seconds N] [--hwnd] [--opaque-bg]
//!   --seconds N    exit after N seconds (default 15; 0 = run until closed)
//!   --hwnd         force the non-composition swapchain, to demonstrate the
//!                  failure this spike exists to rule out
//!   --opaque-bg    clear to solid magenta instead of transparent, to prove
//!                  the window really does cover the region being tested

use std::f32::consts::PI;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId, WindowLevel};

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowAttributesExtWindows;

/// winit's `with_skip_taskbar` uses `ITaskbarList::DeleteTab`, which removes the
/// taskbar button but leaves the window in the Alt+Tab list. A desktop pet should
/// be in neither. `WS_EX_TOOLWINDOW` is the flag that does both, and it is the
/// real equivalent of macOS's `setActivationPolicy(.accessory)` (main.swift:891).
/// `WS_EX_NOACTIVATE` additionally stops the overlay ever stealing focus.
///
/// Verified by `winprobe`: without this, `WS_EX_TOOLWINDOW` reads back unset.
#[cfg(target_os = "windows")]
fn harden_overlay_styles(window: &Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    let Ok(handle) = window.window_handle() else {
        eprintln!("could not get window handle; overlay styles not hardened");
        return;
    };
    let RawWindowHandle::Win32(h) = handle.as_raw() else {
        return;
    };
    let hwnd = HWND(h.hwnd.get() as *mut core::ffi::c_void);
    unsafe {
        let cur = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let want = cur | WS_EX_TOOLWINDOW.0 as isize | WS_EX_NOACTIVATE.0 as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, want);
        let now = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        println!("exstyle: 0x{cur:08X} -> 0x{now:08X} (+TOOLWINDOW +NOACTIVATE)");
    }
}

// ---------------------------------------------------------------------------
// Geometry — a rounded, non-vermin blob (see PORT_PLAN.md §6.1: the spike
// should look like something you would tolerate on your desktop).
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
}

/// UV-sphere, scaled per-axis into an ellipsoid.
fn ellipsoid(radius: [f32; 3], rings: u32, sectors: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut verts = Vec::new();
    let mut idx = Vec::new();
    for r in 0..=rings {
        let phi = PI * (r as f32) / (rings as f32);
        for s in 0..=sectors {
            let theta = 2.0 * PI * (s as f32) / (sectors as f32);
            let n = [
                phi.sin() * theta.cos(),
                phi.cos(),
                phi.sin() * theta.sin(),
            ];
            verts.push(Vertex {
                pos: [n[0] * radius[0], n[1] * radius[1], n[2] * radius[2]],
                // Correct normal for a scaled sphere is the inverse-transpose;
                // for an axis-aligned scale that is just a per-axis divide.
                normal: normalize([n[0] / radius[0], n[1] / radius[1], n[2] / radius[2]]),
            });
        }
    }
    let stride = sectors + 1;
    for r in 0..rings {
        for s in 0..sectors {
            let a = r * stride + s;
            let b = a + stride;
            idx.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    (verts, idx)
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}

// ---------------------------------------------------------------------------
// Minimal column-major 4x4 math (no glam dependency for a spike).
// ---------------------------------------------------------------------------

type Mat4 = [[f32; 4]; 4];

fn identity() -> Mat4 {
    let mut m = [[0.0; 4]; 4];
    for i in 0..4 {
        m[i][i] = 1.0;
    }
    m
}

fn mul(a: Mat4, b: Mat4) -> Mat4 {
    let mut o = [[0.0f32; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k][r] * b[c][k];
            }
            o[c][r] = s;
        }
    }
    o
}

fn translate(x: f32, y: f32, z: f32) -> Mat4 {
    let mut m = identity();
    m[3] = [x, y, z, 1.0];
    m
}

fn rotate_y(a: f32) -> Mat4 {
    let mut m = identity();
    let (s, c) = a.sin_cos();
    m[0][0] = c;
    m[0][2] = -s;
    m[2][0] = s;
    m[2][2] = c;
    m
}

fn rotate_x(a: f32) -> Mat4 {
    let mut m = identity();
    let (s, c) = a.sin_cos();
    m[1][1] = c;
    m[1][2] = s;
    m[2][1] = -s;
    m[2][2] = c;
    m
}

/// Orthographic projection, matching the SceneKit camera in main.swift:buildScene
/// (`usesOrthographicProjection = true`, `orthographicScale = height/2`).
fn ortho(half_w: f32, half_h: f32, near: f32, far: f32) -> Mat4 {
    let mut m = identity();
    m[0][0] = 1.0 / half_w;
    m[1][1] = 1.0 / half_h;
    m[2][2] = 1.0 / (near - far);
    m[3][2] = near / (near - far);
    m
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    mvp: Mat4,
    model: Mat4,
    tint: [f32; 4],
}

// ---------------------------------------------------------------------------

struct Args {
    seconds: u64,
    force_hwnd: bool,
    opaque_bg: bool,
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().collect();
    let seconds = argv
        .iter()
        .position(|a| a == "--seconds")
        .and_then(|i| argv.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(15);
    Args {
        seconds,
        force_hwnd: argv.iter().any(|a| a == "--hwnd"),
        opaque_bg: argv.iter().any(|a| a == "--opaque-bg"),
    }
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
    ubuf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    depth: wgpu::TextureView,
}

struct App {
    args: Args,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    start: Instant,
    frames: u32,
    last_report: Instant,
    reported_alpha: bool,
    hardened_again: bool,
}

impl App {
    fn new(args: Args) -> Self {
        Self {
            args,
            window: None,
            gpu: None,
            start: Instant::now(),
            frames: 0,
            last_report: Instant::now(),
            reported_alpha: false,
            hardened_again: false,
        }
    }
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

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        // Cover the primary monitor edge to edge, like the macOS overlay does
        // (main.swift:733 uses NSScreen.main.frame).
        let monitor = event_loop
            .primary_monitor()
            .or_else(|| event_loop.available_monitors().next());
        let (pos, size) = match &monitor {
            Some(m) => (m.position(), m.size()),
            None => (
                winit::dpi::PhysicalPosition::new(0, 0),
                winit::dpi::PhysicalSize::new(1280, 720),
            ),
        };
        println!(
            "monitor: origin ({}, {})  size {}x{}  scale {:.2}",
            pos.x,
            pos.y,
            size.width,
            size.height,
            monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0)
        );

        let mut attrs = Window::default_attributes()
            .with_title("DesktopFly spike0")
            .with_transparent(true)
            .with_decorations(false)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_position(pos)
            .with_inner_size(size);

        #[cfg(target_os = "windows")]
        {
            attrs = attrs.with_skip_taskbar(true);
            // Required for the DirectComposition path: the window must not have
            // a redirection bitmap for the composition swapchain to present.
            if !self.args.force_hwnd {
                attrs = attrs.with_no_redirection_bitmap(true);
            }
        }

        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));

        // Criterion 2: clicks pass straight through to whatever is underneath.
        // This is the direct equivalent of NSWindow.ignoresMouseEvents (main.swift:739).
        match window.set_cursor_hittest(false) {
            Ok(()) => println!("click-through: enabled (set_cursor_hittest(false))"),
            Err(e) => println!("click-through: FAILED — {e}"),
        }

        // MUST come after every winit window call: winit keeps its own cached
        // copy of GWL_EXSTYLE and rewrites the whole value on calls such as
        // set_cursor_hittest, silently dropping bits we set earlier. Verified
        // by winprobe reading the style back as unset when this ran first.
        #[cfg(target_os = "windows")]
        harden_overlay_styles(&window);

        let presentation = if self.args.force_hwnd {
            wgpu::Dx12SwapchainKind::DxgiFromHwnd
        } else {
            wgpu::Dx12SwapchainKind::DxgiFromVisual
        };
        println!("dx12 presentation system: {presentation:?}");

        // Pin DX12: the transparent-overlay path is `Dx12SwapchainKind::DxgiFromVisual`,
        // and if wgpu is allowed to pick Vulkan it will, silently reporting
        // `alpha_modes: [Opaque]` and defeating the whole spike.
        #[cfg(target_os = "windows")]
        let backends = wgpu::Backends::DX12;
        #[cfg(not(target_os = "windows"))]
        let backends = wgpu::Backends::PRIMARY;

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions {
                dx12: wgpu::Dx12BackendOptions {
                    presentation_system: presentation,
                    ..Default::default()
                },
                ..Default::default()
            },
            display: None,
        });

        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .expect("no adapter");

        let info = adapter.get_info();
        println!(
            "adapter: {} ({:?}, {:?})  driver: {}",
            info.name, info.backend, info.device_type, info.driver_info
        );

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("spike0"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .expect("request device");

        let caps = surface.get_capabilities(&adapter);
        println!("surface alpha modes offered: {:?}", caps.alpha_modes);

        // Criterion 1+5: we need a genuinely alpha-respecting composition mode.
        // Opaque means the spike has failed on this machine.
        let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Inherit) {
            wgpu::CompositeAlphaMode::Inherit
        } else {
            eprintln!(
                "SPIKE FAILURE: no alpha-respecting composite mode offered                  (got {:?}) — this backend cannot do a transparent overlay",
                caps.alpha_modes
            );
            wgpu::CompositeAlphaMode::Auto
        };
        println!("surface alpha mode chosen: {alpha_mode:?}");

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
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        println!("surface format: {format:?}  present mode: {:?}", config.present_mode);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("spike0 shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let (verts, indices) = ellipsoid([46.0, 30.0, 30.0], 32, 48);
        let index_count = indices.len() as u32;

        use wgpu::util::DeviceExt;
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let ubuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bgl"),
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
            label: Some("bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: ubuf.as_entire_binding(),
            }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // Premultiplied source blending — matches the shader output
                    // and what a composition swapchain expects.
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        let depth = make_depth(&device, &config);

        self.gpu = Some(Gpu {
            surface,
            device,
            queue,
            config,
            pipeline,
            vbuf,
            ibuf,
            index_count,
            ubuf,
            bind_group,
            depth,
        });
        self.window = Some(window);
        self.start = Instant::now();
        self.last_report = Instant::now();

        println!(
            "--- running {} — move a window under it, click through it, watch the taskbar ---",
            if self.args.seconds == 0 {
                "until closed".to_string()
            } else {
                format!("for {}s", self.args.seconds)
            }
        );
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(new) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.config.width = new.width.max(1);
                    gpu.config.height = new.height.max(1);
                    gpu.surface.configure(&gpu.device, &gpu.config);
                    gpu.depth = make_depth(&gpu.device, &gpu.config);
                }
            }
            WindowEvent::RedrawRequested => self.render(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

impl App {
    fn render(&mut self, event_loop: &ActiveEventLoop) {
        let Some(gpu) = &mut self.gpu else { return };

        let t = self.start.elapsed().as_secs_f32();

        // Drift the blob across the screen so it visibly crosses whatever is
        // underneath — that is what makes transparency obvious in a screenshot.
        let half_w = gpu.config.width as f32 / 2.0;
        let half_h = gpu.config.height as f32 / 2.0;
        let x = (t * 0.55).sin() * half_w * 0.55;
        let y = (t * 0.37).cos() * half_h * 0.35;

        let model = mul(translate(x, y, 0.0), mul(rotate_y(t * 0.9), rotate_x(0.35)));
        let proj = ortho(half_w, half_h, 1.0, 600.0);
        let view = translate(0.0, 0.0, -300.0);
        let mvp = mul(proj, mul(view, model));

        gpu.queue.write_buffer(
            &gpu.ubuf,
            0,
            bytemuck::bytes_of(&Uniforms {
                mvp,
                model,
                tint: [0.55, 0.78, 0.95, 0.92],
            }),
        );

        use wgpu::CurrentSurfaceTexture as Cst;
        let frame = match gpu.surface.get_current_texture() {
            Cst::Success(f) | Cst::Suboptimal(f) => f,
            Cst::Outdated | Cst::Lost => {
                // The compositor dropped the swapchain (display change, DPI
                // change, monitor unplug). Reconfigure and try again next frame.
                gpu.surface.configure(&gpu.device, &gpu.config);
                return;
            }
            Cst::Timeout | Cst::Occluded => return,
            Cst::Validation => {
                eprintln!("surface error: validation");
                return;
            }
        };
        let view_tex = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Criterion 1: clear to alpha 0. Everything not drawn must be the desktop.
        let clear = if self.args.opaque_bg {
            wgpu::Color { r: 1.0, g: 0.0, b: 1.0, a: 1.0 }
        } else {
            wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }
        };

        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view_tex,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &gpu.depth,
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
            pass.set_pipeline(&gpu.pipeline);
            pass.set_bind_group(0, &gpu.bind_group, &[]);
            pass.set_vertex_buffer(0, gpu.vbuf.slice(..));
            pass.set_index_buffer(gpu.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..gpu.index_count, 0, 0..1);
        }
        gpu.queue.submit(Some(enc.finish()));
        gpu.queue.present(frame);

        if !self.reported_alpha {
            self.reported_alpha = true;
            println!("first frame presented ok");
        }
        // Re-assert after the first presented frame; winit touches the style
        // during startup. In the real app this belongs in the ~1 Hz window poll.
        if !self.hardened_again && self.frames > 4 {
            self.hardened_again = true;
            #[cfg(target_os = "windows")]
            if let Some(w) = &self.window {
                harden_overlay_styles(w);
            }
        }

        self.frames += 1;
        if self.last_report.elapsed() >= Duration::from_secs(3) {
            let fps = self.frames as f32 / self.last_report.elapsed().as_secs_f32();
            println!("fps: {fps:.1}");
            self.frames = 0;
            self.last_report = Instant::now();
        }

        if self.args.seconds > 0 && self.start.elapsed() >= Duration::from_secs(self.args.seconds) {
            println!("--- spike window closing after {}s ---", self.args.seconds);
            event_loop.exit();
        }
    }
}

fn main() {
    let args = parse_args();
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(args);
    event_loop.run_app(&mut app).expect("run");
}
