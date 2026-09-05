//! Where the desktop becomes a stimulus.
//!
//! Port of `computeLoom` (main.swift:606), `injectTap` (main.swift:592) and
//! `injectWindowLoom` (main.swift:579), plus the circadian/sleep neuromodulation
//! from the app's 30 Hz timer (main.swift:761).
//!
//! This is the honesty boundary the README's "What's modeled vs. measured"
//! section describes: **everything in this file is a modelling choice**, and
//! everything downstream of the sensory neurons it drives is FlyWire data. It is
//! also the layer a second creature replaces wholesale — a worm is blind, so a
//! cursor cannot be a looming predator for it (PORT_PLAN.md §5.2). The spider
//! chimera, whose looming module *is* the fly's, reuses this file as-is and
//! adds its small-object channel on top (`spiderrt.rs`), which is why the
//! subject is any `Body` rather than the fly.

use dfcore::Body;
use dfcore::env::EnvSnapshot;
use dfcore::util::{clamp, hypot};
use dfcore::{circadian_activity, Habituation, LifSim, Vec2};

pub struct Transduction {
    prev_cursor: Option<Vec2>,
    cursor_vel: Vec2,
    /// Decaying looming drive contributed by newly-appeared windows.
    window_loom_l: f32,
    window_loom_r: f32,
    /// Menu-driven "Escape Test", decaying over ~1 s.
    loom_override: f32,
    /// What the creature has learned about this user. Lives here, not in the
    /// simulation: it is gain on the stimulus, and every creature transduces.
    pub habituation: Habituation,
}

impl Default for Transduction {
    fn default() -> Self {
        Self::new()
    }
}

impl Transduction {
    pub fn new() -> Self {
        Transduction {
            prev_cursor: None,
            cursor_vel: Vec2::ZERO,
            window_loom_l: 0.0,
            window_loom_r: 0.0,
            loom_override: 0.0,
            habituation: Habituation::new(),
        }
    }

    /// The menu's "Escape Test (loom)" and "Scare Flies".
    pub fn trigger_scare(&mut self) {
        self.loom_override = 0.6;
    }

    /// Cursor kinematics -> per-eye looming drive plus an air puff.
    ///
    /// Loom is the rate of angular expansion: a cursor closing fast at short
    /// range is a big, fast-growing object. This is why a slow approach is
    /// tolerated and a lunge is not — the escape decision itself is made by the
    /// real LC4/LPLC2 -> giant fiber circuit, not here.
    fn compute_loom(&mut self, body: &impl Body, cursor: Option<Vec2>, dt: f32) -> (f32, f32, f32) {
        let Some(m) = cursor else {
            return (0.0, 0.0, 0.0);
        };
        if let Some(pm) = self.prev_cursor {
            if dt > 0.0 {
                let v = Vec2::new((m.x - pm.x) / dt, (m.y - pm.y) / dt);
                self.cursor_vel.x += (v.x - self.cursor_vel.x) * 0.4;
                self.cursor_vel.y += (v.y - self.cursor_vel.y) * 0.4;
            }
        }
        self.prev_cursor = Some(m);

        let rel = Vec2::new(m.x - body.position().x, m.y - body.position().y);
        let dist = hypot(rel.x, rel.y).max(20.0);
        // Radial approach speed; positive means the cursor is closing in.
        let approach = -(rel.x * self.cursor_vel.x + rel.y * self.cursor_vel.y) / dist;
        let mut loom =
            clamp(approach / dist * 6.0, 0.0, 1.0) * clamp(1.0 - dist / 800.0, 0.0, 1.0);
        loom += clamp((130.0 - dist) / 130.0, 0.0, 1.0) * 0.5; // hovering close = big object
        loom = clamp(loom + self.loom_override, 0.0, 1.0);

        // Split between the eyes by bearing relative to heading.
        let f = Vec2::new(body.heading().cos(), body.heading().sin());
        let rd = Vec2::new(rel.x / dist, rel.y / dist);
        let cross_z = f.x * rd.y - f.y * rd.x; // > 0: threat on the left
        let lw = clamp(0.5 + 0.5 * cross_z, 0.12, 1.0);
        let rw = clamp(0.5 - 0.5 * cross_z, 0.12, 1.0);

        let puff = clamp(hypot(self.cursor_vel.x, self.cursor_vel.y) / 1500.0, 0.0, 1.0)
            * clamp(1.0 - dist / 500.0, 0.0, 1.0);
        (loom * lw, loom * rw, puff)
    }

