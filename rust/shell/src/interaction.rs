//! A native companion panel. Only this ordinary window accepts clicks;
//! the desktop overlay remains click-through. Placement uses a labelled map.
use dfcore::Vec2;

#[derive(Clone, Copy)]
pub enum Command {
    Offer(Vec2),
    Move(usize, Vec2),
    Warm(Option<u8>),
    Stop,
    Quiet,
}

#[cfg(target_os = "windows")]
mod native {
    use super::*;
    use std::sync::Mutex;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::*;
    use windows::Win32::UI::WindowsAndMessaging::*;
    static EVENTS: Mutex<Vec<u16>> = Mutex::new(Vec::new());
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_COMMAND => {
                if (wp.0 >> 16) == 0 {
                    if let Ok(mut q) = EVENTS.lock() {
                        q.push((wp.0 & 65535) as u16);
                    }
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
    pub struct Panel {
        hwnd: HWND,
        status: HWND,
        hint: HWND,
        props: HWND,
        offer: HWND,
        warm: Vec<HWND>,
        mode: bool,
        last_status: String,
    }
    impl Panel {
        pub fn new() -> Option<Self> {
            unsafe {
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(proc),
                    lpszClassName: w!("DesktopFlyInteractions"),
                    hCursor: LoadCursorW(None, IDC_ARROW).ok()?,
                    hbrBackground: windows::Win32::Graphics::Gdi::GetSysColorBrush(
                        windows::Win32::Graphics::Gdi::COLOR_WINDOW,
                    ),
                    ..Default::default()
                };
                RegisterClassW(&wc);
                let hwnd = CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    wc.lpszClassName,
                    w!("Habitat interactions"),
                    WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    510,
                    620,
                    None,
                    None,
                    None,
                    None,
                )
                .ok()?;
                let control = |class: PCWSTR,
                               label: &str,
                               id: usize,
                               x,
                               y,
                               width,
                               height,
                               style: WINDOW_STYLE|
                 -> Option<HWND> {
                    let text = wide(label);
                    let child = CreateWindowExW(
                        WINDOW_EX_STYLE::default(),
                        class,
                        PCWSTR(text.as_ptr()),
                        WS_CHILD | WS_VISIBLE | style,
                        x,
                        y,
                        width,
                        height,
                        Some(hwnd),
                        Some(HMENU(id as *mut _)),
                        None,
                        None,
                    )
                    .ok()?;
                    let font = windows::Win32::Graphics::Gdi::GetStockObject(
                        windows::Win32::Graphics::Gdi::DEFAULT_GUI_FONT,
                    );
                    SendMessageW(
                        child,
                        WM_SETFONT,
                        Some(WPARAM(font.0 as usize)),
                        Some(LPARAM(1)),
                    );
                    Some(child)
                };
                control(
                    w!("STATIC"),
                    "Offer an opportunity. The animal decides how to respond.",
                    0,
                    16,
                    16,
                    465,
                    24,
                    WINDOW_STYLE(0),
                )?;
                let status = control(
                    w!("STATIC"),
                    "Open a habitat to begin.",
                    0,
                    16,
                    46,
                    465,
                    64,
                    WINDOW_STYLE(0),
                )?;
                let offer = control(
                    w!("BUTTON"),
                    "Offer food / prey",
                    100,
                    16,
                    116,
                    220,
                    32,
                    WS_TABSTOP,
                )?;
                control(
                    w!("BUTTON"),
                    "Move selected furnishing",
                    101,
                    246,
                    116,
                    230,
                    32,
                    WS_TABSTOP,
                )?;
                let props = control(
                    w!("COMBOBOX"),
                    "",
                    110,
                    16,
                    157,
                    460,
                    210,
                    WS_TABSTOP | WINDOW_STYLE(CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0),
                )?;
                let hint = control(
                    w!("STATIC"),
                    "Choose an action, then a location below (top-down map).",
                    0,
                    16,
                    194,
                    460,
                    36,
                    WINDOW_STYLE(0),
                )?;
                let labels = [
                    "Back left",
                    "Back centre",
                    "Back right",
                    "Left",
                    "Centre",
                    "Right",
                    "Front left",
                    "Front centre",
                    "Front right",
                ];
                for (i, label) in labels.iter().enumerate() {
                    control(
                        w!("BUTTON"),
                        label,
                        200 + i,
                        16 + (i as i32 % 3) * 154,
                        238 + (i as i32 / 3) * 42,
                        148,
                        36,
                        WS_TABSTOP,
                    )?;
                }
                control(
                    w!("STATIC"),
                    "Hognose heat source (simulated, not measured temperature)",
                    0,
                    16,
                    374,
                    460,
                    24,
                    WINDOW_STYLE(0),
                )?;
                let mut warm = Vec::new();
                for (i, label) in ["Heat hide 1", "Heat hide 2", "Daily cycle"]
                    .iter()
                    .enumerate()
                {
                    warm.push(control(
                        w!("BUTTON"),
                        label,
                        102 + i,
                        16 + i as i32 * 154,
                        402,
                        148,
                        32,
                        WS_TABSTOP,
                    )?);
                }
                control(
                    w!("BUTTON"),
                    "Remove prey / stop thumper",
                    105,
                    16,
                    446,
                    270,
                    32,
                    WS_TABSTOP,
                )?;
                control(
                    w!("BUTTON"),
                    "Quiet observation",
                    106,
                    296,
                    446,
                    180,
                    32,
                    WS_TABSTOP,
                )?;
                control(w!("STATIC"), "Quiet observation toggles desktop disturbances off.\r\nFood and shelter do not force feeding or approach.\r\nThe placement map follows the tank, not its camera angle.", 0,16,494,460,66,WINDOW_STYLE(0))?;
                Some(Self {
                    hwnd,
                    status,
                    hint,
                    props,
                    offer,
                    warm,
                    mode: false,
                    last_status: String::new(),
                })
            }
        }
        pub fn active(&self) -> bool {
            unsafe { GetForegroundWindow() == self.hwnd }
        }
        pub fn show(&self) {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOW);
                let _ = SetForegroundWindow(self.hwnd);
            }
        }
        pub fn configure(&mut self, creature: &str, names: &[String]) {
            unsafe {
                let selected = SendMessageW(self.props, CB_GETCURSEL, None, None).0.max(0) as usize;
                SendMessageW(self.props, CB_RESETCONTENT, None, None);
                for name in names {
                    let text = wide(name);
                    SendMessageW(
                        self.props,
                        CB_ADDSTRING,
                        None,
                        Some(LPARAM(text.as_ptr() as isize)),
                    );
                }
                SendMessageW(
                    self.props,
                    CB_SETCURSEL,
                    Some(WPARAM(selected.min(names.len().saturating_sub(1)))),
                    None,
                );
                let label = wide(match creature {
                    "koi" => "Offer a food pellet",
                    "sandworm" => "Place a thumper",
                    "hognose" => "Offer / move preferred hide",
                    "salticid" | "araneus" | "parasteatoda" | "agelenopsis" => {
                        "Release one prey insect"
                    }
                    _ => "Offer / move food patch",
                });
                let _ = SetWindowTextW(self.offer, PCWSTR(label.as_ptr()));
                for h in &self.warm {
                    let _ = windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow(
                        *h,
                        creature == "hognose",
                    );
                }
                self.set_hint(if self.mode {
                    "Move furnishing: select an item, then its new location."
                } else {
                    "Offer: choose a location below (top-down map)."
                });
            }
        }
        fn set_hint(&self, text: &str) {
            unsafe {
                let text = wide(text);
                let _ = SetWindowTextW(self.hint, PCWSTR(text.as_ptr()));
            }
        }
        pub fn status(&mut self, text: &str) {
            if self.last_status != text {
                unsafe {
                    let t = wide(text);
                    let _ = SetWindowTextW(self.status, PCWSTR(t.as_ptr()));
                }
                self.last_status = text.to_string();
            }
        }
        pub fn poll(&mut self) -> Vec<Command> {
            let events = EVENTS
                .lock()
                .map(|mut q| std::mem::take(&mut *q))
                .unwrap_or_default();
            let mut out = Vec::new();
            for id in events {
                match id {
                    100 => {
                        self.mode = false;
                        self.set_hint("Choose where to offer food, prey, or a thumper.");
                    }
                    101 => {
                        self.mode = true;
                        self.set_hint("Select a furnishing above, then its new location.");
                    }
                    102 => out.push(Command::Warm(Some(0))),
                    103 => out.push(Command::Warm(Some(1))),
                    104 => out.push(Command::Warm(None)),
                    105 => out.push(Command::Stop),
                    106 => out.push(Command::Quiet),
                    200..=208 => {
                        let i = id - 200;
                        let at = Vec2::new((i % 3) as f32 * 0.6 - 0.6, 0.6 - (i / 3) as f32 * 0.6);
                        if self.mode {
                            let index =
                                unsafe { SendMessageW(self.props, CB_GETCURSEL, None, None).0 };
                            if index >= 0 {
                                out.push(Command::Move(index as usize, at));
                            }
                        } else {
                            out.push(Command::Offer(at));
                        }
                    }
                    _ => {}
                }
            }
            out
        }
    }
    impl Drop for Panel {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}
