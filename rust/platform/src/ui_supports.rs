//! Geometry-only UI Automation sampling. No names, text, values or screenshots.
//! COM providers may block; a single bounded worker isolates them from animation.
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Ole::{
    SafeArrayDestroy, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound,
};
use windows::Win32::UI::Accessibility::*;

#[derive(Clone, Copy)]
pub struct Control {
    pub id: i64,
    pub rect: RECT,
}
struct Sample {
    hwnd: isize,
    frame: RECT,
    at: Instant,
    controls: Vec<Control>,
}
pub struct UiSupports {
    request: SyncSender<(isize, RECT)>,
    response: Receiver<Sample>,
    cache: HashMap<isize, Sample>,
    next: Instant,
    turn: usize,
}
impl UiSupports {
    pub fn new() -> Self {
        let (request, rx) = mpsc::sync_channel::<(isize, RECT)>(1);
        let (tx, response) = mpsc::sync_channel(1);
        std::thread::spawn(move || unsafe {
            if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                return;
            }
            if let Ok(ui) =
                CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            {
                while let Ok((hwnd, frame)) = rx.recv() {
                    let at = Instant::now();
                    let controls = scan(&ui, hwnd, frame);
                    if tx
                        .send(Sample {
                            hwnd,
                            frame,
                            controls,
                            at,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
            CoUninitialize();
        });
        Self {
            request,
            response,
            cache: HashMap::new(),
            next: Instant::now(),
            turn: 0,
        }
    }
    pub fn sample_windows(&mut self, windows: &[(isize, RECT)]) {
        while let Ok(sample) = self.response.try_recv() {
            self.cache.insert(sample.hwnd, sample);
        }
        self.cache
            .retain(|_, s| s.at.elapsed() < Duration::from_secs(3));
        if Instant::now() >= self.next && !windows.is_empty() {
            let target = windows[self.turn % windows.len().min(3)];
            let _ = self.request.try_send(target);
            self.turn = self.turn.wrapping_add(1);
            self.next = Instant::now() + Duration::from_millis(750);
        }
    }
    pub fn cached(&self, hwnd: isize, frame: RECT) -> Vec<Control> {
        self.cache
            .get(&hwnd)
            .filter(|s| {
                s.hwnd == hwnd
                    && same_rect(s.frame, frame)
                    && s.at.elapsed() < Duration::from_secs(3)
            })
            .map(|s| s.controls.clone())
            .unwrap_or_default()
    }
    #[cfg(test)]
    fn poll(&mut self, hwnd: isize, frame: RECT) -> Vec<Control> {
        self.sample_windows(&[(hwnd, frame)]);
        self.cached(hwnd, frame)
    }
}
fn same_rect(a: RECT, b: RECT) -> bool {
    a.left == b.left && a.right == b.right && a.top == b.top && a.bottom == b.bottom
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_controls_are_invalidated_when_the_window_moves_or_data_expires() {
        let (request, _rx) = mpsc::sync_channel(1);
        let (_tx, response) = mpsc::sync_channel(1);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 800,
            bottom: 600,
        };
        let mut ui = UiSupports {
            request,
            response,
            next: Instant::now() + Duration::from_secs(20),
            turn: 0,
            cache: HashMap::from([(
                7,
                Sample {
                    hwnd: 7,
                    frame: rect,
                    at: Instant::now(),
                    controls: vec![Control { id: 99, rect }],
                },
            )]),
        };
        assert_eq!(ui.poll(7, rect).len(), 1);
        assert!(ui.poll(8, rect).is_empty());
        assert!(ui.poll(7, RECT { left: 10, ..rect }).is_empty());
        ui.cache.get_mut(&7).unwrap().at = Instant::now() - Duration::from_secs(4);
        assert!(ui.poll(7, rect).is_empty());
    }
    #[test]
    fn live_accessibility_sampling_is_bounded_and_returns_only_rectangles() {
        unsafe {
            let hwnd = windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
            if hwnd.is_invalid() {
                return;
            }
            let mut rect = RECT::default();
            if windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect).is_err() {
                return;
            }
            let mut ui = UiSupports::new();
            let mut count = 0;
            for _ in 0..8 {
                let sample = ui.poll(hwnd.0 as isize, rect);
                count = count.max(sample.len());
                for c in sample {
                    assert!(c.rect.left >= rect.left && c.rect.right <= rect.right);
                    assert!(c.rect.top >= rect.top && c.rect.bottom <= rect.bottom);
                    assert!(c.rect.right > c.rect.left && c.rect.bottom > c.rect.top);
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            eprintln!("UI geometry smoke test: {count} accessible outlines");
        }
    }
}
unsafe fn identity(e: &IUIAutomationElement, hwnd: isize) -> Option<i64> {
    let array = e.GetRuntimeId().ok()?;
    if array.is_null() {
        return None;
    }
    let result = (|| {
        let lo = SafeArrayGetLBound(array, 1).ok()?;
        let hi = SafeArrayGetUBound(array, 1).ok()?;
        if hi < lo || hi - lo > 64 {
            return None;
        }
        let mut hash = 1469598103934665603u64 ^ hwnd as u64;
        for i in lo..=hi {
            let mut value = 0i32;
            SafeArrayGetElement(array, &i, &mut value as *mut _ as _).ok()?;
            hash = (hash ^ value as u64).wrapping_mul(1099511628211);
        }
        // Positive ids, separate from ordinary HWNDs and negative habitat ids.
        Some(((hash & 0x1fff_ffff_ffff_ffff) | 0x4000_0000_0000_0000) as i64)
    })();
    let _ = SafeArrayDestroy(array);
    result
}
#[allow(non_upper_case_globals)]
unsafe fn scan(ui: &IUIAutomation, hwnd: isize, frame: RECT) -> Vec<Control> {
    let Ok(root) = ui.ElementFromHandle(HWND(hwnd as _)) else {
        return Vec::new();
    };
    let Ok(walker) = ui.ControlViewWalker() else {
        return Vec::new();
    };
    let mut queue = VecDeque::from([(root, 0)]);
    let start = Instant::now();
    let mut found = HashMap::new();
    let mut visited = 0;
    while let Some((e, depth)) = queue.pop_front() {
        visited += 1;
        if visited > 320 || start.elapsed() > Duration::from_millis(300) {
            break;
        }
        if e.CurrentIsOffscreen().map(|v| v.as_bool()).unwrap_or(true) {
            continue;
        }
        // Only outlines. Password fields are also excluded even though no value is requested.
        let kind = e.CurrentControlType().ok();
        if depth > 0
            && !e.CurrentIsPassword().map(|v| v.as_bool()).unwrap_or(true)
            && kind.is_some_and(|k| {
                [
                    UIA_ButtonControlTypeId,
                    UIA_EditControlTypeId,
                    UIA_TextControlTypeId,
                    UIA_ImageControlTypeId,
                    UIA_HyperlinkControlTypeId,
                    UIA_TabItemControlTypeId,
                    UIA_ListItemControlTypeId,
                    UIA_TreeItemControlTypeId,
                    UIA_ScrollBarControlTypeId,
                    UIA_ToolBarControlTypeId,
                ]
                .contains(&k)
            })
        {
            if let (Ok(r), Some(id)) = (e.CurrentBoundingRectangle(), identity(&e, hwnd)) {
                let r = RECT {
                    left: r.left.max(frame.left),
                    top: r.top.max(frame.top),
                    right: r.right.min(frame.right),
                    bottom: r.bottom.min(frame.bottom),
                };
                if r.right - r.left >= 20 && r.bottom - r.top >= 8 {
                    found.insert(id, Control { id, rect: r });
                }
            }
        }
        if depth < 12 && queue.len() < 320 {
            if let Ok(mut child) = walker.GetFirstChildElement(&e) {
                loop {
                    queue.push_back((child.clone(), depth + 1));
                    if queue.len() >= 320 {
                        break;
                    }
                    match walker.GetNextSiblingElement(&child) {
                        Ok(next) => child = next,
                        Err(_) => break,
                    }
                }
            }
        }
    }
    let mut out: Vec<_> = found.into_values().collect();
    out.sort_by_key(|c| {
        (
            ((c.rect.right - c.rect.left) as i64 * (c.rect.bottom - c.rect.top) as i64),
            c.id,
        )
    });
    out.truncate(96);
    out
}
