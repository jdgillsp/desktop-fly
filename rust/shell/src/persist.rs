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

use dfcore::{Habituation, PropSave};

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
    load_settings()
        .get("creature")?
        .as_str()
        .map(|s| s.to_string())
}

pub fn save_creature_choice(creature_id: &str) {
    save_setting(
        "creature",
        serde_json::Value::String(creature_id.to_string()),
    );
}

/// Glass anatomy or the literal animal, if the user ever toggled it.
pub fn load_glass_choice() -> Option<bool> {
    load_settings().get("glass")?.as_bool()
}

pub fn save_glass_choice(glass: bool) {
    save_setting("glass", serde_json::Value::Bool(glass));
}

/// Whether the creature is confined to an enclosure. Off unless the user has
/// turned it on, so an existing install is unchanged by the feature existing.
pub fn load_habitat_choice() -> Option<bool> {
    load_settings().get("habitat")?.as_bool()
}

pub fn save_habitat_choice(on: bool) {
    save_setting("habitat", serde_json::Value::Bool(on));
}

/// Whether the habitat camera is a close-up on the animal.
pub fn load_closeup_choice() -> Option<bool> {
    load_settings().get("habitat_closeup")?.as_bool()
}

pub fn save_closeup_choice(on: bool) {
    save_setting("habitat_closeup", serde_json::Value::Bool(on));
}

/// How the user has angled and sized the enclosure, as `(pitch, yaw, zoom)`.
///
/// Returned raw: the caller clamps. Keeping the clamp in one place
/// (`HabitatView`) rather than two means a hand-edited settings file cannot
/// produce a projection the app has never validated.
pub fn load_habitat_view() -> Option<(f32, f32, f32)> {
    let v = load_settings();
    let o = v.get("habitat_view")?;
    let f = |k: &str| o.get(k).and_then(|x| x.as_f64()).map(|x| x as f32);
    Some((f("pitch")?, f("yaw")?, f("zoom")?))
}

pub fn save_habitat_view(pitch: f32, yaw: f32, zoom: f32) {
    save_setting(
        "habitat_view",
        serde_json::json!({ "pitch": pitch, "yaw": yaw, "zoom": zoom }),
    );
}

/// The contents of one kind of enclosure, keyed by that kind rather than by the
/// creature: a pond is a pond whichever fish is in it, and two creatures that
/// share a substrate should share the tank they were given.
///
/// `None` means "never arranged" and the enclosure stocks itself. That is not
/// the same as `Some(vec![])`, which is an enclosure the user deliberately
/// emptied — restocking that on every launch would make emptying it impossible.
pub fn load_habitat_contents(habitat_slug: &str) -> Option<Vec<PropSave>> {
    let v = load_settings();
    let entry = v.get("habitat_contents")?.get(habitat_slug)?;
    match serde_json::from_value::<Vec<PropSave>>(entry.clone()) {
        Ok(props) => Some(props),
        Err(e) => {
            // Same reasoning as the habituation file: unreadable settings must
            // never stop the app, and a default enclosure is a fine fallback.
            eprintln!("saved {habitat_slug} contents unreadable ({e}); using the default");
            None
        }
    }
}

pub fn save_habitat_contents(habitat_slug: &str, props: &[PropSave]) {
    let mut all = load_settings()
        .get("habitat_contents")
        .cloned()
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}));
    all[habitat_slug] = serde_json::to_value(props).unwrap_or(serde_json::Value::Null);
    save_setting("habitat_contents", all);
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
    let Some(p) = state_path_for(creature_id) else {
        return;
    };
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
        assert!(
            p.ends_with("DesktopFly/habituation.json")
                || p.ends_with("DesktopFly\\habituation.json")
        );
        assert!(p.is_absolute());
    }

    /// Two creatures must never share a history: the fly keeps the legacy
    /// file name, and every other creature gets its own.
    #[test]
    fn each_creature_has_its_own_state_file() {
        let fly = state_path_for("drosophila").unwrap();
        let worm = state_path_for("c_elegans").unwrap();
        assert_ne!(fly, worm);
        assert!(
            fly.ends_with("habituation.json"),
            "the fly's file is the legacy one"
        );
        assert!(worm
            .to_string_lossy()
            .ends_with("habituation-c_elegans.json"));
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

/// No off-line starvation: appetite advances only while the simulation runs.
pub fn save_appetite(id: &str, value: f32) {
    save_setting(&format!("appetite-{id}"), serde_json::json!(value));
}
pub fn load_appetite(id: &str) -> Option<f32> {
    load_settings()
        .get(format!("appetite-{id}"))?
        .as_f64()
        .map(|v| v as f32)
}

pub fn save_heat(kind: &str, value: Option<u8>) {
    save_setting(&format!("heat-{kind}"), serde_json::json!(value));
}
pub fn load_heat(kind: &str) -> Option<u8> {
    load_settings()
        .get(format!("heat-{kind}"))?
        .as_u64()
        .filter(|v| *v < 2)
        .map(|v| v as u8)
}

/// Display preferences do not alter the animal's simulation or shelter.
pub fn load_animal_scale() -> f32 {
    load_settings().get("animal_scale").and_then(|v| v.as_f64())
        .filter(|v| v.is_finite()).unwrap_or(1.0).clamp(0.5, 4.0) as f32
}
pub fn save_animal_scale(scale: f32) {
    save_setting("animal_scale", serde_json::json!(scale));
}
pub fn load_see_through_hides() -> bool {
    load_settings().get("see_through_hides").and_then(|v| v.as_bool()).unwrap_or(false)
}
pub fn save_see_through_hides(on: bool) {
    save_setting("see_through_hides", serde_json::json!(on));
}
