//! DesktopFly for Windows — the shell.
//!
//! Wires the four layers together (PORT_PLAN.md §1):
//!   dfplatform (senses) -> transduction -> dfcore (sim + body) -> render
//!
//! The overlay itself is Spike 0's, with its findings applied: DX12 pinned,
//! adapter limits, WS_EX_TOOLWINDOW set explicitly after every winit call and
//! re-asserted, and DirectComposition for per-pixel alpha.
//!
//! Run:  desktopfly [--creature ID] [--seconds N] [--no-shadow] [--snapshot out.png]
//!
//! Which creature is running is the `Runtime`'s business (`runtime.rs`); this
//! file owns the overlay, the clocks, the tray and the brain window, and never
//! names a species.

mod brain;
mod flybody;
mod math;
mod mesh;
mod persist;
mod render;
mod runtime;
mod snapshot;
mod tray;
mod transduction;
mod wormbody;
mod wormrt;

use std::sync::Arc;
use std::time::{Duration, Instant};

use dfcore::env::{Rect, ScreenSpace, Senses};
use dfcore::Vec2;
use dfplatform::HostSenses;

use runtime::Runtime;

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
    /// Which creature: `--creature` wins, then the saved choice, then the fly.
    creature: String,
}

/// Resolve the creature id from the command line and the saved choice. An
/// unknown id — a typo, or a settings file from a build that had a creature
/// this one does not — falls back to the fly and says so, rather than
/// panicking on the way to the desktop.
fn creature_arg(a: &[String]) -> String {
    let asked = a
        .iter()
        .position(|x| x == "--creature")
        .and_then(|i| a.get(i + 1))
        .cloned()
        .or_else(persist::load_creature_choice)
        .unwrap_or_else(|| "drosophila".to_string());
    if dfcore::by_id(&asked).is_some() {
        asked
    } else {
        eprintln!(
            "unknown creature '{asked}' (have: {}) - running the fly",
            dfcore::CREATURE_IDS.join(", ")
        );
        "drosophila".to_string()
    }
}

fn parse_args() -> Args {
    let a: Vec<String> = std::env::args().collect();
    Args {
        creature: creature_arg(&a),
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
            // Floor of 20, not 5: `dt` is clamped to 50 ms as a stall guard
            // (inherited from the Swift build, which assumed 60 fps), so a frame
            // budget longer than that would silently run the creature in slow
            // motion — at --fps 5 it would move at a quarter speed rather than
            // simply drawing less often.
            .clamp(20, 240),
    }
}

struct App {
    args: Args,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    renderer: Option<render::Renderer>,

    /// The creature: brain, body, senses and geometry, behind one seam.
    rt: Box<dyn Runtime>,
    senses: HostSenses,
    space: ScreenSpace,

    last_frame: Option<Instant>,
    last_sense: Instant,
    start: Instant,
    frames: u32,
    last_report: Instant,
    last_harden: Instant,
    hidden_for_fullscreen: bool,

