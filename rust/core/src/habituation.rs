//! Habituation — the creature gets used to you.
//!
//! A slow-adapting presynaptic depression on the sensory pathways: repeated
//! harmless stimuli weaken their own drive, and the drive recovers over hours.
//! The result is a pet that flinches at your cursor for the first week and
//! gradually stops, *because you have never actually hurt it*, and that
//! startles properly again after you leave it alone for a weekend.
//!
//! **This is a modelling choice, not measured data** — the connectome gives
//! wiring, not learning rules — and it belongs in the README's
//! "What's modeled vs. measured" section alongside the LIF dynamics and the
//! neurotransmitter signs. But the phenomenon is real and among the best
//! characterised in invertebrate neuroscience: looming-response habituation in
//! *Drosophila*, and tap-withdrawal habituation, dishabituation and
//! sensitization in *C. elegans*, where it is the canonical behavioural assay.
//!
//! Three properties are deliberate and load-bearing:
//!
//! 1. **It floors.** Habituation is never complete, in the animal or here. A
//!    fully habituated fly would stop escaping forever, which is both wrong and
//!    a worse pet.
//! 2. **It recovers slowly**, on the order of tens of minutes, so the effect is
//!    something you notice over days rather than within a session.
//! 3. **Dishabituation exists.** A strong stimulus in a *different* modality
//!    restores the response — a real phenomenon, and the reason a startled
//!    creature becomes jumpy again rather than staying numb.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HabituationParams {
    /// Time constant of depression under continuous full-strength stimulus (s).
    pub depress_tau: f32,
    /// Time constant of recovery toward full sensitivity (s).
    pub recover_tau: f32,
    /// Floor: the response never falls below this fraction.
    pub floor: f32,
    /// Drive below this level does not cause depression — background noise
    /// should not habituate anything.
    pub threshold: f32,
    /// Fraction of lost sensitivity restored by each *novel* stimulus in the
    /// other modality. Applied on the rising edge only — see `step`.
    pub dishabituation: f32,
    /// Drive above which a stimulus counts as novel enough to dishabituate.
    pub novelty_threshold: f32,
}

impl Default for HabituationParams {
    fn default() -> Self {
        HabituationParams {
            // ~12 s of sustained looming to approach the floor. A single cursor
            // lunge is a fraction of a second, so this is tens of lunges.
            depress_tau: 12.0,
            // 30 minutes to forgive. Long enough that a lunch break restores
            // some jumpiness and a weekend restores all of it.
            recover_tau: 1800.0,
            floor: 0.35,
            threshold: 0.15,
            dishabituation: 0.25,
            novelty_threshold: 0.6,
        }
    }
}

/// The persistent part: what the creature has learned about you.
/// Small enough to write to disk between runs, which is what makes the effect
/// accumulate over days rather than resetting every launch.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Habituation {
    /// Sensitivity of the looming pathway, `floor..=1`.
    ///
    /// **f64 on purpose.** The recovery constant is 30 minutes; against an f32
    /// this quantity's per-step increment falls below one ULP near full
    /// sensitivity, and recovery silently stalls part-way back — it stuck at
    /// 0.94 and never returned to 1.0. Storing f32 anywhere in this loop
    /// reintroduces that, whatever the arithmetic is done in.
    pub loom_gain: f64,
    /// Sensitivity of the mechanosensory / wind pathway, `floor..=1`.
    pub tap_gain: f64,
    #[serde(default)]
    pub params: HabituationParams,
    /// Cumulative seconds of above-threshold stimulation, for diagnostics.
    #[serde(default)]
    pub loom_exposure_s: f64,
    #[serde(default)]
    pub tap_exposure_s: f64,
    /// Previous-frame drives, for rising-edge detection. Not persisted.
    #[serde(skip)]
    prev_loom: f32,
    #[serde(skip)]
    prev_tap: f32,
}

impl Default for Habituation {
    fn default() -> Self {
        Habituation {
            loom_gain: 1.0,
            tap_gain: 1.0,
            params: HabituationParams::default(),
            loom_exposure_s: 0.0,
            tap_exposure_s: 0.0,
            prev_loom: 0.0,
            prev_tap: 0.0,
        }
    }
}

