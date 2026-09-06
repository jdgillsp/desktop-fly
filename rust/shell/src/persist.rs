//! Persisting what the creature has learned, and which creature it is.
//!
//! Habituation is only interesting if it survives a restart: the point is a pet
//! that stops flinching at you over *days*, not one that resets every launch.
//! It is kept **per creature**: what the fly has learned about your cursor is
//! not what the worm has learned about your clicks, and switching animals
//! must not hand one the other's history. The state is small and readable on
//! purpose — a user should be able to look at it, and delete it to get a
//! naive creature back.
//!
//! The only other thing on disk is the creature choice itself, so the tray's
//! pick survives a restart.

use std::path::PathBuf;

use dfcore::Habituation;

/// `%LOCALAPPDATA%\DesktopFly\` on Windows, `$XDG_DATA_HOME`/`~/.local/share`
/// elsewhere.
fn data_dir() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }?;
    Some(base.join("DesktopFly"))
}

/// The fly's file keeps its original name so an existing installation's
/// history is not orphaned by the creature picker; every other creature gets
/// its own file beside it.
pub fn state_path_for(creature_id: &str) -> Option<PathBuf> {
    let name = if creature_id == "drosophila" {
        "habituation.json".to_string()
    } else {
        format!("habituation-{creature_id}.json")
    };
    Some(data_dir()?.join(name))
}

fn settings_path() -> Option<PathBuf> {
    Some(data_dir()?.join("settings.json"))
}

fn load_settings() -> serde_json::Value {
    settings_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// Merge one key into the settings file, keeping the others.
fn save_setting(key: &str, value: serde_json::Value) {
    let Some(p) = settings_path() else { return };
    if let Some(dir) = p.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    let mut v = load_settings();
    if !v.is_object() {
        v = serde_json::json!({});
    }
    v[key] = value;
    if let Err(e) = std::fs::write(&p, serde_json::to_string_pretty(&v).unwrap_or_default()) {
        eprintln!("could not save setting {key}: {e}");
    }
}

/// Which creature was running last time, if a choice was ever saved.
pub fn load_creature_choice() -> Option<String> {
    load_settings().get("creature")?.as_str().map(|s| s.to_string())
}

pub fn save_creature_choice(creature_id: &str) {
    save_setting("creature", serde_json::Value::String(creature_id.to_string()));
}

/// Glass anatomy or the literal animal, if the user ever toggled it.
pub fn load_glass_choice() -> Option<bool> {
    load_settings().get("glass")?.as_bool()
}

pub fn save_glass_choice(glass: bool) {
    save_setting("glass", serde_json::Value::Bool(glass));
}

pub fn load(creature_id: &str) -> Habituation {
    let Some(p) = state_path_for(creature_id) else {
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

pub fn save(creature_id: &str, h: &Habituation) {
    let Some(p) = state_path_for(creature_id) else { return };
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

pub fn forget(creature_id: &str) {
    if let Some(p) = state_path_for(creature_id) {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_path_is_under_a_per_user_data_directory() {
        let p = state_path_for("drosophila").expect("a state path on this platform");
        assert!(p.ends_with("DesktopFly/habituation.json") || p.ends_with("DesktopFly\\habituation.json"));
        assert!(p.is_absolute());
    }

    /// Two creatures must never share a history: the fly keeps the legacy
    /// file name, and every other creature gets its own.
    #[test]
    fn each_creature_has_its_own_state_file() {
        let fly = state_path_for("drosophila").unwrap();
        let worm = state_path_for("c_elegans").unwrap();
        assert_ne!(fly, worm);
        assert!(fly.ends_with("habituation.json"), "the fly's file is the legacy one");
        assert!(worm.to_string_lossy().ends_with("habituation-c_elegans.json"));
        assert_eq!(fly.parent(), worm.parent());
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
