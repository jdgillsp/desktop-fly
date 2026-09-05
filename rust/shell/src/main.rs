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

mod brain;
mod flybody;
mod math;
mod mesh;
mod persist;
mod render;
mod snapshot;
mod tray;
mod transduction;

use std::sync::Arc;
use std::time::{Duration, Instant};

use dfcore::body::Fly;
use dfcore::env::{Rect, ScreenSpace, Senses};
use dfcore::{LifSim, SignalBuilder, Sim, Vec2};
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
    no_brain: bool,
    /// Glass anatomy is the default register (PORT_PLAN.md §6.3 #2);
    /// --literal restores the photoreal fly.
    glass: bool,
    /// Frame cap. A background pet does not need 60 fps, and Spike 0 measured
    /// ~8% of a core just to clear and present a full-screen overlay -- so the
    /// frame rate is most of the idle cost. See `--fps`.
    fps: u32,
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
        no_brain: a.iter().any(|x| x == "--no-brain"),
        glass: !a.iter().any(|x| x == "--literal"),
        fps: a
            .iter()
            .position(|x| x == "--fps")
            .and_then(|i| a.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(30)
            .clamp(5, 240),
    }
}

struct App {
    args: Args,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    renderer: Option<render::Renderer>,

    meshes: flybody::FlyMeshes,
    frame_mesh: mesh::Mesh,
    neuron_mesh: mesh::Mesh,
    /// Per-neuron flash brightness for the glass body, decayed each frame.
    body_flash: Vec<f32>,

    sim: Option<LifSim>,
    brain_points: Option<dfcore::data::BrainPointsFile>,
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
    last_harden: Instant,
    hidden_for_fullscreen: bool,

    // Carried between the 30 Hz sense poll and the per-frame update.
    pending_tempo: f32,
    pending_sleepy: bool,
    last_env_cursor: Option<Vec2>,

    gpu: Option<(wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue)>,
    brain: Option<brain::BrainView>,
    brain_window: Option<Arc<Window>>,
    brain_cursor: Option<(f32, f32)>,
    brain_last_frame: Option<Instant>,
    tray: Option<tray::Tray>,
    paused: bool,
    last_mood: String,
    last_state_save: Instant,
    /// Monitors, for the "Move to Next Display" command.
    monitors: Vec<(winit::dpi::PhysicalPosition<i32>, winit::dpi::PhysicalSize<u32>, f64)>,
    monitor_index: usize,
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
            neuron_mesh: mesh::Mesh::default(),
            body_flash: Vec::new(),
            sim: None,
            brain_points: None,
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
            last_harden: Instant::now(),
            hidden_for_fullscreen: false,
            pending_tempo: 1.0,
            pending_sleepy: false,
            last_env_cursor: None,
            gpu: None,
            brain: None,
            brain_window: None,
            brain_cursor: None,
            brain_last_frame: None,
            tray: None,
            paused: false,
            last_mood: String::new(),
            last_state_save: Instant::now(),
            monitors: Vec::new(),
            monitor_index: 0,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        self.monitors = event_loop
            .available_monitors()
            .map(|m| (m.position(), m.size(), m.scale_factor()))
            .collect();
        let monitor = event_loop
            .primary_monitor()
            .or_else(|| event_loop.available_monitors().next());
        if let Some(m) = &monitor {
            self.monitor_index = self
                .monitors
                .iter()
                .position(|(p, _, _)| *p == m.position())
                .unwrap_or(0);
        }
        let (pos, size, scale) = match &monitor {
            Some(m) => (m.position(), m.size(), m.scale_factor()),
            None => (
                winit::dpi::PhysicalPosition::new(0, 0),
                winit::dpi::PhysicalSize::new(1920, 1080),
                1.0,
            ),
        };
        // Scene units are logical, so the creature keeps a constant apparent
        // size across displays with different scaling (PORT_PLAN.md §8 dec. 4).
        self.space = ScreenSpace::with_scale(
            Rect::new(
                pos.x,
                pos.y,
                pos.x + size.width as i32,
                pos.y + size.height as i32,
            ),
            scale as f32,
        );

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

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("desktopfly"),
            required_features: wgpu::Features::empty(),
            // Spike 0 finding 2: downlevel_defaults caps textures at 2048,
            // smaller than an ordinary monitor.
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .expect("request device");