impl Habituation {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fully naive again — what "a fresh creature" means.
    pub fn reset(&mut self) {
        self.loom_gain = 1.0;
        self.tap_gain = 1.0;
        self.loom_exposure_s = 0.0;
        self.tap_exposure_s = 0.0;
    }

    /// Advance by `dt` seconds given the current sensory drive on each pathway.
    ///
    /// Both pathways depress toward the floor while driven and recover toward 1
    /// while quiet; a strong stimulus on one pathway partially restores the
    /// other (dishabituation).
    pub fn step(&mut self, dt: f32, loom_drive: f32, tap_drive: f32) {
        let p = self.params;

        // Dishabituation is triggered by the *onset* of a novel stimulus, not by
        // its continued presence. Driving both pathways at once used to hold
        // both gains pinned at 1.0 forever, so nothing ever habituated — a real
        // modelling bug, and also the wrong biology: it is novelty that
        // dishabituates, not steady noise.
        let loom_onset = loom_drive > p.novelty_threshold && self.prev_loom <= p.novelty_threshold;
        let tap_onset = tap_drive > p.novelty_threshold && self.prev_tap <= p.novelty_threshold;

        // Computed in f64. With 1 ms steps and a 30-minute recovery constant the
        // per-step increment near gain=1 is smaller than an f32 ULP, so an f32
        // update silently stops recovering and the creature stalls permanently
        // part-way back — it stuck at 0.94 and never reached 1.0. This is also
        // why `step` should be called per frame rather than per simulated
        // millisecond; see `LifSim::step`.
        let floor = p.floor as f64;
        let d = dt as f64;
        let mut update = |gain: &mut f64, exposure: &mut f64, drive: f32| {
            let new = if drive > p.threshold {
                // Depress in proportion to how strong the stimulus is.
                let strength =
                    (((drive - p.threshold) / (1.0 - p.threshold)) as f64).clamp(0.0, 1.0);
                *exposure += d * strength;
                let k = (-d * strength / p.depress_tau as f64).exp();
                floor + (*gain - floor) * k
            } else {
                // Forgive, slowly.
                let k = (-d / p.recover_tau as f64).exp();
                1.0 - (1.0 - *gain) * k
            };
            *gain = new.clamp(floor, 1.0);
        };

        update(&mut self.loom_gain, &mut self.loom_exposure_s, loom_drive);
        update(&mut self.tap_gain, &mut self.tap_exposure_s, tap_drive);

        // Each novel stimulus in the other modality restores a fraction of the
        // sensitivity lost so far.
        if tap_onset {
            self.loom_gain += p.dishabituation as f64 * (1.0 - self.loom_gain);
        }
        if loom_onset {
            self.tap_gain += p.dishabituation as f64 * (1.0 - self.tap_gain);
        }
        self.loom_gain = self.loom_gain.clamp(floor, 1.0);
        self.tap_gain = self.tap_gain.clamp(floor, 1.0);

        self.prev_loom = loom_drive;
        self.prev_tap = tap_drive;
    }

    /// A one-line summary for the tray menu, so the state is legible to the user
    /// rather than a hidden number.
    /// The looming pathway's gain as the sim wants it.
    pub fn loom_gain_f32(&self) -> f32 {
        self.loom_gain as f32
    }
    /// The mechanosensory pathway's gain as the sim wants it.
    pub fn tap_gain_f32(&self) -> f32 {
        self.tap_gain as f32
    }

