//! winprobe — Windows window-terrain sensing, and Spike 0's OS-level verifier.
//!
//! Two jobs, both needed:
//!
//! 1. **Verify Spike 0** — read the real `GWL_EXSTYLE` bits off the overlay
//!    window instead of trusting winit's return value, and confirm it is
//!    topmost / click-through / not in the taskbar.
//!
//! 2. **Start Phase 2's terrain sensing** — this is the Windows equivalent of
//!    `WindowSense.poll` (Environment.swift:26), which on macOS uses
//!    `CGWindowListCopyWindowInfo`. The equivalences (PORT_PLAN.md §4):
//!      - `CGWindowListCopyWindowInfo`  -> `EnumWindows`
//!      - `kCGWindowLayer == 0`         -> visible, has a title, not a tool window
//!      - `kCGWindowAlpha > 0.05`       -> not cloaked (`DWMWA_CLOAKED`)
//!      - `kCGWindowBounds`             -> `DWMWA_EXTENDED_FRAME_BOUNDS`, *not*
//!                                         `GetWindowRect` (which includes the
//!                                         invisible resize border, so the fly
//!                                         would walk several px above the
//!                                         title bar)
//!      - `kCGWindowOwnerPID != myPID`  -> `GetWindowThreadProcessId`
//!      - `kCGWindowNumber`             -> the `HWND` as the ledge id
//!
//! Usage: winprobe [--title SUBSTR] [--terrain]

use std::ffi::c_void;

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
};

/// Not exposed by the `windows` crate's WindowsAndMessaging constants.
const WS_EX_NOREDIRECTIONBITMAP: isize = 0x0020_0000;

#[derive(Debug, Clone)]
struct WinInfo {
    hwnd: isize,
    pid: u32,
    title: String,
    ex_style: isize,
    visible: bool,
    cloaked: bool,
    /// GetWindowRect — includes the invisible resize border.
    outer: RECT,
    /// DWMWA_EXTENDED_FRAME_BOUNDS — what the user actually sees.
    frame: RECT,
}

impl WinInfo {
    /// The macOS build treats a window's top edge as a walkable ledge
    /// (Environment.swift:52). Same idea, using the visually-correct bounds.
    fn is_terrain_candidate(&self, my_pid: u32) -> bool {
        self.visible
            && !self.cloaked
            && self.pid != my_pid
            && !self.title.is_empty()
            && (self.ex_style & WS_EX_TOOLWINDOW.0 as isize) == 0
            && (self.frame.right - self.frame.left) >= 160
            && (self.frame.bottom - self.frame.top) >= 60
    }
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let out = &mut *(lparam.0 as *mut Vec<WinInfo>);
    let len = GetWindowTextLengthW(hwnd);
    let title = if len > 0 {
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..n as usize])
    } else {
        String::new()
    };

    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));

    let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);

    let mut outer = RECT::default();
    let _ = GetWindowRect(hwnd, &mut outer);

    // The correct visual bounds. Falls back to GetWindowRect if DWM says no.
    let mut frame = RECT::default();
    let ok = DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS,
        &mut frame as *mut RECT as *mut c_void,
        std::mem::size_of::<RECT>() as u32,
    );
    if ok.is_err() {
        frame = outer;
    }

    // Cloaked windows are "visible" to IsWindowVisible but not shown: UWP
    // suspended apps, windows on other virtual desktops. macOS filtered these
    // with kCGWindowAlpha; on Windows this is the equivalent.
    let mut cloaked = 0u32;
    let _ = DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED,
        &mut cloaked as *mut u32 as *mut c_void,
        std::mem::size_of::<u32>() as u32,
    );

    out.push(WinInfo {
        hwnd: hwnd.0 as isize,
        pid,
        title,
        ex_style,
        visible: IsWindowVisible(hwnd).as_bool(),
        cloaked: cloaked != 0,
        outer,
        frame,
    });
    BOOL(1) // keep enumerating
}

fn enumerate() -> Vec<WinInfo> {
    let mut out: Vec<WinInfo> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut out as *mut Vec<WinInfo> as isize));
    }
    out
}

fn report_styles(w: &WinInfo) {
    println!("\nFOUND  \"{}\"  hwnd=0x{:X}  pid={}", w.title, w.hwnd, w.pid);
    println!("  visible: {}   cloaked: {}", w.visible, w.cloaked);
    println!(
        "  GetWindowRect:              ({}, {}) {}x{}",
        w.outer.left,
        w.outer.top,
        w.outer.right - w.outer.left,
        w.outer.bottom - w.outer.top
    );
    println!(
        "  DWMWA_EXTENDED_FRAME_BOUNDS ({}, {}) {}x{}   <- the bounds to walk on",
        w.frame.left,
        w.frame.top,
        w.frame.right - w.frame.left,
        w.frame.bottom - w.frame.top
    );
    println!("  exstyle: 0x{:08X}", w.ex_style);

    let checks: [(&str, isize, bool); 6] = [
        ("WS_EX_TOPMOST", WS_EX_TOPMOST.0 as isize, true),
        ("WS_EX_TRANSPARENT", WS_EX_TRANSPARENT.0 as isize, true),
        ("WS_EX_TOOLWINDOW", WS_EX_TOOLWINDOW.0 as isize, true),
        ("WS_EX_NOREDIRECTIONBITMAP", WS_EX_NOREDIRECTIONBITMAP, true),
        ("WS_EX_LAYERED", WS_EX_LAYERED.0 as isize, false),
        ("WS_EX_NOACTIVATE", WS_EX_NOACTIVATE.0 as isize, false),
    ];
    for (name, bit, required) in checks {
        let set = (w.ex_style & bit) != 0;
        let verdict = match (required, set) {
            (true, true) => "SET   (required) OK",
            (true, false) => "unset (required) *** MISSING ***",
            (_, true) => "SET   (optional)",
            (_, false) => "unset (optional)",
        };
        println!("    {name:<27} {verdict}");
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let want_title = argv
        .iter()
        .position(|a| a == "--title")
        .and_then(|i| argv.get(i + 1).cloned())
        .unwrap_or_else(|| "DesktopFly spike0".to_string());
    let terrain = argv.iter().any(|a| a == "--terrain");

    let my_pid = unsafe { GetCurrentProcessId() };
    let all = enumerate();
    println!("EnumWindows returned {} top-level windows", all.len());

    if terrain {
        let mut ledges: Vec<&WinInfo> = all
            .iter()
            .filter(|w| w.is_terrain_candidate(my_pid))
            .collect();
        ledges.sort_by_key(|w| w.frame.top);
        println!("\n{} terrain candidates (window top edge = walkable ledge):", ledges.len());
        for w in ledges.iter().take(20) {
            let t: String = w.title.chars().take(48).collect();
            println!(
                "  y={:<6} x={:>6}..{:<6}  {}",
                w.frame.top, w.frame.left, w.frame.right, t
            );
        }
    }

    let matches: Vec<&WinInfo> = all.iter().filter(|w| w.title.contains(&want_title)).collect();
    if matches.is_empty() {
        println!("\nno window titled containing {want_title:?} — is the spike running?");
        std::process::exit(1);
    }
    for w in matches {
        report_styles(w);
    }
}
