//! Per-OS implementations of `dfcore::Senses`.
//!
//! This is the only crate allowed to know what an operating system is.
//! Everything it produces is a plain `EnvSnapshot`; everything downstream is
//! portable (PORT_PLAN.md §5).

#[cfg(target_os = "windows")]
mod windows_senses;

#[cfg(target_os = "windows")]
pub use windows_senses::WindowsSenses;

/// The `Senses` implementation for the host platform.
#[cfg(target_os = "windows")]
pub type HostSenses = WindowsSenses;

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use dfcore::env::{Rect, ScreenSpace, Senses};

    /// Smoke test against the live desktop: polling must not panic, and every
    /// value must be in range. Most of the risky maths is unit-tested in
    /// `dfcore::env`; this checks the FFI plumbing.
    #[test]
    fn polling_the_real_desktop_returns_sane_values() {
        let space = ScreenSpace::new(Rect::new(0, 0, 2560, 1440));
        let mut s = HostSenses::new();
        let first = s.poll(&space);
        // The first poll seeds the window set, so it must report no looms.
        assert!(first.new_windows.is_empty(), "first poll must not report looms");
        assert!(first.closed_windows.is_empty());

        let snap = s.poll(&space);
        assert!(snap.idle_secs >= 0.0);
        assert!((0.0..=1.0).contains(&snap.typing_level));
        assert!((0.0..=1.0).contains(&snap.machine_heat));
        assert!((0.0..24.0).contains(&snap.local_hour));
        assert!(snap.ledges.len() <= 12, "ledge list is capped at 12");
        for l in &snap.ledges {
            assert!(l.x1 > l.x0, "ledge must have positive width");
        }
        assert!(!s.fidelity_notes().is_empty(), "must disclose fidelity losses");
    }
}
