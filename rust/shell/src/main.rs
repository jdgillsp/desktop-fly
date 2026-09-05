//! DesktopFly for Windows — the shell.
//!
//! Wires the four layers together (PORT_PLAN.md §1):
//!   dfplatform (senses) -> transduction -> dfcore (sim + body) -> render
//!
//! The overlay itself is Spike 0's, with its findings applied: DX12 pinned,
//! adapter limits, WS_EX_TOOLWINDOW set explicitly after every winit call and
//! re-asserted, and DirectComposition for per-pixel alpha.
//!
//! Run:  desktopfly [--seconds N] [--no-shadow] [--snapshot out.png]

mod flybody;
mod math;
mod mesh;
mod render;
mod snapshot;
mod transduction;

use std::sync::Arc;
use std::time::{Duration, Instant};

use dfcore::body::Fly;
use dfcore::env::{Rect, ScreenSpace, Senses};
use dfcore::{LifSim, SignalBuilder, Vec2};
use dfplatform::HostSenses;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId, WindowLevel};

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowAttributesExtWindows;

/// Spike 0 findings 3 and 4: winit's `with_skip_taskbar` is not
/// `WS_EX_TOOLWINDOW` (it leaves the window in Alt+Tab), and winit rewrites the
/// whole ex-style on calls like `set_cursor_hittest`, so this must run *after*
/// them and be re-asserted once the window is live.
#[cfg(target_os = "windows")]
fn harden_overlay_styles(window: &Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    let Ok(h) = window.window_handle() else { return };
    let RawWindowHandle::Win32(w) = h.as_raw() else {
        return;
    };
    let hwnd = HWND(w.hwnd.get() as *mut core::ffi::c_void);
    unsafe {
        let cur = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            cur | WS_EX_TOOLWINDOW.0 as isize | WS_EX_NOACTIVATE.0 as isize,
        );
    }
}

struct Args {
    seconds: u64,
    shadows: bool,
    /// Per-stage frame tracing. Earned its keep finding the QUNS_BUSY bug.
    diag: bool,
}

