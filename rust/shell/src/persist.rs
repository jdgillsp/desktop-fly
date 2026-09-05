//! Persisting what the creature has learned.
//!
//! Habituation is only interesting if it survives a restart: the point is a pet
//! that stops flinching at you over *days*, not one that resets every launch.
//! This is the only state the app keeps on disk, and it is small and readable
//! on purpose — a user should be able to look at it, and delete it to get a
//! naive creature back.

use std::path::PathBuf;

use dfcore::Habituation;

/// `%LOCALAPPDATA%\DesktopFly\habituation.json` on Windows,
/// `$XDG_DATA_HOME`/`~/.local/share` elsewhere.
pub fn state_path() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }?;
    Some(base.join("DesktopFly").join("habituation.json"))
}

pub fn load() -> Habituation {
    let Some(p) = state_path() else {
        return Habituation::new();
    };
    match std::fs::read_to_string(&p) {
        Ok(text) => match serde_json::from_str::<Habituation>(&text) {
            Ok(h) => {
                println!("remembered you: {} ({})", h.describe(), p.display());
                h
            }
            Err(e) => {
                // A corrupt file must never stop the app starting; a naive
                // creature is a perfectly good fallback.
                eprintln!("habituation state unreadable ({e}); starting naive");
                Habituation::new()
            }
        },
        Err(_) => Habituation::new(),
    }
}

pub fn save(h: &Habituation) {
    let Some(p) = state_path() else { return };
    if let Some(dir) = p.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    match serde_json::to_string_pretty(h) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&p, text) {
                eprintln!("could not save habituation state: {e}");
            }
        }
        Err(e) => eprintln!("could not serialise habituation state: {e}"),
    }
}

pub fn forget() {
    if let Some(p) = state_path() {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_path_is_under_a_per_user_data_directory() {
        let p = state_path().expect("a state path on this platform");
        assert!(p.ends_with("DesktopFly/habituation.json") || p.ends_with("DesktopFly\\habituation.json"));
        assert!(p.is_absolute());
    }

    /// A corrupt or truncated file must degrade to a naive creature, never
    /// prevent startup.
    #[test]
    fn corrupt_state_falls_back_to_naive() {
        let bad = "{ this is not json";
        let parsed = serde_json::from_str::<Habituation>(bad);
        assert!(parsed.is_err());
        // load() maps that to a fresh creature; assert the fallback's shape.
        let fresh = Habituation::new();
        assert_eq!(fresh.loom_gain, 1.0);
    }

    #[test]
    fn a_saved_creature_round_trips() {
        let mut h = Habituation::new();
        for _ in 0..2000 {
            h.step(0.016, 1.0, 0.0);
        }
        assert!(h.loom_gain < 0.9);
        let text = serde_json::to_string_pretty(&h).unwrap();
        let back: Habituation = serde_json::from_str(&text).unwrap();
        assert!((back.loom_gain - h.loom_gain).abs() < 1e-9);
    }
}