#[cfg(target_os = "windows")]
pub use native::Panel;

/// Returns feedback for the control panel. Coordinates are tank-relative.
pub fn apply(
    rt: &mut dyn crate::runtime::Runtime,
    h: &mut dfcore::Habitat,
    cmd: Command,
) -> &'static str {
    let point = |p: Vec2| {
        Vec2::new(
            h.region.center.x + p.x * h.region.size.0 * 0.5,
            h.region.center.y + p.y * h.region.size.1 * 0.5,
        )
    };
    match cmd {
        Command::Offer(p) => {
            let at = point(p);
            let ok = match rt.creature().id() {
                "koi" => h.offer_food(at),
                "drosophila" | "c_elegans" => {
                    let kind = if rt.creature().id() == "drosophila" {
                        dfcore::PropKind::Fruit
                    } else {
                        dfcore::PropKind::Lawn
                    };
                    if let Some(i) = h.props.iter().position(|p| p.kind == kind) {
                        if kind == dfcore::PropKind::Lawn {
                            h.props[i].pos = h.region.clamp_inside(at, h.props[i].radius + 6.0);
                            true
                        } else {
                            h.move_prop(i, at)
                        }
                    } else {
                        h.add(kind)
                    }
                }
                "hognose" => {
                    let variant = h.preferred_hide_variant();
                    if let Some(i) = h
                        .props
                        .iter()
                        .position(|p| p.kind == dfcore::PropKind::Hide && p.variant == variant)
                    {
                        h.move_prop(i, at)
                    } else {
                        h.add(dfcore::PropKind::Hide)
                    }
                }
                _ => {
                    let offered = rt.offer(at);
                    if offered && rt.creature().id() == "sandworm" {
                        h.thumper = Some(at);
                    }
                    offered
                }
            };
            if ok {
                "Opportunity placed. The animal may investigate or ignore it."
            } else {
                "No room for another offer. Remove existing food or prey first."
            }
        }
        Command::Move(i, p) => {
            let at = point(p);
            if h.move_prop(i, at) {
                "Furnishing moved; arrangement saved."
            } else {
                "This furnishing stays attached to its substrate."
            }
        }
        Command::Warm(end) => {
            h.warm_end = end;
            h.focus = None;
            "Heat source updated; the snake chooses its shelter."
        }
        Command::Stop => {
            rt.stop_interaction();
            h.thumper = None;
            "Loose prey removed / thumper stopped."
        }
        Command::Quiet => "Quiet observation changed.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn map_placement_tracks_an_offset_habitat_and_camera_independent_coordinates() {
        let mut rt = crate::koirt::KoiRuntime::new(2);
        let mut h = dfcore::Habitat::new(
            dfcore::HabitatKind::Pond,
            dfcore::Region::new(Vec2::new(350.0, -120.0), (600.0, 420.0)),
            2,
        );
        let i = h
            .props
            .iter()
            .position(|p| p.kind == dfcore::PropKind::Food)
            .unwrap();
        h.props[i].respawn = f32::INFINITY;
        apply(&mut rt, &mut h, Command::Offer(Vec2::new(0.6, -0.6)));
        assert!(h.props[i].pos.dist(Vec2::new(530.0, -246.0)) < 0.01);
    }
}

#[cfg(not(target_os = "windows"))]
pub struct Panel;
#[cfg(not(target_os = "windows"))]
impl Panel {
    pub fn new() -> Option<Self> {
        None
    }
    pub fn active(&self) -> bool {
        false
    }
    pub fn show(&self) {}
    pub fn configure(&mut self, _: &str, _: &[String]) {}
    pub fn status(&mut self, _: &str) {}
    pub fn poll(&mut self) -> Vec<Command> {
        Vec::new()
    }
}