fn parse_args() -> Args {
    let a: Vec<String> = std::env::args().collect();
    Args {
        seconds: a
            .iter()
            .position(|x| x == "--seconds")
            .and_then(|i| a.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        shadows: !a.iter().any(|x| x == "--no-shadow"),
        diag: a.iter().any(|x| x == "--diag"),
    }
}

struct App {
    args: Args,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    renderer: Option<render::Renderer>,

    meshes: flybody::FlyMeshes,
    frame_mesh: mesh::Mesh,

    sim: Option<LifSim>,
    signals: SignalBuilder,
    fly: Fly,
    trans: transduction::Transduction,
    senses: HostSenses,
    space: ScreenSpace,

    ms_accumulator: f64,
    last_frame: Option<Instant>,
    last_sense: Instant,
    start: Instant,
    frames: u32,
    last_report: Instant,
    hardened: bool,
    hidden_for_fullscreen: bool,

    // Carried between the 30 Hz sense poll and the per-frame update.
    pending_tempo: f32,
    pending_sleepy: bool,
    last_env_cursor: Option<Vec2>,
}

impl App {
    fn new(args: Args) -> Self {
        let space = ScreenSpace::new(Rect::new(0, 0, 1920, 1080));
        App {
            args,
            window: None,
            surface: None,
            renderer: None,
            meshes: flybody::FlyMeshes::build(),
            frame_mesh: mesh::Mesh::default(),
            sim: None,
            signals: SignalBuilder::new(),
            fly: Fly::new(Vec2::ZERO, dfcore::DEFAULT_SEED),
            trans: transduction::Transduction::new(),
            senses: HostSenses::new(),
            space,
            ms_accumulator: 0.0,
            last_frame: None,
            last_sense: Instant::now(),
            start: Instant::now(),
            frames: 0,
            last_report: Instant::now(),
            hardened: false,
            hidden_for_fullscreen: false,
            pending_tempo: 1.0,
            pending_sleepy: false,
            last_env_cursor: None,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let monitor = event_loop
            .primary_monitor()
            .or_else(|| event_loop.available_monitors().next());
        let (pos, size) = match &monitor {
            Some(m) => (m.position(), m.size()),
            None => (
                winit::dpi::PhysicalPosition::new(0, 0),
                winit::dpi::PhysicalSize::new(1920, 1080),
            ),
        };
        self.space = ScreenSpace::new(Rect::new(
            pos.x,
            pos.y,
            pos.x + size.width as i32,
            pos.y + size.height as i32,
        ));

        let mut attrs = Window::default_attributes()
            .with_title("DesktopFly")
            .with_transparent(true)
            .with_decorations(false)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_position(pos)
            .with_inner_size(size);
        #[cfg(target_os = "windows")]
        {
            attrs = attrs.with_skip_taskbar(true).with_no_redirection_bitmap(true);
        }
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let _ = window.set_cursor_hittest(false);
        #[cfg(target_os = "windows")]
        harden_overlay_styles(&window);

        // Spike 0 finding 1: DX12 must be pinned. If wgpu is allowed to pick
        // Vulkan it will, and the surface reports alpha_modes: [Opaque].
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
                    presentation_system: wgpu::Dx12SwapchainKind::DxgiFromVisual,
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

        let renderer = render::Renderer::new(&adapter, &surface, size.width, size.height);
        if !renderer.alpha_ok {
            eprintln!(
                "WARNING: no premultiplied-alpha surface; the overlay will not be transparent"
            );
        }

        // The brain.
        match dfcore::data::load() {
            Ok(brain) => {
                let sim = LifSim::new(&brain.circuit, dfcore::DEFAULT_SEED);
                println!(
                    "FlyWire v783 - {} somas - circuit {}n/{}e",
                    brain.points.points.len(),
                    brain.circuit.neurons.len(),
                    brain.circuit.edges.len()
                );
                self.sim = Some(sim);
            }
            Err(e) => eprintln!("no brain data ({e}) - falling back to brainless behaviour"),
        }

        for note in self.senses.fidelity_notes() {
            println!("note: {note}");
        }

        let (w, h) = self.space.size();
        self.fly.pos = Vec2::new(w * 0.2, -h * 0.15);

        self.surface = Some(surface);
        self.renderer = Some(renderer);
        self.window = Some(window);
        self.start = Instant::now();
        self.last_report = Instant::now();
        println!("overlay up on {}x{} - the fly is loose", w as i32, h as i32);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(new) => {
                if let (Some(r), Some(s)) = (self.renderer.as_mut(), self.surface.as_ref()) {
                    r.resize(s, new.width, new.height);
                }
            }
            WindowEvent::RedrawRequested => {
                if self.args.diag && self.frames < 3 {
                    eprintln!("[diag] RedrawRequested #{}", self.frames);
                }
                self.tick(event_loop)
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _e: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

impl App {
    fn tick(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = match self.last_frame {
            Some(t) => (now - t).as_secs_f32().clamp(0.0, 0.05),
            None => {
                self.last_frame = Some(now);
                return;
            }
        };
        self.last_frame = Some(now);

        // Senses at ~30 Hz, as the macOS build's timer does (main.swift:761).
        if now - self.last_sense >= Duration::from_millis(33) {
            self.last_sense = now;
            let t0 = Instant::now();
            let env = self.senses.poll(&self.space);
            if self.args.diag && self.frames < 3 {
                eprintln!("[diag] senses.poll took {:?}", t0.elapsed());
            }

            // Yield to fullscreen games and presentations. New on Windows;
            // macOS's .fullScreenAuxiliary made it unnecessary there.
            if self.args.diag && self.frames < 3 {
                eprintln!("[diag] fullscreen_app_active = {}", env.fullscreen_app_active);
            }
            if env.fullscreen_app_active != self.hidden_for_fullscreen {
                self.hidden_for_fullscreen = env.fullscreen_app_active;
                if let Some(w) = &self.window {
                    w.set_visible(!self.hidden_for_fullscreen);
                }
            }

            self.fly.terrain = env.ledges.clone();

            if self.args.diag && self.frames < 3 { eprintln!("[diag] A: about to transduce"); }
            if let Some(sim) = self.sim.as_mut() {
                let (tempo, sleepy) = self.trans.apply(sim, &self.fly, &env, dt.max(1e-4));
                if self.args.diag && self.frames < 3 { eprintln!("[diag] B: transduced"); }
                self.pending_tempo = tempo;
                self.pending_sleepy = sleepy;
            }
            self.last_env_cursor = env.cursor;
        }

        // Step the brain at a true 1 kHz, decoupled from the frame rate.
        let mut signals = None;
        if let Some(sim) = self.sim.as_mut() {
            self.ms_accumulator += dt as f64 * 1000.0;
            let steps = (self.ms_accumulator as i64).min(50);
            self.ms_accumulator -= steps as f64;
            if self.args.diag && self.frames < 3 { eprintln!("[diag] C: stepping sim {steps} ms"); }
            sim.step(steps);
            if self.args.diag && self.frames < 3 { eprintln!("[diag] D: sim stepped"); }
            let mut s = self.signals.make(sim, dt);
            s.tempo = self.pending_tempo;
            s.sleep = self.pending_sleepy;
            signals = Some(s);
        }

        let bounds = self.space.size();
        self.fly
            .update(dt, bounds, self.last_env_cursor, signals);

        if self.args.diag && self.frames < 3 { eprintln!("[diag] E: body updated"); }
        let pose = self.fly.pose();
        flybody::build_frame(&mut self.frame_mesh, &self.meshes, &self.fly, &pose);
        if self.args.diag && self.frames < 3 { eprintln!("[diag] F: geometry built"); }

        if let (Some(r), Some(s)) = (self.renderer.as_mut(), self.surface.as_ref()) {
            if !self.hidden_for_fullscreen {
                let t0 = Instant::now();
                r.render(s, &self.frame_mesh, self.args.shadows);
                if self.args.diag && self.frames < 3 {
                    eprintln!(
                        "[diag] render took {:?}, {} verts, fly at ({:.0},{:.0})",
                        t0.elapsed(),
                        self.frame_mesh.verts.len(),
                        self.fly.pos.x,
                        self.fly.pos.y
                    );
                }
            }
        }

        // Spike 0 finding 4: re-assert the ex-style once the window is live.
        self.frames += 1;
        if !self.hardened && self.frames > 4 {
            self.hardened = true;
            #[cfg(target_os = "windows")]
            if let Some(w) = &self.window {
                harden_overlay_styles(w);
            }
        }
        if self.last_report.elapsed() >= Duration::from_secs(10) {
            let fps = self.frames as f32 / self.last_report.elapsed().as_secs_f32();
            println!(
                "fps {fps:.0}  state {:?}  pos ({:.0},{:.0})  ledges {}",
                self.fly.state,
                self.fly.pos.x,
                self.fly.pos.y,
                self.fly.terrain.len()
            );
            self.frames = 0;
            self.last_report = Instant::now();
        }

        if self.args.seconds > 0 && self.start.elapsed() >= Duration::from_secs(self.args.seconds) {
            event_loop.exit();
        }
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if let Some(i) = argv.iter().position(|a| a == "--snapshot") {
        let path = argv.get(i + 1).cloned().unwrap_or_else(|| "fly.png".into());
        let alt = argv
            .iter()
            .position(|a| a == "--alt")
            .and_then(|j| argv.get(j + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        snapshot::render_to_png(&path, 320, 320, alt, 20);
        return;
    }

    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(parse_args());
    event_loop.run_app(&mut app).expect("run");
}
