//! System-tray item and menu — the Windows equivalent of the macOS menu-bar
//! `NSStatusItem` (main.swift:829).
//!
//! Parity matters here for a mundane reason: the overlay is click-through and
//! has no taskbar button, so **the tray is the only way to quit**. Until this
//! existed the only exit was Task Manager.
//!
//! `tray-icon` wraps `Shell_NotifyIcon` on Windows and `NSStatusItem` on macOS,
//! so the same code serves both shells (PORT_PLAN.md §4).

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use dfcore::CREATURE_IDS;

/// What the user picked. Kept as an enum so the app loop stays declarative and
/// this file owns nothing but presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    TogglePause,
    ToggleBrain,
    EscapeTest,
    Scare,
    NextDisplay,
    ToggleShadows,
    ForgetMe,
    /// The "Creature" submenu: switch to this creature id.
    SelectCreature(&'static str),
    /// Glass anatomy on/off: the "cyber" look versus the literal animal.
    ToggleGlass,
    /// Confine the creature to a rendered enclosure, or let it have the screen.
    ToggleHabitat,
    Quit,
}

pub struct Tray {
    _icon: TrayIcon,
    pause: MenuItem,
    /// Which data the running creature is on.
    info: MenuItem,
    /// Shows what the creature currently thinks of you.
    mood: MenuItem,
    /// One check item per creature, in `CREATURE_IDS` order.
    creatures: Vec<(&'static str, CheckMenuItem)>,
    glass: CheckMenuItem,
    habitat: CheckMenuItem,
    ids: Vec<(tray_icon::menu::MenuId, TrayCommand)>,
}

/// A 32x32 fly silhouette, generated rather than shipped as an asset — the
/// macOS build just uses the emoji, and Windows needs real pixels.
fn fly_icon_rgba() -> Vec<u8> {
    const N: i32 = 32;
    let mut px = vec![0u8; (N * N * 4) as usize];
    let put = |px: &mut Vec<u8>, x: i32, y: i32, c: [u8; 4]| {
        if x < 0 || y < 0 || x >= N || y >= N {
            return;
        }
        let i = ((y * N + x) * 4) as usize;
        // Simple source-over so overlapping parts blend.
        let a = c[3] as f32 / 255.0;
        for k in 0..3 {
            px[i + k] = (c[k] as f32 * a + px[i + k] as f32 * (1.0 - a)) as u8;
        }
        px[i + 3] = px[i + 3].max(c[3]);
    };

    let ellipse = |px: &mut Vec<u8>, cx: f32, cy: f32, rx: f32, ry: f32, c: [u8; 4]| {
        let x0 = (cx - rx).floor() as i32;
        let x1 = (cx + rx).ceil() as i32;
        let y0 = (cy - ry).floor() as i32;
        let y1 = (cy + ry).ceil() as i32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = (x as f32 + 0.5 - cx) / rx;
                let dy = (y as f32 + 0.5 - cy) / ry;
                let d = dx * dx + dy * dy;
                if d <= 1.0 {
                    // Soften the rim so it does not look aliased in the tray.
                    let edge = ((1.0 - d) * 6.0).min(1.0);
                    let mut cc = c;
                    cc[3] = (c[3] as f32 * edge) as u8;
                    put(px, x, y, cc);
                }
            }
        }
    };

    let body = [70u8, 54, 32, 255];
    let wing = [200u8, 210, 225, 130];
    // Wings first, so the body sits over them.
    ellipse(&mut px, 9.0, 15.0, 6.5, 3.2, wing);
    ellipse(&mut px, 23.0, 15.0, 6.5, 3.2, wing);
    ellipse(&mut px, 16.0, 20.0, 5.0, 7.0, body); // abdomen
    ellipse(&mut px, 16.0, 12.0, 4.6, 4.6, body); // thorax
    ellipse(&mut px, 16.0, 7.0, 3.4, 3.0, [86, 66, 40, 255]); // head
    ellipse(&mut px, 13.2, 6.4, 1.8, 2.0, [150, 30, 24, 255]); // eyes
    ellipse(&mut px, 18.8, 6.4, 1.8, 2.0, [150, 30, 24, 255]);
    // Legs.
    for (sx, sy, ex, ey) in [
        (12.0f32, 11.0f32, 5.0f32, 7.0f32),
        (20.0, 11.0, 27.0, 7.0),
        (12.0, 14.0, 4.0, 15.0),
        (20.0, 14.0, 28.0, 15.0),
        (13.0, 17.0, 6.0, 23.0),
        (19.0, 17.0, 26.0, 23.0),
    ] {
        let steps = 18;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            put(
                &mut px,
                (sx + (ex - sx) * t) as i32,
                (sy + (ey - sy) * t) as i32,
                [50, 38, 22, 235],
            );
        }
    }
    px
}

