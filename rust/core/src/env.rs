//! The platform boundary.
//!
//! The single most important structural fact about DesktopFly is that the
//! simulation never touches the OS: it consumes a handful of scalars and emits
//! population rates. So the entire platform layer's job is to fill in one plain
//! struct, [`EnvSnapshot`], 30 times a second. Everything below this line is
//! portable (PORT_PLAN.md §5).
//!
//! This is also the seam a second creature reads through: a worm and a fly
//! sense the same desktop, they just transduce it differently.

use crate::util::{Ledge, Vec2};

/// A rectangle in physical screen pixels: Win32 convention, top-left origin,
/// y increasing downward. On a multi-monitor desktop `left`/`top` may be
/// negative for monitors above or to the left of the primary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Rect {
            left,
            top,
            right,
            bottom,
        }
    }
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn center_x(&self) -> f32 {
        (self.left + self.right) as f32 / 2.0
    }
    pub fn center_y(&self) -> f32 {
        (self.top + self.bottom) as f32 / 2.0
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        self.left < o.right && o.left < self.right && self.top < o.bottom && o.top < self.bottom
    }
}

/// Converts between physical screen pixels and the creature's scene space.
///
/// **This type exists because the alternative is a bug farm.** macOS gives
/// Cocoa coordinates (origin bottom-left of the primary display, y up); Win32
/// gives top-left origin with y down and a virtual desktop that can extend into
/// negative coordinates. The Swift build open-codes `primaryH - rect.maxY`
/// flips at four different sites (Environment.swift:30-56). Doing that in Rust
/// would reproduce the bugs, so every conversion goes through here and is
/// tested.
///
/// Scene space matches the Swift build's convention: origin at the *centre* of
/// the display the creature is on, x right, **y up**.
///
/// Scene units are **logical**, not physical pixels. macOS works in points, so
/// the Swift build gets this free; on Windows every coordinate that arrives
/// from Win32 (`GetCursorPos`, `DwmGetWindowAttribute`, monitor bounds) is in
/// physical pixels, and using those directly makes the creature shrink by the
/// display's scale factor — 33% smaller on a 150% display, half size on a 200%
/// one. Dividing through here means everything downstream (body size, walking
/// speed, ledge widths, escape distances) keeps a constant *apparent* size,
/// which is PORT_PLAN.md §8 decision 4.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenSpace {
    /// The display's bounds in physical pixels.
    pub display: Rect,
    /// Physical pixels per logical unit. 1.0 at 100%, 1.5 at 150%.
    pub scale: f32,
}

impl ScreenSpace {
    /// Unscaled — for tests and for platforms that report logical bounds.
    pub fn new(display: Rect) -> Self {
        ScreenSpace {
            display,
            scale: 1.0,
        }
    }

    pub fn with_scale(display: Rect, scale: f32) -> Self {
        ScreenSpace {
            display,
            scale: if scale > 0.05 { scale } else { 1.0 },
        }
    }

    /// Physical pixel -> scene. Note the y flip and the DPI divide.
    #[inline]
    pub fn to_scene(&self, x: f32, y: f32) -> Vec2 {
        Vec2::new(
            (x - self.display.center_x()) / self.scale,
            (self.display.center_y() - y) / self.scale,
        )
    }

    /// Scene -> physical pixel.
    #[inline]
    pub fn to_physical(&self, p: Vec2) -> (f32, f32) {
        (
            p.x * self.scale + self.display.center_x(),
            self.display.center_y() - p.y * self.scale,
        )
    }

    /// Logical size of the display — the bounds the creature lives inside.
    pub fn size(&self) -> (f32, f32) {
        (
            self.display.width() as f32 / self.scale,
            self.display.height() as f32 / self.scale,
        )
    }

    /// A window's top edge becomes a walkable ledge, clipped to this display.
    /// Port of the ledge construction at Environment.swift:47-56, including the
    /// 15 px inset and the "must be on screen" and "wide enough" guards.
    pub fn ledge_from_window(&self, w: &Rect, id: i64) -> Option<Ledge> {
        if !w.intersects(&self.display) {
            return None;
        }
        let (width, height) = self.size();
        let top_y = (self.display.center_y() - w.top as f32) / self.scale;
        let x0 = ((w.left as f32 - self.display.center_x()) / self.scale).max(-width / 2.0 + 15.0);
        let x1 = ((w.right as f32 - self.display.center_x()) / self.scale).min(width / 2.0 - 15.0);
        if top_y < height / 2.0 - 8.0 && top_y > -height / 2.0 + 8.0 && x1 - x0 > 100.0 {
            Some(Ledge {
                y: top_y,
                x0,
                x1,
                id,
            })
        } else {
            None
        }
    }
}