    /// Carried between the 30 Hz sense poll and the per-frame update.
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
        let rt = runtime::make(&args.creature, dfcore::DEFAULT_SEED);
        App {
            args,
            window: None,
            surface: None,
            renderer: None,
            rt,
            senses: HostSenses::new(),
            space,
            last_frame: None,
            last_sense: Instant::now(),
            start: Instant::now(),
            frames: 0,
            last_report: Instant::now(),
            last_harden: Instant::now(),
            hidden_for_fullscreen: false,
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

        // The brain was loaded with the creature, in `runtime::make`.
        self.gpu = Some((instance, adapter, device, queue));
        if !self.args.no_brain {
            self.open_brain_window(event_loop, pos, size);
        }

        self.tray = tray::Tray::new(&self.rt.brain_info(), self.rt.creature().id());
        if self.tray.is_none() {
            eprintln!("WARNING: no tray icon; quit with Task Manager");
        }

        // What the creature already knows about this user.
        *self.rt.habituation_mut() = persist::load(self.rt.creature().id());

        for note in self.senses.fidelity_notes() {
            println!("note: {note}");
        }

        let (w, h) = self.space.size();
        self.rt.place(Vec2::new(w * 0.2, -h * 0.15));

        self.surface = Some(surface);
        self.renderer = Some(renderer);
        self.window = Some(window);
        self.start = Instant::now();
        self.last_report = Instant::now();
        println!(
            "overlay up on {}x{} - the {} is loose",
            w as i32,
            h as i32,
            self.rt.creature().display_name().to_lowercase()
        );
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
                    if let (Some(b), Some(sim)) = (self.brain.as_mut(), self.rt.sim()) {
                        b.update(bdt, sim);
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
                    let name = self.rt.creature().display_name();
                    if let (Some(b), Some(sim), Some((cx, cy))) = (
                        self.brain.as_mut(),
                        self.rt.sim_mut(),
                        self.brain_cursor,
                    ) {
                        if let Some(label) = b.handle_click(cx, cy, sim) {
                            println!("stimulating {label}");
                            if let Some(w) = &self.brain_window {
                                w.set_title(&format!("{name} Brain - {label}"));
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
        let (Some(sim), Some(pts)) = (self.rt.sim(), self.rt.brain_points()) else {
            return;
        };
        let (bw, bh) = (420u32, 340u32);
        let title = format!(
            "{} Brain - {} (click = stimulate)",
            self.rt.creature().display_name(),
            self.rt.creature().provenance().describe()
        );
        let attrs = Window::default_attributes()
            .with_title(title)
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
                            device, queue, adapter, bs, pts, sim, bw, bh,
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
        self.rt.moved_display(self.space.size());
        println!("moved to display {} ({}x{})", self.monitor_index, size.width, size.height);
    }

    /// Swap the running creature for another, in place: the new animal appears
    /// where the old one was, with its own habituation history, and the brain
    /// window is rebuilt for its data (or closed if it has none).
    fn switch_creature(&mut self, event_loop: &ActiveEventLoop, id: &str) {
        if id == self.rt.creature().id() {
            return;
        }
        persist::save(self.rt.creature().id(), self.rt.habituation());
        let at = self.rt.position();
        let mut rt = runtime::make(id, dfcore::DEFAULT_SEED);
        *rt.habituation_mut() = persist::load(id);
        rt.place(at);
        self.rt = rt;
        persist::save_creature_choice(id);

        let had_brain = self.brain.is_some();
        self.brain = None;
        self.brain_window = None;
        if had_brain {
            let (pos, size, _) = self.monitors[self.monitor_index];
            self.open_brain_window(event_loop, pos, size);
        }
        if let Some(t) = &self.tray {
            t.set_creature(id, &self.rt.brain_info());
            t.set_mood(&self.rt.habituation().describe());
        }
        self.last_mood.clear();
        println!(
            "now running the {} ({})",
            self.rt.creature().display_name().to_lowercase(),
            self.rt.brain_info()
        );
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
                    persist::save(self.rt.creature().id(), self.rt.habituation());
                    event_loop.exit();
                    return;
                }
                tray::TrayCommand::SelectCreature(id) => self.switch_creature(event_loop, id),
                tray::TrayCommand::TogglePause => {
                    self.paused = !self.paused;
                    if let Some(t) = &self.tray {
                        t.set_paused(self.paused);
                    }
                }
                tray::TrayCommand::EscapeTest | tray::TrayCommand::Scare => {
                    // A real stimulus into the real circuit, not a scripted
                    // takeoff: the fly flees only if its giant fiber fires.
                    self.rt.scare();
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
                    self.rt.habituation_mut().reset();
                    persist::forget(self.rt.creature().id());
                    println!("habituation reset - the creature is naive again");
                }
            }
        }
        if self.paused {
            return;
        }

        // Senses at ~30 Hz, as the macOS build's timer does (main.swift:761).
        if now - self.last_sense >= Duration::from_millis(33) {
            // Transduction must be given the SENSE interval, not the frame
            // interval. Cursor velocity is (position delta / dt) between
            // consecutive sense polls, so handing it the frame dt overestimates
            // speed by the ratio of the two rates — at --fps 60 the fly reads
            // every cursor movement as twice as fast and flees twice as
            // readily. Frame rate must not change behaviour.
            let sense_dt = (now - self.last_sense).as_secs_f32().clamp(1e-4, 0.5);
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

            if self.args.diag && self.frames < 3 { eprintln!("[diag] A: about to transduce"); }
            self.rt.sense(&env, sense_dt);
            if self.args.diag && self.frames < 3 { eprintln!("[diag] B: transduced"); }
            self.last_env_cursor = env.cursor;
        }

        // The brain at a true 1 kHz, decoupled from the frame rate, then the
        // body once — both inside the runtime, whichever creature it is.
        if self.args.diag && self.frames < 3 { eprintln!("[diag] C: stepping creature"); }
        let bounds = self.space.size();
        self.rt.tick(dt, bounds, self.last_env_cursor);
        if self.args.diag && self.frames < 3 { eprintln!("[diag] E: body updated"); }

        let diag = self.args.diag && self.frames < 3;
        let (glass, shadows) = (self.args.glass, self.args.shadows);
        let hidden = self.hidden_for_fullscreen;
        let at = self.rt.position();
        let geometry = self.rt.build(glass);
        if diag { eprintln!("[diag] F: geometry built"); }

        if let (Some(r), Some(s)) = (self.renderer.as_mut(), self.surface.as_ref()) {
            if !hidden {
                let t0 = Instant::now();
                r.render(s, geometry.body, geometry.neurons, shadows);
                if diag {
                    eprintln!(
                        "[diag] render took {:?}, {} verts, creature at ({:.0},{:.0})",
                        t0.elapsed(),
                        geometry.body.verts.len(),
                        at.x,
                        at.y
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
        let mood = self.rt.habituation().describe();
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
            persist::save(self.rt.creature().id(), self.rt.habituation());
        }

        if self.last_report.elapsed() >= Duration::from_secs(10) {
            let fps = self.frames as f32 / self.last_report.elapsed().as_secs_f32();
            println!("fps {fps:.0}  {}", self.rt.status());
            self.frames = 0;
            self.last_report = Instant::now();
        }

        if self.args.seconds > 0 && self.start.elapsed() >= Duration::from_secs(self.args.seconds) {
            persist::save(self.rt.creature().id(), self.rt.habituation());
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
        let mut rt = runtime::make(&creature_arg(&argv), dfcore::DEFAULT_SEED);
        snapshot::render_to_png(rt.as_mut(), &path, 320, 320, alt, 20, glass);
        return;
    }

    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(parse_args());
    event_loop.run_app(&mut app).expect("run");
}