    pub fn describe(&self) -> String {
        let pct = (self.loom_gain * 100.0).round() as i32;
        let mood = if self.loom_gain > 0.9 {
            "jumpy"
        } else if self.loom_gain > 0.65 {
            "wary"
        } else if self.loom_gain > 0.45 {
            "used to you"
        } else {
            "unbothered"
        };
        format!("{mood} ({pct}% startle)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive one pathway for `secs` at full strength, in 1 ms steps.
    fn expose(h: &mut Habituation, secs: f32, loom: f32, tap: f32) {
        let dt = 0.001;
        for _ in 0..((secs / dt) as u32) {
            h.step(dt, loom, tap);
        }
    }

    #[test]
    fn repeated_looming_reduces_sensitivity() {
        let mut h = Habituation::new();
        assert_eq!(h.loom_gain, 1.0);
        // tau is 12 s of *sustained* full-strength looming, and a cursor lunge
        // is a fraction of a second, so this is tens of lunges — the effect is
        // meant to be felt over days, not within a session.
        expose(&mut h, 5.0, 1.0, 0.0);
        assert!(h.loom_gain < 0.80, "gain after 5s: {}", h.loom_gain);
        assert!(h.loom_gain > 0.70, "should not collapse instantly: {}", h.loom_gain);
        assert!(h.loom_exposure_s > 4.0);
    }

    /// Never fully habituates: a numb creature that can no longer escape is
    /// both biologically wrong and a worse pet.
    #[test]
    fn habituation_floors_and_never_reaches_zero() {
        let mut h = Habituation::new();
        expose(&mut h, 600.0, 1.0, 0.0);
        assert!(
            (h.loom_gain - h.params.floor as f64).abs() < 0.02,
            "gain {} should approach the floor {}",
            h.loom_gain,
            h.params.floor
        );
        assert!(h.loom_gain >= h.params.floor as f64);
    }

    #[test]
    fn sensitivity_recovers_when_left_alone() {
        let mut h = Habituation::new();
        expose(&mut h, 30.0, 1.0, 0.0);
        let habituated = h.loom_gain;
        assert!(habituated < 0.5, "habituated to {habituated}");

        // A lunch break.
        expose(&mut h, 1800.0, 0.0, 0.0);
        assert!(
            h.loom_gain > habituated + 0.2,
            "{habituated} -> {} after 30 min",
            h.loom_gain
        );
        // A weekend.
        expose(&mut h, 20_000.0, 0.0, 0.0);
        assert!(h.loom_gain > 0.95, "should be naive again: {}", h.loom_gain);
    }

    /// Sub-threshold background must not habituate anything, or the creature
    /// would go numb from noise alone.
    #[test]
    fn background_noise_does_not_habituate() {
        let mut h = Habituation::new();
        let below = h.params.threshold * 0.5;
        expose(&mut h, 60.0, below, below);
        assert!(h.loom_gain > 0.99, "gain {}", h.loom_gain);
        assert_eq!(h.loom_exposure_s, 0.0);
    }

    /// Dishabituation: a strong stimulus in another modality restores the
    /// response. This is why a startled creature becomes jumpy again.
    #[test]
    fn a_novel_stimulus_in_another_modality_restores_sensitivity() {
        let mut h = Habituation::new();
        expose(&mut h, 30.0, 1.0, 0.0);
        let habituated = h.loom_gain;

        // A sharp tap, with no looming: one rising edge.
        expose(&mut h, 1.0, 0.0, 1.0);
        assert!(
            h.loom_gain > habituated,
            "tap should dishabituate looming: {habituated} -> {}",
            h.loom_gain
        );
    }

    #[test]
    fn the_two_pathways_habituate_independently() {
        let mut h = Habituation::new();
        expose(&mut h, 20.0, 1.0, 0.0);
        assert!(h.loom_gain < 0.7, "loom_gain {}", h.loom_gain);
        // The tap pathway saw dishabituating looming but no taps of its own,
        // so it must remain fully sensitive.
        assert!(h.tap_gain > 0.95, "tap_gain {}", h.tap_gain);
    }

    #[test]
    fn reset_makes_the_creature_naive() {
        let mut h = Habituation::new();
        expose(&mut h, 30.0, 1.0, 1.0);
        assert!(h.loom_gain < 0.6, "loom_gain {}", h.loom_gain);
        h.reset();
        assert_eq!(h.loom_gain, 1.0);
        assert_eq!(h.tap_gain, 1.0);
    }

    #[test]
    fn state_round_trips_through_json_so_it_survives_a_restart() {
        let mut h = Habituation::new();
        expose(&mut h, 7.0, 1.0, 0.0);
        let json = serde_json::to_string(&h).expect("serialize");
        let back: Habituation = serde_json::from_str(&json).expect("deserialize");
        assert!((back.loom_gain - h.loom_gain).abs() < 1e-6);
        assert!((back.loom_exposure_s - h.loom_exposure_s).abs() < 1e-6);
    }

    #[test]
    fn describe_tracks_the_gain() {
        let mut h = Habituation::new();
        assert!(h.describe().contains("jumpy"));
        expose(&mut h, 30.0, 1.0, 0.0);
        assert!(
            h.describe().contains("unbothered") || h.describe().contains("used to you"),
            "{}",
            h.describe()
        );
    }
}