/// A newly-appeared window, as a looming stimulus.
#[derive(Debug, Clone, Copy)]
pub struct WindowLoom {
    /// Window centre in scene coordinates.
    pub center: Vec2,
    /// Largest dimension in pixels — how big the thing that appeared is.
    pub size: f32,
}

/// Everything the OS layer produces. Nothing below this knows what a window is.
#[derive(Debug, Clone, Default)]
pub struct EnvSnapshot {
    pub cursor: Option<Vec2>,
    pub cursor_vel: Vec2,
    /// Click positions since the last poll — substrate taps.
    pub clicks: Vec<Vec2>,
    pub ledges: Vec<Ledge>,
    pub new_windows: Vec<WindowLoom>,
    /// Ledge ids that vanished since the last poll.
    pub closed_windows: Vec<i64>,
    /// Seconds since the user last touched the machine.
    pub idle_secs: f32,
    /// 0..1 — is the user typing right now? See `Senses` docs for the caveat.
    pub typing_level: f32,
    /// 0..1 — thermal pressure on macOS; a clock-throttle proxy on Windows.
    pub machine_heat: f32,
    /// Local time of day, 0..24, for the circadian curve.
    pub local_hour: f32,
    /// Whether a fullscreen app or presentation is running; the overlay should
    /// hide rather than fight it. No macOS equivalent is needed.
    pub fullscreen_app_active: bool,
    /// The "grab" chord (Ctrl+Shift) is held: the user is repositioning the
    /// enclosure. Modifier keys only — they carry no typed content, which is
    /// what keeps this on the right side of the content-blind rule the other
    /// senses follow. Nothing reads it outside habitat mode.
    pub grab_held: bool,
    /// What kind of application is in front — an enum, never a name or a
    /// title, so nothing about *what* the user is doing can leak downstream
    /// (SPIDER_PLAN.md §5).
    pub foreground: Foreground,
    /// Build or test outcomes the user chose to report since the last poll,
    /// via `desktopfly notify pass|fail`. Opt-in only: nothing is watched.
    pub build_events: Vec<BuildEvent>,
}

/// The class of the foreground application. Derived from the process name
/// alone; the platform layer exposes no string, so there is nothing to leak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Foreground {
    #[default]
    Unknown,
    Editor,
    Terminal,
    Browser,
    Other,
}

impl Foreground {
    /// Classify an executable's base name (lower-cased, no extension). The
    /// list is deliberately short and boring; "Other" is the safe answer.
    pub fn classify(exe_stem: &str) -> Foreground {
        const EDITORS: &[&str] = &[
            "code", "code - insiders", "cursor", "windsurf", "devenv", "rider64", "idea64",
            "pycharm64", "clion64", "goland64", "webstorm64", "rustrover64", "sublime_text",
            "notepad++", "nvim", "neovide", "vim", "gvim", "emacs", "zed", "atom", "claude",
            "kate", "notepad",
        ];
        const TERMINALS: &[&str] = &[
            "windowsterminal", "wt", "cmd", "powershell", "pwsh", "conhost", "alacritty",
            "wezterm-gui", "wezterm", "hyper", "mintty", "kitty", "ghostty", "tabby",
        ];
        const BROWSERS: &[&str] = &[
            "chrome", "msedge", "firefox", "brave", "opera", "vivaldi", "arc", "chromium",
            "safari", "zen",
        ];
        let s = exe_stem.to_ascii_lowercase();
        if EDITORS.contains(&s.as_str()) {
            Foreground::Editor
        } else if TERMINALS.contains(&s.as_str()) {
            Foreground::Terminal
        } else if BROWSERS.contains(&s.as_str()) {
            Foreground::Browser
        } else {
            Foreground::Other
        }
    }

