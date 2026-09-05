//! Dump one second of live desktop senses, so the platform layer can be
//! verified without a renderer. The macOS equivalents are in Environment.swift.

use dfcore::env::{Rect, ScreenSpace, Senses};
use dfplatform::HostSenses;

fn main() {
    // Primary monitor; the real shell enumerates displays.
    let display = Rect::new(0, 0, 2560, 1440);
    let space = ScreenSpace::new(display);
    let mut senses = HostSenses::new();

    println!("fidelity notes:");
    for n in senses.fidelity_notes() {
        println!("  - {n}");
    }
    println!();

    for i in 0..30 {
        let s = senses.poll(&space);
        if i == 0 {
            // The first poll seeds the known-window set, so looms and closes
            // are meaningless on it.
            continue;
        }
        if i % 10 != 0 {
            std::thread::sleep(std::time::Duration::from_millis(33));
            continue;
        }
        println!(
            "cursor {:?}  vel ({:.0},{:.0})  idle {:.1}s  typing {:.2}  heat {:.2}  hour {:.2}  fullscreen {}",
            s.cursor.map(|c| (c.x as i32, c.y as i32)),
            s.cursor_vel.x, s.cursor_vel.y, s.idle_secs, s.typing_level,
            s.machine_heat, s.local_hour, s.fullscreen_app_active
        );
        println!("  ledges: {}", s.ledges.len());
        for l in s.ledges.iter().take(6) {
            println!("    y={:>7.1}  x {:>7.1}..{:<7.1}  id=0x{:X}", l.y, l.x0, l.x1, l.id);
        }
        if !s.new_windows.is_empty() {
            println!("  NEW WINDOWS (looms): {}", s.new_windows.len());
        }
        if !s.closed_windows.is_empty() {
            println!("  CLOSED: {:?}", s.closed_windows.len());
        }
        if !s.clicks.is_empty() {
            println!("  CLICKS: {:?}", s.clicks.len());
        }
        std::thread::sleep(std::time::Duration::from_millis(33));
    }
}