        let mut renderer = render::Renderer::new(
            device.clone(),
            queue.clone(),
            &adapter,
            &surface,
            size.width,
            size.height,
        );
        renderer.scale = scale as f32;
        if !renderer.alpha_ok {
            eprintln!(
                "WARNING: no premultiplied-alpha surface; the overlay will not be transparent"
            );
        }

        // The brain.
        match dfcore::data::load() {
            Ok(brain) => {
                let mut sim = LifSim::new(&brain.circuit, dfcore::DEFAULT_SEED);
                // What the creature already knows about this user.
                sim.habituation = persist::load();
                // The brain window flashes spikes where they actually happen.
                sim.collect_spikes = true;
                let sim_n = sim.n;
                println!(
                    "FlyWire v783 - {} somas - circuit {}n/{}e",
                    brain.points.points.len(),
                    brain.circuit.neurons.len(),
                    brain.circuit.edges.len()
                );
                self.sim = Some(sim);
                self.body_flash = vec![0.0; sim_n];
                self.brain_points = Some(brain.points);
            }
            Err(e) => eprintln!("no brain data ({e}) - falling back to brainless behaviour"),
        }

        self.gpu = Some((instance, adapter, device, queue));
        if !self.args.no_brain {
            self.open_brain_window(event_loop, pos, size);
        }