    /// Is the user coding, as far as the spider can tell?
    pub fn is_work(self) -> bool {
        matches!(self, Foreground::Editor | Foreground::Terminal)
    }
}

/// A reported build or test outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildEvent {
    Pass,
    Fail,
}

impl BuildEvent {
    pub fn parse(word: &str) -> Option<BuildEvent> {
        match word.trim().to_ascii_lowercase().as_str() {
            "pass" | "ok" | "green" | "success" | "0" => Some(BuildEvent::Pass),
            "fail" | "failed" | "error" | "red" | "1" => Some(BuildEvent::Fail),
            _ => None,
        }
    }
}

/// Implemented once per OS. The only trait in the project that is allowed to
/// know about the operating system.
pub trait Senses {
    /// Called at ~30 Hz. Must not block.
    fn poll(&mut self, space: &ScreenSpace) -> EnvSnapshot;

    /// Human-readable note about what this platform can and cannot measure, so
    /// the app can be honest in its UI. On Windows, typing detection is an
    /// inference and "temperature" is really machine load.
    fn fidelity_notes(&self) -> &'static [&'static str] {
        &[]
    }
}

/// Maps thermal state to a locomotion tempo multiplier. Flies are ectotherms:
/// a hot machine is a fast fly. Port of `thermalTempo` (Environment.swift:91),
/// generalised from macOS's four discrete states to a 0..1 scalar so Windows
/// can supply a continuous proxy.
pub fn thermal_tempo(machine_heat: f32) -> f32 {
    1.0 + 0.5 * crate::util::clamp(machine_heat, 0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn primary() -> ScreenSpace {
        ScreenSpace::new(Rect::new(0, 0, 2560, 1440))
    }

    #[test]
    fn scene_origin_is_display_centre() {
        let s = primary();
        let p = s.to_scene(1280.0, 720.0);
        assert_eq!(p, Vec2::new(0.0, 0.0));
    }

    #[test]
    fn y_is_flipped_relative_to_win32() {
        let s = primary();
        // Top of the screen in Win32 (y=0) must be *positive* y in scene space.
        assert!(s.to_scene(1280.0, 0.0).y > 0.0);
        assert!(s.to_scene(1280.0, 1439.0).y < 0.0);
    }

    #[test]
    fn round_trips() {
        let s = primary();
        for (x, y) in [(0.0, 0.0), (2560.0, 1440.0), (137.0, 991.0)] {
            let (bx, by) = s.to_physical(s.to_scene(x, y));
            assert!((bx - x).abs() < 1e-3 && (by - y).abs() < 1e-3, "{x},{y}");
        }
    }

    /// The bug this type exists to prevent: a second monitor placed to the left
    /// of the primary has negative physical coordinates, and naive maths puts
    /// the creature off-screen.
    #[test]
    fn handles_monitors_at_negative_virtual_coordinates() {
        let s = ScreenSpace::new(Rect::new(-1920, -200, 0, 880));
        assert_eq!(s.to_scene(-960.0, 340.0), Vec2::new(0.0, 0.0));
        let (x, y) = s.to_physical(Vec2::new(0.0, 0.0));
        assert!((x + 960.0).abs() < 1e-3 && (y - 340.0).abs() < 1e-3);
        // A point at the monitor's top-left maps to the top-left of scene space.
        let tl = s.to_scene(-1920.0, -200.0);
        assert!(tl.x < 0.0 && tl.y > 0.0);
    }

    #[test]
    fn ledge_sits_at_the_window_top_edge_in_scene_space() {
        let s = primary();
        // A window whose top edge is 240px down from the top of the screen.
        let w = Rect::new(400, 240, 1600, 900);
        let l = s.ledge_from_window(&w, 7).expect("should be a ledge");
        assert_eq!(l.id, 7);
        assert!((l.y - (720.0 - 240.0)).abs() < 1e-3, "y was {}", l.y);
        assert!(l.x0 < l.x1);
        // Walking along it must stay inside the display.
        assert!(l.x0 >= -1280.0 && l.x1 <= 1280.0);
    }

    #[test]
    fn narrow_or_offscreen_windows_are_not_ledges() {
        let s = primary();
        // Too narrow to walk on.
        assert!(s.ledge_from_window(&Rect::new(0, 300, 60, 400), 1).is_none());
        // Entirely on another monitor.
        assert!(s
            .ledge_from_window(&Rect::new(3000, 300, 4000, 900), 2)
            .is_none());
        // Top edge above the screen (a maximized window) — nothing to stand on.
        assert!(s
            .ledge_from_window(&Rect::new(0, -800, 2000, 900), 3)
            .is_none());
    }

    /// The DPI bug this exists to prevent: on a scaled display, physical
    /// coordinates make the creature and its world shrink.
    #[test]
    fn scene_units_are_logical_not_physical() {
        let unscaled = ScreenSpace::new(Rect::new(0, 0, 2560, 1440));
        let scaled = ScreenSpace::with_scale(Rect::new(0, 0, 2560, 1440), 2.0);

        // The same physical monitor is half as many logical units across.
        assert_eq!(unscaled.size(), (2560.0, 1440.0));
        assert_eq!(scaled.size(), (1280.0, 720.0));

        // A cursor at the same physical pixel is half as far from centre.
        let a = unscaled.to_scene(2560.0, 720.0);
        let b = scaled.to_scene(2560.0, 720.0);
        assert!((a.x - 1280.0).abs() < 1e-3);
        assert!((b.x - 640.0).abs() < 1e-3);
    }

    #[test]
    fn scaled_round_trips() {
        let s = ScreenSpace::with_scale(Rect::new(-1920, -200, 0, 880), 1.5);
        for (x, y) in [(-1920.0, -200.0), (0.0, 880.0), (-433.0, 91.0)] {
            let (bx, by) = s.to_physical(s.to_scene(x, y));
            assert!((bx - x).abs() < 1e-2 && (by - y).abs() < 1e-2, "{x},{y}");
        }
    }

    #[test]
    fn ledges_are_reported_in_logical_units_too() {
        let s = ScreenSpace::with_scale(Rect::new(0, 0, 2560, 1440), 2.0);
        // A window 480 physical px down from the top of the screen.
        let l = s
            .ledge_from_window(&Rect::new(200, 480, 1600, 1200), 1)
            .expect("should be a ledge");
        // The screen centre is 720 physical px down; the window top is 480, so
        // the edge is 240 physical px above centre = 120 logical px at 2x.
        assert!((l.y - 120.0).abs() < 1e-3, "ledge y {}", l.y);
        assert!(l.x1 <= 640.0, "clipped to the logical display: {}", l.x1);
    }

    /// A zero or nonsense scale factor must not produce a divide-by-zero world.
    #[test]
    fn a_broken_scale_factor_falls_back_to_unscaled() {
        let s = ScreenSpace::with_scale(Rect::new(0, 0, 800, 600), 0.0);
        assert_eq!(s.scale, 1.0);
        assert_eq!(s.size(), (800.0, 600.0));
    }

    #[test]
    fn thermal_tempo_spans_the_documented_range() {
        // macOS mapped nominal->1.0 and critical->1.5 (Environment.swift:91).
        assert!((thermal_tempo(0.0) - 1.0).abs() < 1e-6);
        assert!((thermal_tempo(1.0) - 1.5).abs() < 1e-6);
        assert!((thermal_tempo(2.0) - 1.5).abs() < 1e-6, "must clamp");
    }

    #[test]
    fn foreground_classifies_by_process_stem_only() {
        assert_eq!(Foreground::classify("Code"), Foreground::Editor);
        assert_eq!(Foreground::classify("WindowsTerminal"), Foreground::Terminal);
        assert_eq!(Foreground::classify("msedge"), Foreground::Browser);
        assert_eq!(Foreground::classify("explorer"), Foreground::Other);
        assert!(Foreground::Editor.is_work() && Foreground::Terminal.is_work());
        assert!(!Foreground::Browser.is_work());
    }

    #[test]
    fn build_events_parse_leniently_and_reject_junk() {
        assert_eq!(BuildEvent::parse(" PASS
"), Some(BuildEvent::Pass));
        assert_eq!(BuildEvent::parse("fail"), Some(BuildEvent::Fail));
        assert_eq!(BuildEvent::parse("1"), Some(BuildEvent::Fail));
        assert_eq!(BuildEvent::parse("rm -rf /"), None);
    }
}
