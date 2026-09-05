//! Windows implementation of [`dfcore::Senses`].
//!
//! Every macOS sense in `Environment.swift` mapped to its Windows equivalent
//! (PORT_PLAN.md §4). Like the macOS build, nothing here needs a permission
//! prompt or an entitlement — but two senses are genuinely weaker on Windows
//! and [`WindowsSenses::fidelity_notes`] says so out loud, because the whole
//! project rests on being precise about what is measured.

use std::collections::HashSet;
use std::ffi::c_void;
use std::time::Instant;

use dfcore::env::{EnvSnapshot, Foreground, Rect, ScreenSpace, Senses, WindowLoom};
use dfcore::util::clamp;

use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::System::Power::{CallNtPowerInformation, ProcessorInformation};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, GetSystemTimes, OpenProcess, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetLastInputInfo, LASTINPUTINFO, VK_LBUTTON, VK_RBUTTON,
};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW,
    GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
};

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ProcessorPowerInformation {
    number: u32,
    max_mhz: u32,
    current_mhz: u32,
    mhz_limit: u32,
    max_idle_state: u32,
    current_idle_state: u32,
}

#[derive(Debug, Clone, Copy)]
struct RawWindow {
    hwnd: isize,
    frame: RECT,
}

struct EnumState {
    my_pid: u32,
    out: Vec<RawWindow>,
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let st = &mut *(lparam.0 as *mut EnumState);

    // macOS filtered on kCGWindowLayer==0, alpha>0.05, owner PID and a minimum
    // size (Environment.swift:38-45). These are the Windows equivalents.
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    if GetWindowTextLengthW(hwnd) == 0 {
        return BOOL(1);
    }
    if (GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW.0 as isize) != 0 {
        return BOOL(1);
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == st.my_pid {
        return BOOL(1); // never treat our own overlay as terrain
    }

    // Cloaked: visible to IsWindowVisible but not actually shown (suspended UWP
    // apps, windows on another virtual desktop).
    let mut cloaked = 0u32;
    let _ = DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED,
        &mut cloaked as *mut u32 as *mut c_void,
        std::mem::size_of::<u32>() as u32,
    );
    if cloaked != 0 {
        return BOOL(1);
    }

    // GetWindowRect includes the invisible resize border — using it would put
    // the creature several pixels above the visible title bar. Spike 0 measured
    // an 8 px discrepancy on maximized windows.
    let mut frame = RECT::default();
    if DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS,
        &mut frame as *mut RECT as *mut c_void,
        std::mem::size_of::<RECT>() as u32,
    )
    .is_err()
        && GetWindowRect(hwnd, &mut frame).is_err() {
            return BOOL(1);
        }
    if (frame.right - frame.left) < 160 || (frame.bottom - frame.top) < 60 {
        return BOOL(1);
    }

    st.out.push(RawWindow {
        hwnd: hwnd.0 as isize,
        frame,
    });
    BOOL(1)
}

pub struct WindowsSenses {
    my_pid: u32,
    known_windows: HashSet<i64>,
    first_poll: bool,
    last_cursor: Option<(f32, f32)>,
    last_poll: Instant,
    /// EMA, matching the macOS `typingLevel` smoothing (main.swift:772).
    typing_level: f32,
    /// The build hook's pipe server (SPIDER_PLAN.md §5).
    notify: crate::notify::NotifyServer,
    prev_click_down: (bool, bool),
    prev_cpu: Option<(u64, u64)>,
    cpu_load: f32,
    /// When the CPU counters were last sampled. Sampling at the 30 Hz poll rate
    /// gives a 33 ms window, which is barely two ticks of the system timer's
    /// ~15.6 ms granularity — the resulting "load" is mostly quantisation
    /// noise, and it drives the creature's tempo.
    last_cpu_sample: Instant,
}

impl Default for WindowsSenses {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsSenses {
    pub fn new() -> Self {
        WindowsSenses {
            my_pid: unsafe { GetCurrentProcessId() },
            known_windows: HashSet::new(),
            first_poll: true,
            last_cursor: None,
            last_poll: Instant::now(),
            typing_level: 0.0,
            notify: crate::notify::NotifyServer::start(),
            prev_click_down: (false, false),
            prev_cpu: None,
            cpu_load: 0.0,
            last_cpu_sample: Instant::now(),
        }
    }

    fn enumerate(&self) -> Vec<RawWindow> {
        let mut st = EnumState {
            my_pid: self.my_pid,
            out: Vec::new(),
        };
        unsafe {
            let _ = EnumWindows(
                Some(enum_proc),
                LPARAM(&mut st as *mut EnumState as isize),
            );
        }
        st.out
    }