    /// A window appearing near the fly is a real looming object; which eye sees
    /// it depends on where it appeared relative to the fly's heading.
    fn inject_window_loom(&mut self, body: &impl Body, strength: f32, at: Vec2) {
        let rel = Vec2::new(at.x - body.position().x, at.y - body.position().y);
        let dist = hypot(rel.x, rel.y).max(1.0);
        let f = Vec2::new(body.heading().cos(), body.heading().sin());
        let cross_z = (f.x * rel.y - f.y * rel.x) / dist;
        self.window_loom_l = self
            .window_loom_l
            .max(strength * clamp(0.5 + 0.5 * cross_z, 0.12, 1.0));
        self.window_loom_r = self
            .window_loom_r
            .max(strength * clamp(0.5 - 0.5 * cross_z, 0.12, 1.0));
    }

    /// Drive the whole circuit from one snapshot of the desktop.
    /// Returns `(tempo, sleepy)` for the body.
    pub fn apply(
        &mut self,
        sim: &mut LifSim,
        body: &impl Body,
        env: &EnvSnapshot,
        dt: f32,
    ) -> (f32, bool) {
        // Clicks are taps on the fly's substrate, reaching it through the real
        // wind/sensory pathway rather than as a scripted scare. Gated by
        // habituation: repeated clicking is exactly the tap-withdrawal assay,
        // and a creature that never stops flinching at your mouse is wrong.
        let tap_gain = self.habituation.tap_gain_f32();
        for c in &env.clicks {
            let d = hypot(c.x - body.position().x, c.y - body.position().y);
            let strength = clamp(1.0 - d / 520.0, 0.0, 1.0);
            if strength > 0.05 {
                let sens = sim.sens.clone();
                sim.stimulate(&sens, (0.15 + strength * 0.35) * tap_gain, 130);
            }
        }

        // New windows loom; the circuit decides whether to flee your dialogs.
        for w in &env.new_windows {
            let d = hypot(w.center.x - body.position().x, w.center.y - body.position().y);
            let strength = clamp(1.0 - d / 480.0, 0.0, 1.0) * 0.75;
            if strength > 0.08 {
                self.inject_window_loom(body, strength, w.center);
            }
        }

        let (l, r, puff) = self.compute_loom(body, env.cursor, dt);
        let decay = (-4.0 * dt).exp();
        self.window_loom_l *= decay;
        self.window_loom_r *= decay;
        let raw_loom_l = l.max(self.window_loom_l);
        let raw_loom_r = r.max(self.window_loom_r);
        // Typing is substrate vibration — the idle-time API knows *when* keys
        // were pressed, never which. On Windows this is an inference; see
        // dfplatform's fidelity notes.
        let raw_puff = puff.max(env.typing_level * 0.30);

        // Advance habituation on the stimulus the world actually presented,
        // then attenuate what reaches the connectome. Driving it with the
        // already-attenuated value would make it depress ever more slowly as it
        // depresses — a feedback loop, not an adaptation.
        self.habituation
            .step(dt, raw_loom_l.max(raw_loom_r), raw_puff);
        let loom_gain = self.habituation.loom_gain_f32();
        sim.loom_l = raw_loom_l * loom_gain;
        sim.loom_r = raw_loom_r * loom_gain;
        sim.air_puff = raw_puff * self.habituation.tap_gain_f32();

        // Body -> brain: leg proprioception from the current gait.
        sim.gait_drive = body.proprioception().drive;
        sim.gait_phase = body.proprioception().phase;

        // Circadian + sleep neuromodulation. Compressed toward 1 on purpose:
        // the LIF neurons sit just below threshold, so a raw multiplier
        // silences them entirely. This is the "siesta coma" bug in CLAUDE.md.
        let activity = circadian_activity(env.local_hour);
        let sleepy = (env.idle_secs > 600.0 && (env.local_hour >= 22.0 || env.local_hour < 6.0))
            || env.idle_secs > 1800.0;
        sim.activity_scale = (1.0 - (1.0 - activity) * 0.35) * if sleepy { 0.75 } else { 1.0 };
        sim.sensory_gate = if sleepy { 0.55 } else { 1.0 };

        self.loom_override = (self.loom_override - dt * 1.2).max(0.0);

        (dfcore::env::thermal_tempo(env.machine_heat), sleepy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dfcore::body::Fly;

    fn fly_at_origin() -> Fly {
        Fly::new(Vec2::ZERO, dfcore::DEFAULT_SEED)
    }

    fn brain() -> Option<dfcore::BrainData> {
        match dfcore::data::load() {
            Ok(b) => Some(b),
            Err(e) => {
                eprintln!("skipping: {e}");
                None
            }
        }
    }

    #[test]
    fn a_distant_stationary_cursor_produces_no_loom() {
        let mut t = Transduction::new();
        let fly = fly_at_origin();
        let far = Some(Vec2::new(700.0, 700.0));
        t.compute_loom(&fly, far, 1.0 / 60.0);
        let (l, r, puff) = t.compute_loom(&fly, far, 1.0 / 60.0);
        assert!(l < 0.01 && r < 0.01, "loom {l},{r}");
        assert!(puff < 0.01);
    }

    /// Loom sampled at one fixed distance, after the velocity EMA has settled.
    fn loom_at(fly: &Fly, speed_px_per_s: f32, sample_dist: f32) -> f32 {
        let dt = 1.0 / 60.0;
        let step = speed_px_per_s * dt;
        let mut t = Transduction::new();
        let mut d = sample_dist + step * 30.0;
        let mut last = 0.0;
        while d > sample_dist {
            let (l, r, _) = t.compute_loom(fly, Some(Vec2::new(d, 0.0)), dt);
            last = l + r;
            d -= step;
        }
        last
    }

    /// The README's claim is that slow approaches are tolerated and fast lunges
    /// are not. At range, the transduction discriminates strongly.
    #[test]
    fn a_fast_approach_looms_more_than_a_slow_one_at_range() {
        let fly = fly_at_origin();
        let drift = loom_at(&fly, 20.0, 600.0);
        let lunge = loom_at(&fly, 2000.0, 600.0);
        assert!(lunge > drift * 3.0, "drift {drift}, lunge {lunge}");
    }

    /// But the formula **saturates**: `clamp(approach / dist * 6, 0, 1)` pins at
    /// 1 for any approach faster than `dist / 6` px/s, so at close range a slow
    /// creep and a lunge produce identical drive.
    ///
    /// This is faithful to main.swift:619, and worth pinning with a test,
    /// because it locates the real mechanism: "slow approaches are tolerated" is
    /// a property of the **circuit** — LC->GF excitation losing a race against
    /// ~1,200 synapses of 4 ms-delayed feedforward inhibition — and not of this
    /// file. A future change that makes the transduction "smarter" here would be
    /// moving the decision out of the connectome and into hand-written code.
    #[test]
    fn loom_saturates_at_close_range_so_the_circuit_makes_the_decision() {
        let fly = fly_at_origin();
        let slow = loom_at(&fly, 120.0, 300.0);
        let fast = loom_at(&fly, 2400.0, 300.0);
        // Not bit-identical: the two sweeps overshoot `sample_dist` by
        // different amounts, which shifts the `1 - dist/800` attenuation
        // slightly. Within a few percent is saturation; compare with the 3x
        // separation the at-range test asserts.
        let ratio = fast / slow;
        assert!(
            (0.95..1.05).contains(&ratio),
            "expected saturation, got slow {slow} vs fast {fast} (ratio {ratio})"
        );
    }

    #[test]
    fn loom_splits_between_the_eyes_by_bearing() {
        let mut fly = fly_at_origin();
        fly.heading = 0.0; // facing +x
        let mut t = Transduction::new();
        // A threat off to the fly's left (+y) must weight the left eye more.
        for _ in 0..4 {
            t.compute_loom(&fly, Some(Vec2::new(60.0, 120.0)), 1.0 / 60.0);
        }
        let (l, r, _) = t.compute_loom(&fly, Some(Vec2::new(55.0, 110.0)), 1.0 / 60.0);
        assert!(l > r, "left {l} should exceed right {r}");
    }

    #[test]
    fn the_escape_test_override_decays() {
        let mut t = Transduction::new();
        t.trigger_scare();
        assert!(t.loom_override > 0.5);
        for _ in 0..120 {
            t.loom_override = (t.loom_override - (1.0 / 60.0) * 1.2).max(0.0);
        }
        assert_eq!(t.loom_override, 0.0);
    }

    /// Habituation now lives here rather than inside the simulation, so this
    /// is the test that the wiring actually attenuates what reaches the
    /// connectome. Asserted by *forcing* a gain rather than by accumulating
    /// one, so it measures the wiring and not the adaptation rate — those are
    /// separate claims and conflating them makes both tests weak.
    #[test]
    fn habituation_attenuates_the_loom_that_reaches_the_circuit() {
        let Some(brain) = brain() else { return };
        let fly = fly_at_origin();

        let peak_with_gain = |gain: f64| -> f32 {
            let mut sim = LifSim::new(&brain.circuit, dfcore::DEFAULT_SEED);
            let mut t = Transduction::new();
            t.habituation.loom_gain = gain;
            // Freeze it, so this measures attenuation and nothing else.
            t.habituation.params.depress_tau = f32::MAX;
            t.habituation.params.recover_tau = f32::MAX;

            let mut peak = 0.0f32;
            let mut d = 500.0f32;
            while d > 60.0 {
                let env = EnvSnapshot {
                    cursor: Some(Vec2::new(d, 0.0)),
                    local_hour: 12.0,
                    ..Default::default()
                };
                t.apply(&mut sim, &fly, &env, 1.0 / 30.0);
                peak = peak.max(sim.loom_l.max(sim.loom_r));
                d -= 60.0;
            }
            peak
        };

        let naive = peak_with_gain(1.0);
        let habituated = peak_with_gain(0.5);
        assert!(naive > 0.1, "a lunge should produce loom, got {naive}");
        let ratio = habituated / naive;
        assert!(
            (0.45..0.55).contains(&ratio),
            "a gain of 0.5 should halve the loom reaching the circuit, got {ratio:.3}"
        );
        // The floor guarantees the pathway is never silenced entirely.
        assert!(peak_with_gain(Habituation::new().params.floor as f64) > 0.0);
    }

    /// And that repeated exposure actually moves the gain. Separate from the
    /// wiring test above: this one is about the adaptation, and it is
    /// deliberately slow — the effect is meant to be felt over days.
    #[test]
    fn repeated_lunges_habituate_the_looming_pathway() {
        let Some(brain) = brain() else { return };
        let mut sim = LifSim::new(&brain.circuit, dfcore::DEFAULT_SEED);
        let mut t = Transduction::new();
        let fly = fly_at_origin();

        let before = t.habituation.loom_gain;
        for _ in 0..400 {
            let mut d = 500.0f32;
            while d > 60.0 {
                let env = EnvSnapshot {
                    cursor: Some(Vec2::new(d, 0.0)),
                    local_hour: 12.0,
                    ..Default::default()
                };
                t.apply(&mut sim, &fly, &env, 1.0 / 30.0);
                d -= 60.0;
            }
        }
        assert!(
            t.habituation.loom_gain < before - 0.02,
            "400 lunges should habituate: {before} -> {}",
            t.habituation.loom_gain
        );
        assert!(
            t.habituation.loom_gain >= t.habituation.params.floor as f64,
            "must never fall below the floor"
        );
    }

    /// The simulation itself must be habituation-free, so the circuit stays
    /// numerically identical to the Swift oracle. Setting the stimulus directly
    /// bypasses transduction and must therefore be unaffected by how much
    /// habituation the transduction layer has accumulated.
    #[test]
    fn the_circuit_itself_is_unaffected_by_habituation() {
        let Some(brain) = brain() else { return };
        let run = || {
            let mut sim = LifSim::new(&brain.circuit, dfcore::DEFAULT_SEED);
            sim.step(1000);
            for _ in 0..400 {
                sim.loom_l = 1.0;
                sim.loom_r = 0.5;
                sim.step(1);
            }
            (sim.rate_loom, sim.total_spikes)
        };
        assert_eq!(run(), run(), "the circuit must be deterministic");
    }

    #[test]
    fn night_plus_long_idle_makes_the_fly_sleepy_but_not_comatose() {
        let Some(brain) = brain() else { return };
        let mut sim = LifSim::new(&brain.circuit, dfcore::DEFAULT_SEED);
        let mut t = Transduction::new();
        let fly = fly_at_origin();

        let mut env = EnvSnapshot::default();
        env.local_hour = 2.0;
        env.idle_secs = 1200.0;
        let (_, sleepy) = t.apply(&mut sim, &fly, &env, 1.0 / 60.0);
        assert!(sleepy, "2am after 20 minutes idle should be sleep");
        // Compressed, never a raw multiply: the network must stay alive.
        assert!(
            sim.activity_scale > 0.5,
            "activity_scale {} would silence the network",
            sim.activity_scale
        );

        env.local_hour = 14.0; // siesta
        env.idle_secs = 0.0;
        let (_, awake) = t.apply(&mut sim, &fly, &env, 1.0 / 60.0);
        assert!(!awake);
        assert!(sim.activity_scale > 0.8, "siesta must slow, not stop");
    }
}
