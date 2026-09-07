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
mod camera;
mod flybody;
mod habitatmesh;
mod koibody;
mod koirt;
mod math;
mod mesh;
mod persist;
mod render;
mod runtime;
mod snapshot;
mod spiderbody;
mod spiderrt;
mod weaverbody;
mod weaverrt;
mod tray;
mod transduction;
mod wormbody;
mod wormrt;

use std::sync::Arc;
use std::time::{Duration, Instant};

use dfcore::env::{Rect, ScreenSpace, Senses};
use camera::{Camera, HabitatView};
use dfcore::{Habitat, HabitatChord, HabitatKind, PropKind, Region, Vec2};
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
    /// Confine the creature to a rendered enclosure.
    habitat: bool,
    /// Whether the user ever said so (flag or saved choice). If not, the
    /// creature's own preference decides: a web builder starts in a tank.
    habitat_explicit: bool,
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
        // --literal wins for one run; otherwise the tray's saved toggle;
        // otherwise glass, the default register.
        glass: if a.iter().any(|x| x == "--literal") {
            false
        } else {
            persist::load_glass_choice().unwrap_or(true)
        },
        // Off unless asked for: free roam stays the default, and an existing
        // install is unaffected by this feature existing.
        habitat: a.iter().any(|x| x == "--habitat") || persist::load_habitat_choice().unwrap_or(false),
        habitat_explicit: a.iter().any(|x| x == "--habitat") || persist::load_habitat_choice().is_some(),
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

    /// The enclosure, when there is one. `None` is free roam, and then every
    /// region the creature sees is `Region::centered(display)` — bit-identical
    /// to the arithmetic the bodies did before habitats existed.
    habitat: Option<Habitat>,
    /// This frame's composed geometry: the tank behind, the creature, then the
    /// front glass. One mesh and one draw, so the renderer needs no extra
    /// buffers — index order is draw order.
    frame_mesh: mesh::Mesh,
    /// Scratch for the front pane, built separately because it has to be
    /// appended *after* the creature.
    front_mesh: mesh::Mesh,
    /// The grab chord is down and the user is handling the enclosure — moving
    /// it, turning it or resizing it. Drives the rim highlight, so every
    /// adjustment gives the same "you have hold of it" feedback.
    grabbing: bool,
    /// How the user has angled and sized the enclosure. Persisted.
    view: HabitatView,
    /// The chord held this frame and last, and where the pointer was when the
    /// last frame ran — in **screen** units, not ground ones.
    ///
    /// Screen matters: the orbit chord changes the camera, and the ground
    /// position of a stationary pointer changes with it. Feeding a ground delta
    /// back into the camera would be a loop that runs away the moment the user
    /// stops moving the mouse.
    chord: HabitatChord,
    prev_chord: HabitatChord,
    last_screen_cursor: Option<Vec2>,
    chord_from: Option<Vec2>,

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
    fn new(mut args: Args) -> Self {
        let space = ScreenSpace::new(Rect::new(0, 0, 1920, 1080));
        let rt = runtime::make(&args.creature, dfcore::DEFAULT_SEED);
        if !args.habitat_explicit && rt.prefers_habitat() {
            args.habitat = true;
        }
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
            habitat: None,
            frame_mesh: mesh::Mesh::default(),
            front_mesh: mesh::Mesh::default(),
            grabbing: false,
            view: {
                let mut v = HabitatView::default();
                if let Some((pitch, yaw, zoom)) = persist::load_habitat_view() {
                    v = HabitatView { pitch, yaw, zoom };
                }
                v
            },
            chord: HabitatChord::None,
            prev_chord: HabitatChord::None,
            last_screen_cursor: None,
            chord_from: None,
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

        self.tray = tray::Tray::new(
            &self.rt.brain_info(),
            self.rt.creature().id(),
            self.args.glass,
            self.args.habitat,
        );
        if self.tray.is_none() {
            eprintln!("WARNING: no tray icon; quit with Task Manager");
        }
        self.refresh_habitat_menu();

        // What the creature already knows about this user.
        *self.rt.habituation_mut() = persist::load(self.rt.creature().id());

        for note in self.senses.fidelity_notes() {
            println!("note: {note}");
        }

        // The enclosure exists from the first frame if it was asked for, and
        // the creature starts inside it rather than swimming in from off-stage.
        self.rebuild_habitat(false);
        let (w, h) = self.space.size();
        match &self.habitat {
            Some(h) => {
                self.rt.place(h.region.center);
                println!("habitat: {} at ({:.0},{:.0}), {:.0}x{:.0}",
                    h.kind.label(), h.region.center.x, h.region.center.y,
                    h.region.size.0, h.region.size.1);
            }
            None => self.rt.place(Vec2::new(w * 0.2, -h * 0.15)),
        }

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

    /// Where the camera is. Straight down in free roam, which is what keeps
    /// the creature registered to real window edges; tilted inside an
    /// enclosure, where nothing is registered to anything (camera.rs).
    fn camera(&self) -> Camera {
        match self.habitat {
            Some(_) => self.view.camera(),
            None => Camera::TopDown,
        }
    }

    /// The world's edge this frame: the enclosure if there is one, otherwise
    /// the whole display. Every other part of the app asks this rather than
    /// reaching for `space.size()`, so there is one answer to "where does the
    /// world end" and not two that can drift apart.
    fn region(&self) -> Region {
        match &self.habitat {
            Some(h) => h.region,
            None => Region::centered(self.space.size()),
        }
    }

    /// Build, move or drop the enclosure to match the current setting, the
    /// current creature, the current display and the current view.
    ///
    /// `keep_place` is the difference between "the world changed under the
    /// tank" and "the user is adjusting the tank". A display switch should park
    /// it back in its corner; turning or resizing it must not teleport it out
    /// from under the pointer that is doing the turning.
    fn rebuild_habitat(&mut self, keep_place: bool) {
        if !self.args.habitat {
            self.habitat = None;
            return;
        }
        let kind = HabitatKind::for_substrate(self.rt.substrate());
        // The tank is placed by where it will *appear*, not by its ground
        // rectangle: under a tilted, yawed camera those are different shapes,
        // and a ground-space placement would hang a corner off the screen.
        let c = self.view.camera();
        let display = self.space.size();
        // Each enclosure has its own proportions — a vivarium is narrow for
        // its height, a pond a little wider than a tank — applied to one
        // display-relative default, then to the user's zoom. The zoom is
        // applied *before* `fit_size`, so the top of its range means "as large
        // as this display allows" rather than "off the edge".
        let (fw, fh) = habitatmesh::footprint(kind);
        let z = self.view.clamped_zoom();
        let want = (
            (display.0 * 0.40).clamp(320.0, 720.0) * fw * z,
            (display.1 * 0.44).clamp(240.0, 520.0) * fh * z,
        );
        let (lo, hi) = (habitatmesh::FLOOR_Z, habitatmesh::top_z(kind));
        let size = c.fit_size(display, want, lo, hi, 24.0);
        let region = match (keep_place, &self.habitat) {
            // Hold the centre, but re-clamp: a turn or a growth changes the
            // tank's screen footprint, and a tank that was against the edge
            // would otherwise climb off it.
            (true, Some(h)) => {
                c.clamp_on_screen(Region::new(h.region.center, size), display, lo, hi, 24.0)
            }
            _ => c.place_lower_right(display, size, lo, hi, 24.0),
        };
        match &mut self.habitat {
            // Same kind: keep the props where they are and just move the tank,
            // so switching displays does not reset a ball the user watched the
            // creature push into a corner.
            Some(h) if h.kind == kind => h.reshape(region),
            _ => {
                let mut h = Habitat::new(kind, region, dfcore::DEFAULT_SEED);
                // A tank the user has arranged comes back as they left it.
                if let Some(saved) = persist::load_habitat_contents(kind.slug()) {
                    h.restore(&saved);
                }
                self.habitat = Some(h);
            }
        }
    }

    /// Tell the tray what the view is and what the enclosure could hold. Both
    /// depend on the running creature, so this is called wherever the tank is.
    fn refresh_habitat_menu(&self) {
        let Some(t) = &self.tray else { return };
        t.set_view(
            &if self.args.habitat {
                self.view.describe()
            } else {
                "habitat off".to_string()
            },
            self.view.is_default(),
        );
        let slots: Vec<(String, bool, bool)> = match &self.habitat {
            Some(h) => PropKind::catalogue(h.kind)
                .iter()
                .map(|k| {
                    (
                        k.label().to_lowercase(),
                        h.can_add(*k),
                        h.count(*k) > 0,
                    )
                })
                .collect(),
            None => Vec::new(),
        };
        t.set_contents(&slots);
    }

    /// Persist what the user has arranged: the angles and size, and the props
    /// as fractions of the tank.
    fn save_habitat_settings(&self) {
        persist::save_habitat_view(self.view.pitch, self.view.yaw, self.view.zoom);
        if let Some(h) = &self.habitat {
            persist::save_habitat_contents(h.kind.slug(), &h.snapshot());
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
        self.rebuild_habitat(false);
        self.rt.moved_display(self.region());
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
        // A koi in a terrarium, or a spider in a fish tank, would be the wrong
        // enclosure for the animal — so the tank is rebuilt from the new
        // creature's substrate, and the creature is placed inside it.
        self.rebuild_habitat(false);
        self.rt.moved_display(self.region());

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
        self.refresh_habitat_menu();
        self.last_mood.clear();
        println!(
            "now running the {} ({})",
            self.rt.creature().display_name().to_lowercase(),
            self.rt.brain_info()
        );
    }

    /// Move, turn or resize the enclosure while a chord is held.
    ///
    /// Deliberately not a click-and-drag — the overlay never takes a click
    /// (HABITAT_PLAN.md §2), and a held modifier gets the same "grab and move"
    /// feel without putting a hole in the desktop.
    ///
    /// Move works from the pointer's absolute position, because the tank should
    /// end up *under the pointer*. Turn and size work from its motion since the
    /// last frame, because there is no absolute position an angle could mean —
    /// and the delta is only taken when the same chord was held last frame, so
    /// reaching for the keys does not jerk the view by however far the mouse
    /// happened to be from where it was.
    fn handle_chord(&mut self) {
        let chord = if self.habitat.is_some() { self.chord } else { HabitatChord::None };
        let continuing = chord == self.prev_chord && chord.active();
        let delta = match (continuing, self.chord_from, self.last_screen_cursor) {
            (true, Some(a), Some(b)) => Vec2::new(b.x - a.x, b.y - a.y),
            _ => Vec2::ZERO,
        };
        let released = self.prev_chord.active() && !chord.active();
        self.prev_chord = chord;
        self.chord_from = self.last_screen_cursor;

        match chord {
            HabitatChord::Move => {
                if let (Some(at), Some(h)) = (self.last_env_cursor, self.habitat.as_mut()) {
                    let want = Region::new(at, h.region.size);
                    h.reshape(self.view.camera().clamp_on_screen(
                        want,
                        self.space.size(),
                        habitatmesh::FLOOR_Z,
                        habitatmesh::top_z(h.kind),
                        12.0,
                    ));
                }
            }
            // Across the width of a 1080p display is roughly the full range of
            // each control: fast enough to reach the ends without a second
            // grab, slow enough to stop where you meant to.
            HabitatChord::Orbit => {
                // Pointer up raises the camera, so the tank is seen from
                // further overhead — which is a *smaller* pitch.
                if self.view.adjust(-delta.y * 0.0035, delta.x * 0.0030, 0.0) {
                    self.rebuild_habitat(true);
                    self.rt.moved_display(self.region());
                }
            }
            HabitatChord::Zoom => {
                if self.view.adjust(0.0, 0.0, delta.y * 0.0030) {
                    self.rebuild_habitat(true);
                    self.rt.moved_display(self.region());
                }
            }
            HabitatChord::None => {}
        }

        // Writing the settings file on every frame of a drag would be a lot of
        // disk for a value that is only interesting once the user has settled
        // on it, so it goes down when they let go.
        if released {
            self.save_habitat_settings();
            self.refresh_habitat_menu();
            if self.args.habitat {
                println!("habitat view: {}", self.view.describe());
            }
        }
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
                    self.save_habitat_settings();
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
                tray::TrayCommand::ToggleGlass => {
                    self.args.glass = !self.args.glass;
                    persist::save_glass_choice(self.args.glass);
                    if let Some(t) = &self.tray {
                        t.set_glass(self.args.glass);
                    }
                    println!(
                        "{}",
                        if self.args.glass { "glass anatomy on" } else { "literal animal" }
                    );
                }
                tray::TrayCommand::ToggleHabitat => {
                    self.args.habitat = !self.args.habitat;
                    persist::save_habitat_choice(self.args.habitat);
                    self.rebuild_habitat(false);
                    // Whichever way it went, the creature has to end up inside
                    // the new world — otherwise switching the tank on around a
                    // fly at the far corner of the screen leaves it outside its
                    // own glass, walking home from off-stage.
                    self.rt.moved_display(self.region());
                    if let Some(t) = &self.tray {
                        t.set_habitat(self.args.habitat);
                    }
                    self.refresh_habitat_menu();
                    println!(
                        "{}",
                        match &self.habitat {
                            Some(h) => format!("habitat on ({})", h.kind.label()),
                            None => "habitat off - free roam".to_string(),
                        }
                    );
                }
                tray::TrayCommand::AdjustView(step) => {
                    // One coarse step each; the chords are the fine control.
                    let (dp, dy, dz) = match step {
                        tray::ViewStep::TiltUp => (-0.10, 0.0, 0.0),
                        tray::ViewStep::TiltDown => (0.10, 0.0, 0.0),
                        tray::ViewStep::TurnLeft => (0.0, -0.10, 0.0),
                        tray::ViewStep::TurnRight => (0.0, 0.10, 0.0),
                        tray::ViewStep::Bigger => (0.0, 0.0, 0.12),
                        tray::ViewStep::Smaller => (0.0, 0.0, -0.11),
                    };
                    if self.view.adjust(dp, dy, dz) {
                        self.rebuild_habitat(true);
                        self.rt.moved_display(self.region());
                        self.save_habitat_settings();
                    }
                    self.refresh_habitat_menu();
                }
                tray::TrayCommand::ResetView => {
                    self.view = camera::HabitatView::default();
                    self.rebuild_habitat(false);
                    self.rt.moved_display(self.region());
                    self.save_habitat_settings();
                    self.refresh_habitat_menu();
                    println!("habitat view reset: {}", self.view.describe());
                }
                tray::TrayCommand::AddProp(slot) | tray::TrayCommand::RemoveProp(slot) => {
                    let adding = matches!(cmd, tray::TrayCommand::AddProp(_));
                    if let Some(h) = self.habitat.as_mut() {
                        let kind = PropKind::catalogue(h.kind).get(slot).copied();
                        if let Some(k) = kind {
                            let changed = if adding { h.add(k) } else { h.remove(k) };
                            if changed {
                                println!(
                                    "{} {} - {} prop(s) in the {}",
                                    if adding { "added" } else { "removed" },
                                    k.label().to_lowercase(),
                                    h.props.len(),
                                    h.kind.label()
                                );
                                self.save_habitat_settings();
                            }
                        }
                    }
                    self.refresh_habitat_menu();
                }
                tray::TrayCommand::Restock => {
                    if let Some(h) = self.habitat.as_mut() {
                        h.restock();
                        println!("restocked the {}", h.kind.label());
                    }
                    self.save_habitat_settings();
                    self.refresh_habitat_menu();
                }
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
            if self.habitat.is_some() {
                // Two corrections inside a tank.
                //
                // Window edges are terrain for a creature loose on the desktop.
                // In an enclosure they are not: a fly that latched onto the top
                // of a browser window would be standing on something that is not
                // in its tank.
                //
                // And the cursor arrives in *screen* coordinates, which under
                // the tilted camera are no longer ground coordinates. Without
                // un-projecting it the creature flees a pointer that is nowhere
                // near where the user can see it — and wrongly by more the
                // further up the tank the pointer is. Everything else about the
                // senses still applies: someone leaning over the glass is
                // exactly what those channels are for.
                let cam = self.camera();
                let mut penned = env.clone();
                penned.ledges.clear();
                penned.cursor = penned.cursor.map(|c| cam.unproject_ground(c));
                for c in penned.clicks.iter_mut() {
                    *c = cam.unproject_ground(*c);
                }
                self.rt.sense(&penned, sense_dt);
                self.last_env_cursor = penned.cursor;
                self.last_screen_cursor = env.cursor;
                self.chord = env.chord;
                self.grabbing = env.chord.active();
            } else {
                self.rt.sense(&env, sense_dt);
                self.last_env_cursor = env.cursor;
                self.last_screen_cursor = env.cursor;
                self.chord = HabitatChord::None;
                self.grabbing = false;
            }
            if self.args.diag && self.frames < 3 { eprintln!("[diag] B: transduced"); }
        }

        // The brain at a true 1 kHz, decoupled from the frame rate, then the
        // body once — both inside the runtime, whichever creature it is.
        if self.args.diag && self.frames < 3 { eprintln!("[diag] C: stepping creature"); }
        let at_last_frame = self.rt.position();
        // Repositioning: while the chord is held the tank follows the pointer.
        // Deliberately not a click-and-drag — the overlay never takes a click
        // (HABITAT_PLAN.md §2), and a held modifier gets the same "grab and
        // move" feel without putting a hole in the desktop.
        self.handle_chord();

        let region = self.region();
        // The enclosure moves first, so the creature reacts to where the props
        // are now rather than to where they were a frame ago.
        let attractor = match &mut self.habitat {
            Some(h) => {
                h.step(dt, at_last_frame, self.last_env_cursor);
                h.attractor()
            }
            None => None,
        };
        self.rt.tick(dt, region, self.last_env_cursor, attractor);
        if self.args.diag && self.frames < 3 { eprintln!("[diag] E: body updated"); }

        let diag = self.args.diag && self.frames < 3;
        let (glass, shadows) = (self.args.glass, self.args.shadows);
        let hidden = self.hidden_for_fullscreen;
        let at = self.rt.position();
        // Read everything the composition needs off `self` before `build`
        // borrows the runtime for the rest of the frame.
        let cam = self.camera();
        let lift_hint = self.rt.vertical_hint();
        let grabbing = self.grabbing;
        let geometry = self.rt.build(glass);
        if diag { eprintln!("[diag] F: geometry built"); }

        // The enclosure and the creature share one pipeline and one vertex
        // format, so they go to the GPU as a single mesh rather than as a third
        // buffer and a second draw. The tank is written first and sits below the
        // creature in z, so depth ordering matches drawing order either way.
        // Compose the frame. Index order is draw order in a single draw call,
        // so the tank goes in behind the creature and the front glass after it,
        // and the pane blends over the animal exactly as glass should. The
        // creature's own index range is handed to the renderer so the shadow
        // pass covers the animal and not the furniture.
        let (body, creature_range, ground_z) = match &self.habitat {
            Some(h) => {
                let view = cam.view_dir();
                habitatmesh::build_back(&mut self.frame_mesh, h, view);
                habitatmesh::build_front(&mut self.front_mesh, h, view, grabbing);

                // The creature is lifted into the tank: standing on the
                // substrate, or — for a swimmer — floating at its own depth,
                // which the straight-down view could only ever fake with size.
                let lift = habitatmesh::creature_lift(h.kind, lift_hint);
                let start = self.frame_mesh.indices.len() as u32;
                let base = self.frame_mesh.verts.len() as u32;
                self.frame_mesh
                    .verts
                    .extend(geometry.body.verts.iter().map(|v| {
                        let mut v = *v;
                        v.pos[2] += lift;
                        v
                    }));
                self.frame_mesh
                    .indices
                    .extend(geometry.body.indices.iter().map(|i| i + base));
                let end = self.frame_mesh.indices.len() as u32;

                let fbase = self.frame_mesh.verts.len() as u32;
                self.frame_mesh.verts.extend_from_slice(&self.front_mesh.verts);
                self.frame_mesh
                    .indices
                    .extend(self.front_mesh.indices.iter().map(|i| i + fbase));
                (&self.frame_mesh, start..end, habitatmesh::shadow_z(h.kind))
            }
            None => {
                let n = geometry.body.indices.len() as u32;
                (geometry.body, 0..n, -0.5)
            }
        };

        if let (Some(r), Some(s)) = (self.renderer.as_mut(), self.surface.as_ref()) {
            if !hidden {
                let t0 = Instant::now();
                r.render(
                    s,
                    &render::Frame {
                        mesh: body,
                        neurons: geometry.neurons,
                        shadows,
                        creature: creature_range,
                        camera: cam,
                        ground_z,
                    },
                );
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
            // The tank goes down on the same checkpoint: a lily pad nosed into
            // a corner over an afternoon is exactly the kind of state a kill
            // would otherwise throw away.
            self.save_habitat_settings();
        }

        if self.last_report.elapsed() >= Duration::from_secs(10) {
            let fps = self.frames as f32 / self.last_report.elapsed().as_secs_f32();
            println!("fps {fps:.0}  {}", self.rt.status());
            self.frames = 0;
            self.last_report = Instant::now();
        }

        if self.args.seconds > 0 && self.start.elapsed() >= Duration::from_secs(self.args.seconds) {
            persist::save(self.rt.creature().id(), self.rt.habituation());
            self.save_habitat_settings();
            event_loop.exit();
        }
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    // The build hook (SPIDER_PLAN.md §5): `desktopfly notify pass|fail`
    // delivers one word to the running app and exits. This is the whole
    // integration; nothing is watched.
    if argv.get(1).map(|a| a == "notify").unwrap_or(false) {
        let word = argv.get(2).cloned().unwrap_or_default();
        if dfcore::env::BuildEvent::parse(&word).is_none() {
            eprintln!("usage: desktopfly notify pass|fail");
            std::process::exit(2);
        }
        match dfplatform::notify::send(&word) {
            Ok(()) => return,
            Err(e) => {
                eprintln!("no running DesktopFly to notify ({e})");
                std::process::exit(1);
            }
        }
    }
    if let Some(i) = argv.iter().position(|a| a == "--snapshot") {
        let path = argv.get(i + 1).cloned().unwrap_or_else(|| "fly.png".into());
        let alt = argv
            .iter()
            .position(|a| a == "--alt")
            .and_then(|j| argv.get(j + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        let glass = !argv.iter().any(|a| a == "--literal");
        let zoom = argv
            .iter()
            .position(|a| a == "--zoom")
            .and_then(|j| argv.get(j + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.0);
        let mut rt = runtime::make(&creature_arg(&argv), dfcore::DEFAULT_SEED);
        // The habitat view is spelled out on the command line so that an
        // angle or a size can be judged from a rendered file rather than from
        // a screenshot of the live overlay — which is how the enclosure itself
        // was checked (HABITAT_PLAN.md §4).
        let num = |flag: &str, dflt: f32| {
            argv.iter()
                .position(|a| a == flag)
                .and_then(|i| argv.get(i + 1))
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(dflt)
        };
        let habitat = argv.iter().any(|a| a == "--habitat").then(|| camera::HabitatView {
            pitch: num("--pitch", camera::DEFAULT_PITCH),
            yaw: num("--yaw", camera::DEFAULT_YAW),
            zoom: num("--tank", 1.0),
        });
        // Bigger frame with a tank in it: 320 px is barely wider than the fly.
        let side = if habitat.is_some() { 560 } else { 320 };
        snapshot::render_to_png(
            rt.as_mut(),
            &path,
            side,
            side,
            alt,
            20,
            glass,
            zoom,
            habitat,
        );
        return;
    }

    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(parse_args());
    event_loop.run_app(&mut app).expect("run");
}