    /// Seconds since the user last touched mouse or keyboard.
    /// `GetLastInputInfo` is the direct equivalent of the macOS
    /// `CGEventSource` idle query — permission-free, reveals *when* and never
    /// *what*.
    fn idle_seconds() -> f32 {
        unsafe {
            let mut li = LASTINPUTINFO {
                cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };
            if GetLastInputInfo(&mut li).as_bool() {
                let now = GetTickCount();
                // Tick counts wrap every ~49 days; wrapping_sub handles it.
                now.wrapping_sub(li.dwTime) as f32 / 1000.0
            } else {
                0.0
            }
        }
    }

    /// Windows has no `ProcessInfo.thermalState`. WMI thermal zones need admin
    /// and are frequently unimplemented, so this uses processor clock
    /// throttling (`CurrentMhz / MaxMhz`) blended with overall CPU load —
    /// permission-free, and a decent proxy for "this machine is working hard".
    /// It is honestly *machine load*, not temperature; see `fidelity_notes`.
    fn machine_heat(&mut self) -> f32 {
        let throttle = unsafe {
            let cpus = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1);
            let mut buf = vec![ProcessorPowerInformation::default(); cpus];
            let bytes = (cpus * std::mem::size_of::<ProcessorPowerInformation>()) as u32;
            let status = CallNtPowerInformation(
                ProcessorInformation,
                None,
                0,
                Some(buf.as_mut_ptr() as *mut c_void),
                bytes,
            );
            if status.is_ok() {
                let (mut cur, mut max) = (0f64, 0f64);
                for p in &buf {
                    cur += p.current_mhz as f64;
                    max += p.max_mhz.max(1) as f64;
                }
                // Ratio > 1 happens under boost; clamp so boost reads as "hot".
                clamp((cur / max.max(1.0)) as f32, 0.0, 1.0)
            } else {
                0.5
            }
        };

        // CPU load across the whole system. Sampled over at least half a
        // second so the window spans enough timer ticks to mean something;
        // between samples the previous value is held.
        let sample_due = self.last_cpu_sample.elapsed() >= std::time::Duration::from_millis(500);
        if sample_due {
            self.last_cpu_sample = Instant::now();
        }
        if sample_due {
        unsafe {
            let (mut idle, mut kernel, mut user) = (
                Default::default(),
                Default::default(),
                Default::default(),
            );
            if GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).is_ok() {
                let to_u64 = |ft: windows::Win32::Foundation::FILETIME| {
                    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
                };
                let idle_t = to_u64(idle);
                let total_t = to_u64(kernel) + to_u64(user);
                if let Some((prev_idle, prev_total)) = self.prev_cpu {
                    // On Windows the kernel time already includes idle, so
                    // total = kernel + user and idle is a subset of it.
                    let d_total = total_t.saturating_sub(prev_total);
                    let d_idle = idle_t.saturating_sub(prev_idle);
                    if d_total > 0 {
                        let load = 1.0 - (d_idle as f64 / d_total as f64);
                        self.cpu_load = clamp(load as f32, 0.0, 1.0);
                    }
                }
                self.prev_cpu = Some((idle_t, total_t));
            }
        }
        }

        // Mostly load, with sustained clock-throttling as a multiplier: a busy
        // machine that is *also* clocking down is the hottest signal available.
        let throttle_penalty = clamp((1.0 - throttle) * 2.0, 0.0, 1.0);
        clamp(0.65 * self.cpu_load + 0.35 * throttle_penalty, 0.0, 1.0)
    }

    /// Windows cannot separate keyboard from mouse idle: `GetLastInputInfo`
    /// returns one combined timestamp, and the APIs that *can* separate them
    /// (`WH_KEYBOARD_LL`, raw input) also reveal *which key* — which would
    /// break the project's "knows when, never what" privacy claim.
    ///
    /// So typing is inferred: recent input **and** a stationary cursor.
    fn typing_now(idle_secs: f32, cursor_moved: bool) -> bool {
        idle_secs < 0.6 && !cursor_moved
    }

    /// Should the overlay get out of the way?
    ///
    /// Only for genuine exclusive-fullscreen D3D and presentation mode.
    /// **Not** `QUNS_BUSY`: that also fires for an ordinary maximized window,
    /// which would hide the fly during normal use — it did exactly that, and
    /// looked like a renderer hang rather than a policy bug.
    /// The class of the foreground app, from its process image name only.
    /// The name never leaves this function; the title is never read.
    fn foreground_class() -> Foreground {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return Foreground::Unknown;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return Foreground::Unknown;
            }
            let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                return Foreground::Unknown;
            };
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
            let _ = CloseHandle(h);
            if !ok {
                return Foreground::Unknown;
            }
            let path = String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]);
            let stem = std::path::Path::new(&path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            Foreground::classify(stem)
        }
    }

    fn fullscreen_app_active() -> bool {
        unsafe {
            match SHQueryUserNotificationState() {
                Ok(s) => s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE,
                Err(_) => false,
            }
        }
    }

    fn local_hour() -> f32 {
        let t = unsafe { GetLocalTime() };
        t.wHour as f32 + t.wMinute as f32 / 60.0
    }
}