        let info = match &self.sim {
            Some(sim) => format!("FlyWire v783 - circuit {}n", sim.n),
            None => "no data - run etl.py".to_string(),
        };
        self.tray = tray::Tray::new(&info);
        if self.tray.is_none() {
            eprintln!("WARNING: no tray icon; quit with Task Manager");
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

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // The brain window is a separate, ordinary window; route its events here
        // before the overlay's.
        let is_brain = self
            .brain_window
            .as_ref()
            .map(|w| w.id() == id)
            .unwrap_or(false);
        if is_brain {
            match event {
                WindowEvent::CloseRequested => {
                    // Closing the brain window must not quit the app, only hide
                    // the view — the same as the macOS panel's close button.
                    self.brain = None;
                    self.brain_window = None;
                }
                WindowEvent::Resized(new) => {
                    if let Some(b) = self.brain.as_mut() {
                        b.resize(new.width, new.height);
                    }
                }
                WindowEvent::RedrawRequested => {
                    // MUST draw here. A RedrawRequested that returns without
                    // presenting leaves the update region invalid, Windows
                    // re-posts WM_PAINT immediately, and the message pump
                    // starves every other window — which is exactly what
                    // stopped the overlay from ever rendering.
                    let now = Instant::now();
                    let bdt = self
                        .brain_last_frame
                        .map(|t| (now - t).as_secs_f32().clamp(0.0, 0.1))
                        .unwrap_or(1.0 / 60.0);
                    self.brain_last_frame = Some(now);
                    if let (Some(b), Some(sim)) = (self.brain.as_mut(), self.sim.as_ref()) {
                        b.update(bdt, sim as &dyn Sim);
                        b.render();
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    self.brain_cursor = Some((position.x as f32, position.y as f32));
                    // Hovering holds the rotation still so you can aim.
                    if let Some(b) = self.brain.as_mut() {
                        b.paused_by_hover = true;
                    }
                }
                WindowEvent::CursorLeft { .. } => {
                    self.brain_cursor = None;
                    if let Some(b) = self.brain.as_mut() {
                        b.paused_by_hover = false;
                    }
                }
                WindowEvent::MouseInput {
                    state: winit::event::ElementState::Pressed,
                    button: winit::event::MouseButton::Left,
                    ..
                } => {
                    if let (Some(b), Some(sim), Some((cx, cy))) = (
                        self.brain.as_mut(),
                        self.sim.as_mut(),
                        self.brain_cursor,
                    ) {
                        if let Some(label) = b.handle_click(cx, cy, sim as &mut dyn Sim) {
                            println!("stimulating {label}");
                            if let Some(w) = &self.brain_window {
                                w.set_title(&format!("Fly Brain - {label}"));
                            }
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(new) => {
                if let (Some(r), Some(s)) = (self.renderer.as_mut(), self.surface.as_ref()) {
                    r.resize(s, new.width, new.height);
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // Dragging to a differently-scaled monitor, or the user changing
                // display scaling while the pet is running.
                self.space = ScreenSpace::with_scale(self.space.display, scale_factor as f32);
                if let Some(r) = self.renderer.as_mut() {
                    r.scale = scale_factor as f32;
                }
                println!("display scale is now {scale_factor:.2}");
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

    fn about_to_wait(&mut self, e: &ActiveEventLoop) {
        // Sleep until the next frame is due rather than polling flat out. With
        // ControlFlow::Poll the loop spins between presents; a pet that sits on
        // the desktop all day should be cheap when nothing is happening.
        let budget = Duration::from_secs_f32(1.0 / self.args.fps as f32);
        let due = self
            .last_frame
            .map(|t| t.elapsed() >= budget)
            .unwrap_or(true);
        if due {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        } else if let Some(t) = self.last_frame {
            e.set_control_flow(ControlFlow::WaitUntil(t + budget));
        }
        // Throttle the brain window to ~30 Hz. Requesting a redraw every
        // iteration keeps its update region permanently invalid, which floods
        // the message pump and starves the overlay.
        if let Some(w) = &self.brain_window {
            let due = self
                .brain_last_frame
                .map(|t| t.elapsed() >= Duration::from_millis(33))
                .unwrap_or(true);
            if due {
                w.request_redraw();
            }
        }
    }
}

impl App {
    /// Open (or reopen) the brain window. Separate from `resumed` so the tray's
    /// Show/Hide Brain can bring it back after the user closes it.
    fn open_brain_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        pos: winit::dpi::PhysicalPosition<i32>,
        size: winit::dpi::PhysicalSize<u32>,
    ) {
        let Some((instance, adapter, device, queue)) = self.gpu.as_ref() else {
            return;
        };
        let (Some(sim), Some(pts)) = (self.sim.as_ref(), self.brain_points.as_ref()) else {
            return;
        };
        let (bw, bh) = (420u32, 340u32);
        let attrs = Window::default_attributes()
            .with_title("Fly Brain - FlyWire v783 (click = stimulate)")
            .with_inner_size(winit::dpi::PhysicalSize::new(bw, bh))
            .with_position(winit::dpi::PhysicalPosition::new(
                pos.x + size.width as i32 - bw as i32 - 24,
                pos.y + size.height as i32 - bh as i32 - 80,
            ))
            .with_window_level(WindowLevel::AlwaysOnTop);
        match event_loop.create_window(attrs) {
            Ok(w) => {
                let w = Arc::new(w);
                match instance.create_surface(w.clone()) {
                    Ok(bs) => {
                        self.brain = Some(brain::BrainView::new(
                            device,
                            queue,
                            adapter,
                            bs,
                            pts,
                            sim as &dyn Sim,
                            bw,
                            bh,
                        ));
                        self.brain_window = Some(w);
                        println!("brain window open: {} somas", pts.points.len());
                    }
                    Err(e) => eprintln!("no brain surface: {e}"),
                }
            }
            Err(e) => eprintln!("no brain window: {e}"),
        }
    }

    /// Hop the creature to the next monitor, as the macOS build's
    /// "Move to Next Display" does (main.swift:822).
    fn move_to_next_display(&mut self) {
        if self.monitors.len() < 2 {
            return;
        }
        self.monitor_index = (self.monitor_index + 1) % self.monitors.len();
        let (pos, size, scale) = self.monitors[self.monitor_index];
        // Monitors can differ in DPI, so the scale travels with the creature.
        self.space = ScreenSpace::with_scale(
            Rect::new(
                pos.x,
                pos.y,
                pos.x + size.width as i32,
                pos.y + size.height as i32,
            ),
            scale as f32,
        );
        if let Some(w) = &self.window {
            w.set_outer_position(pos);
            let _ = w.request_inner_size(size);
        }
        if let (Some(r), Some(s)) = (self.renderer.as_mut(), self.surface.as_ref()) {
            r.scale = scale as f32;
            r.resize(s, size.width, size.height);
        }
        // Terrain is stale until the next poll, and the fly must land inside
        // the new display.
        self.fly.terrain.clear();
        self.fly.ledge = None;
        let (w, h) = self.space.size();
        self.fly.pos.x = self.fly.pos.x.clamp(-w / 2.0 + 40.0, w / 2.0 - 40.0);
        self.fly.pos.y = self.fly.pos.y.clamp(-h / 2.0 + 40.0, h / 2.0 - 40.0);
        println!("moved to display {} ({}x{})", self.monitor_index, size.width, size.height);
    }

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

        // Drain first so the tray borrow ends before anything mutates self.
        let commands = self.tray.as_ref().map(|t| t.poll()).unwrap_or_default();
        for cmd in commands {
            match cmd {
                tray::TrayCommand::Quit => {
                    if let Some(sim) = self.sim.as_ref() {
                        persist::save(&sim.habituation);
                    }
                    event_loop.exit();
                    return;
                }
                tray::TrayCommand::TogglePause => {
                    self.paused = !self.paused;
                    if let Some(t) = &self.tray {
                        t.set_paused(self.paused);
                    }
                }
                tray::TrayCommand::EscapeTest | tray::TrayCommand::Scare => {
                    // A real stimulus into the real circuit, not a scripted
                    // takeoff: the fly flees only if its giant fiber fires.
                    self.trans.trigger_scare();
                }
                tray::TrayCommand::ToggleShadows => self.args.shadows = !self.args.shadows,
                tray::TrayCommand::NextDisplay => self.move_to_next_display(),
                tray::TrayCommand::ToggleBrain => {
                    if self.brain.is_some() {
                        self.brain = None;
                        self.brain_window = None;
                    } else {
                        let (pos, size, _) = self.monitors[self.monitor_index];
                        self.open_brain_window(event_loop, pos, size);
                    }
                }
                tray::TrayCommand::ForgetMe => {
                    if let Some(sim) = self.sim.as_mut() {
                        sim.habituation.reset();
                    }
                    persist::forget();
                    println!("habituation reset - the creature is naive again");
                }
            }
        }
        if self.paused {
            return;
        }

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
        flybody::build_frame(
            &mut self.frame_mesh,
            &self.meshes,
            &self.fly,
            &pose,
            self.args.glass,
        );

        // The connectome inside the glass shell: decay, then light up whatever
        // just spiked. Same data the brain window draws, on the creature itself.
        let mut has_neurons = false;
        if self.args.glass {
            if let Some(sim) = self.sim.as_ref() {
                let decay = (-dt * 7.0).exp();
                for f in self.body_flash.iter_mut() {
                    *f *= decay;
                }
                for ev in &sim.last_spikes {
                    if ev.neuron < self.body_flash.len() {
                        self.body_flash[ev.neuron] = if ev.is_gf { 2.5 } else { 1.0 };
                    }
                }
                flybody::build_neuron_field(
                    &mut self.neuron_mesh,
                    sim,
                    &self.body_flash,
                    &self.fly,
                    &pose,
                );
                has_neurons = true;
            }
        }
        if self.args.diag && self.frames < 3 { eprintln!("[diag] F: geometry built"); }

        if let (Some(r), Some(s)) = (self.renderer.as_mut(), self.surface.as_ref()) {
            if !self.hidden_for_fullscreen {
                let t0 = Instant::now();
                let neurons = if has_neurons {
                    Some(&self.neuron_mesh)
                } else {
                    None
                };
                r.render(s, &self.frame_mesh, neurons, self.args.shadows);
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

        // Spike 0 finding 4: winit owns GWL_EXSTYLE and rewrites the whole value
        // on calls such as set_cursor_hittest and set_visible — and the
        // fullscreen-yield path calls set_visible. Asserting once at startup is
        // not enough; re-assert on the same ~1 Hz cadence as the window poll.
        self.frames += 1;
        if self.last_harden.elapsed() >= Duration::from_secs(1) {
            self.last_harden = Instant::now();
            #[cfg(target_os = "windows")]
            if let Some(w) = &self.window {
                harden_overlay_styles(w);
            }
        }
        if let Some(sim) = self.sim.as_ref() {
            let mood = sim.habituation.describe();
            if mood != self.last_mood {
                self.last_mood = mood.clone();
                if let Some(t) = &self.tray {
                    t.set_mood(&mood);
                }
            }
            // Checkpoint every 60 s so a crash or a kill does not lose days of
            // accumulated familiarity.
            if self.last_state_save.elapsed() >= Duration::from_secs(60) {
                self.last_state_save = Instant::now();
                persist::save(&sim.habituation);
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
            if let Some(sim) = self.sim.as_ref() {
                persist::save(&sim.habituation);
            }
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
        let glass = !argv.iter().any(|a| a == "--literal");
        snapshot::render_to_png(&path, 320, 320, alt, 20, glass);
        return;
    }

    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(parse_args());
    event_loop.run_app(&mut app).expect("run");
}