impl Tray {
    /// `current` is the running creature's id; `data_info` its data line.
    pub fn new(data_info: &str, current: &str, glass_on: bool, habitat_on: bool) -> Option<Self> {
        let menu = Menu::new();

        let title = MenuItem::new("DesktopFly", false, None);
        let info = MenuItem::new(data_info, false, None);
        let pause = MenuItem::new("Pause", true, None);
        let brain = MenuItem::new("Show/Hide Brain", true, None);
        let escape = MenuItem::new("Escape Test (startle)", true, None);
        let scare = MenuItem::new("Scare It", true, None);
        let display = MenuItem::new("Move to Next Display", true, None);
        let shadow = MenuItem::new("Toggle Shadow", true, None);
        let glass = CheckMenuItem::new("Glass Anatomy (see the circuit)", true, glass_on, None);
        let habitat = CheckMenuItem::new("Habitat (confine to a tank)", true, habitat_on, None);
        let mood = MenuItem::new("getting to know you", false, None);
        let forget = MenuItem::new("Forget Me (reset habituation)", true, None);
        let quit = MenuItem::new("Quit", true, None);

        // The creature picker. Check items rather than plain ones so the menu
        // shows which animal is running without a separate status line.
        let creatures: Vec<(&'static str, CheckMenuItem)> = CREATURE_IDS
            .iter()
            .map(|id| {
                let name = dfcore::by_id(id)
                    .map(|c| c.display_name())
                    .unwrap_or(id);
                (*id, CheckMenuItem::new(name, true, *id == current, None))
            })
            .collect();
        let submenu = Submenu::new("Creature", true);
        for (_, item) in &creatures {
            submenu.append(item).ok()?;
        }

        let mut ids = vec![
            (pause.id().clone(), TrayCommand::TogglePause),
            (brain.id().clone(), TrayCommand::ToggleBrain),
            (escape.id().clone(), TrayCommand::EscapeTest),
            (scare.id().clone(), TrayCommand::Scare),
            (display.id().clone(), TrayCommand::NextDisplay),
            (shadow.id().clone(), TrayCommand::ToggleShadows),
            (glass.id().clone(), TrayCommand::ToggleGlass),
            (habitat.id().clone(), TrayCommand::ToggleHabitat),
            (forget.id().clone(), TrayCommand::ForgetMe),
            (quit.id().clone(), TrayCommand::Quit),
        ];
        for (id, item) in &creatures {
            ids.push((item.id().clone(), TrayCommand::SelectCreature(id)));
        }

        menu.append_items(&[
            &title,
            &info,
            &submenu,
            &PredefinedMenuItem::separator(),
            &pause,
            &brain,
            &escape,
            &scare,
            &display,
            &shadow,
            &glass,
            &habitat,
            &PredefinedMenuItem::separator(),
            &mood,
            &forget,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .ok()?;

        let icon = Icon::from_rgba(fly_icon_rgba(), 32, 32).ok()?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("DesktopFly - a real connectome on your desktop")
            .with_icon(icon)
            .build()
            .ok()?;

        Some(Tray {
            _icon: tray,
            pause,
            info,
            mood,
            creatures,
            glass,
            habitat,
            ids,
        })
    }

    pub fn set_glass(&self, on: bool) {
        self.glass.set_checked(on);
    }

    pub fn set_habitat(&self, on: bool) {
        self.habitat.set_checked(on);
    }

    /// Reflect a creature switch: tick the right item, update the data line.
    pub fn set_creature(&self, id: &str, data_info: &str) {
        for (cid, item) in &self.creatures {
            item.set_checked(*cid == id);
        }
        self.info.set_text(data_info);
    }

    /// Drain any menu activations since the last frame.
    pub fn poll(&self) -> Vec<TrayCommand> {
        let mut out = Vec::new();
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if let Some((_, cmd)) = self.ids.iter().find(|(id, _)| *id == ev.id) {
                out.push(*cmd);
            }
        }
        out
    }

    pub fn set_paused(&self, paused: bool) {
        self.pause.set_text(if paused { "Resume" } else { "Pause" });
    }

    /// Surface the habituation state, so it is legible rather than a hidden
    /// number the user can only infer from behaviour.
    pub fn set_mood(&self, text: &str) {
        self.mood.set_text(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generated_icon_is_a_valid_32x32_rgba_image() {
        let px = fly_icon_rgba();
        assert_eq!(px.len(), 32 * 32 * 4);
        let opaque = px.chunks(4).filter(|c| c[3] > 20).count();
        // Enough ink to be a recognisable mark, not so much it is a blob.
        assert!(
            (150..900).contains(&opaque),
            "{opaque} visible pixels in the tray icon"
        );
        // The corners must stay transparent or the tray shows a square.
        for (x, y) in [(0, 0), (31, 0), (0, 31), (31, 31)] {
            let i = ((y * 32 + x) * 4) as usize;
            assert_eq!(px[i + 3], 0, "corner ({x},{y}) is not transparent");
        }
    }
}