impl Senses for WindowsSenses {
    fn poll(&mut self, space: &ScreenSpace) -> EnvSnapshot {
        let now = Instant::now();
        let dt = (now - self.last_poll).as_secs_f32().max(1e-4);
        self.last_poll = now;

        let mut snap = EnvSnapshot::default();

        // --- cursor (macOS: NSEvent.mouseLocation) ---
        let mut pt = POINT::default();
        let mut cursor_moved = false;
        if unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt) }.is_ok() {
            let (px, py) = (pt.x as f32, pt.y as f32);
            if let Some((lx, ly)) = self.last_cursor {
                let (vx, vy) = ((px - lx) / dt, (py - ly) / dt);
                // Velocity in *scene* space, so y points the same way the
                // creature thinks it does.
                snap.cursor_vel = dfcore::Vec2::new(vx, -vy);
                cursor_moved = (px - lx).abs() > 0.5 || (py - ly).abs() > 0.5;
            }
            self.last_cursor = Some((px, py));
            snap.cursor = Some(space.to_scene(px, py));
        }

        // --- clicks as substrate taps ---
        // Polling GetAsyncKeyState rather than installing WH_MOUSE_LL: a global
        // mouse hook is an EDR/antivirus heuristic, and this app should not look
        // like a keylogger. Cost: a very fast click between polls can be missed.
        let down = unsafe {
            (
                (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0,
                (GetAsyncKeyState(VK_RBUTTON.0 as i32) as u16 & 0x8000) != 0,
            )
        };
        let pressed = (down.0 && !self.prev_click_down.0) || (down.1 && !self.prev_click_down.1);
        self.prev_click_down = down;
        if pressed {
            if let Some(c) = snap.cursor {
                snap.clicks.push(c);
            }
        }

        // --- idle, typing ---
        snap.idle_secs = Self::idle_seconds();
        let typing = Self::typing_now(snap.idle_secs, cursor_moved);
        self.typing_level += ((if typing { 1.0 } else { 0.0 }) - self.typing_level) * 0.15;
        snap.typing_level = self.typing_level;

        // --- window terrain and looms ---
        let windows = self.enumerate();
        let mut ids = HashSet::new();
        for w in &windows {
            ids.insert(w.hwnd as i64);
            let r = Rect::new(w.frame.left, w.frame.top, w.frame.right, w.frame.bottom);
            if let Some(l) = space.ledge_from_window(&r, w.hwnd as i64) {
                if snap.ledges.len() < 12 {
                    snap.ledges.push(l);
                }
            }
            if !self.first_poll && !self.known_windows.contains(&(w.hwnd as i64)) {
                snap.new_windows.push(WindowLoom {
                    center: space.to_scene(r.center_x(), r.center_y()),
                    size: r.width().max(r.height()) as f32,
                });
            }
        }
        if !self.first_poll {
            for old in self.known_windows.difference(&ids) {
                snap.closed_windows.push(*old);
            }
        }
        self.known_windows = ids;
        self.first_poll = false;

        snap.machine_heat = self.machine_heat();
        snap.local_hour = Self::local_hour();
        snap.fullscreen_app_active = Self::fullscreen_app_active();
        snap.foreground = Self::foreground_class();
        snap.build_events = self.notify.drain();
        snap
    }

    fn fidelity_notes(&self) -> &'static [&'static str] {
        &[
            "Typing detection is INFERRED, not measured: GetLastInputInfo returns one \
             combined keyboard+mouse timestamp, and the APIs that separate them also \
             reveal which key was pressed. Inferred as: recent input AND stationary cursor.",
            "\"Temperature\" is machine LOAD, not heat: Windows has no equivalent of \
             ProcessInfo.thermalState. Derived from processor clock throttling \
             (CurrentMhz/MaxMhz) blended with system CPU load.",
            "Very fast clicks can be missed: clicks are polled via GetAsyncKeyState \
             rather than a WH_MOUSE_LL hook, deliberately, so the app does not look \
             like a keylogger to security software.",
        ]
    }
}
