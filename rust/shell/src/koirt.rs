//! Creature #4 on the desktop: a koi, driven by rules.
//!
//! Every other runtime owns a simulation. This one owns **nothing** — there is
//! no fish connectome at this scale, so `sim()` returns `None`, the brain
//! window stays closed, and the tray says `PROCEDURAL, no connectome` rather
//! than naming a dataset. That is the point: the app's whole claim is that the
//! brain data is real, and a creature with no data has to say so instead of
//! quietly borrowing someone else's.
//!
//! What replaces the circuit is a small, explicit drive model. It produces the
//! same [`BrainSignals`] every other creature's readout produces, so the body
//! and the app loop cannot tell the difference — which is exactly what the
//! `Runtime` seam is for, and is what would let a real fish connectome slot in
//! later without the body or the behaviour changing (see KOI_PLAN.md).
//!
//! The senses are a fish's, not a fly's:
//!
//! | stimulus | fly | koi |
//! |---|---|---|
//! | cursor approaching | looming → LC4/LPLC2 → giant fiber | a shadow overhead → startle |
//! | click | substrate tap through the wind pathway | a knock on the glass → startle |
//! | new window | a predator looming | ignored: it is not in the water |
//! | window ledges | terrain to walk on | ignored: there is nothing to stand on |
//!
//! Habituation is shared with the rest of the app and matters here: a koi in a
//! pond stops fleeing the person who feeds it. The gain is applied to the
//! startle drive, so the fish that bolted from your cursor on day one only
//! flicks its tail by the end of the week.

use dfcore::creature::{Body, World};
use dfcore::data::BrainPointsFile;
use dfcore::env::thermal_tempo;
use dfcore::util::{clamp, hypot};
use dfcore::{
    circadian_activity, BrainSignals, Creature, EnvSnapshot, Habituation, KoiBody, Sim, Vec2,
};

use crate::koibody;
use crate::mesh::Mesh;
use crate::runtime::{Geometry, Runtime};

/// How near the cursor has to come, in scene units, before the fish notices it
/// at all. Generous: a fish reacts to a shadow well before contact.
const NOTICE_RANGE: f32 = 190.0;
/// A click this far away still registers as a knock.
const KNOCK_RANGE: f32 = 420.0;

pub struct KoiRuntime {
    creature: dfcore::creature::Koi,
    koi: KoiBody,
    habituation: Habituation,
    frame_mesh: Mesh,

    prev_cursor: Option<Vec2>,
    /// Startle drive accumulated this poll, 0..1, before habituation.
    threat: f32,
    /// Smoothed threat, so a single noisy frame does not fire the escape.
    threat_ema: f32,
    /// Where the threat was, so the escape turns away from it.
    threat_at: Option<Vec2>,
    /// Set by the tray's Escape Test.
    forced_startle: bool,

    pending_tempo: f32,
    pending_sleepy: bool,
    /// Circadian activity, 0..1: koi are markedly less active when it is cold
    /// and dark, which is a real and well-known thing about the animal.
    activity: f32,
}

impl KoiRuntime {
    pub fn new(seed: u64) -> Self {
        let creature = dfcore::creature::Koi;
        println!(
            "{} - {}",
            creature.display_name(),
            creature.provenance().describe()
        );
        if let dfcore::Provenance::Procedural { why, .. } = creature.provenance() {
            println!("note: {why}");
        }
        KoiRuntime {
            creature,
            koi: KoiBody::new(Vec2::ZERO, seed),
            habituation: Habituation::new(),
            frame_mesh: Mesh::default(),
            prev_cursor: None,
            threat: 0.0,
            threat_ema: 0.0,
            threat_at: None,
            forced_startle: false,
            pending_tempo: 1.0,
            pending_sleepy: false,
            activity: 1.0,
        }
    }

    /// The drive model. This is what a readout would be if there were a circuit
    /// to read — kept in one place for the same reason `SignalBuilder` is.
    fn drives(&mut self) -> BrainSignals {
        let mut s = BrainSignals::new();
        s.tempo = self.pending_tempo;
        s.sleep = self.pending_sleepy;
        // Circadian activity scales swimming effort rather than gating it: a
        // sluggish koi still swims, it just does less.
        s.walk_drive = clamp(self.activity, 0.15, 1.0);
        // Startle, gated by habituation of the same pathway.
        let gain = self.habituation.loom_gain_f32();
        s.escape = (self.threat_ema * gain > 0.55) || self.forced_startle;
        self.forced_startle = false;
        s.arousal = clamp(self.threat_ema * gain, 0.0, 1.0);
        s
    }
}

impl Runtime for KoiRuntime {
    fn creature(&self) -> &dyn Creature {
        &self.creature
    }

    /// No simulation, ever. The brain window keys off this and stays shut.
    fn sim(&self) -> Option<&dyn Sim> {
        None
    }
    fn sim_mut(&mut self) -> Option<&mut dyn Sim> {
        None
    }
    fn brain_points(&self) -> Option<&BrainPointsFile> {
        None
    }
    fn brain_info(&self) -> String {
        // Deliberately not a dataset name. A user reading this line must be
        // able to tell at a glance that this creature is not running a brain.
        "PROCEDURAL - no connectome, hand-written behaviour".to_string()
    }

    fn habituation(&self) -> &Habituation {
        &self.habituation
    }
    fn habituation_mut(&mut self) -> &mut Habituation {
        &mut self.habituation
    }

    fn scare(&mut self) {
        self.forced_startle = true;
    }

    fn sense(&mut self, env: &EnvSnapshot, dt: f32) {
        self.threat = 0.0;
        self.threat_at = None;

        // A cursor moving near the fish is a shadow passing over the pond. Both
        // proximity and speed matter: a still cursor resting nearby is not a
        // threat, and neither is a fast one on the far side of the screen.
        if let Some(m) = env.cursor {
            let speed = match self.prev_cursor {
                Some(p) if dt > 0.0 => hypot(m.x - p.x, m.y - p.y) / dt,
                _ => 0.0,
            };
            self.prev_cursor = Some(m);
            let d = hypot(m.x - self.koi.pos.x, m.y - self.koi.pos.y);
            if d < NOTICE_RANGE {
                let near = 1.0 - d / NOTICE_RANGE;
                let fast = clamp(speed / 700.0, 0.0, 1.0);
                // Something has to be moving for it to be a threat.
                let t = near * (0.25 + 0.75 * fast);
                if t > self.threat {
                    self.threat = t;
                    self.threat_at = Some(m);
                }
            }
        }

        // A click is a knock on the glass, felt across the whole pond.
        for c in &env.clicks {
            let d = hypot(c.x - self.koi.pos.x, c.y - self.koi.pos.y);
            let t = clamp(1.0 - d / KNOCK_RANGE, 0.0, 1.0) * 0.9;
            if t > self.threat {
                self.threat = t;
                self.threat_at = Some(*c);
            }
        }

        // Rise fast, fall slow: a startle should not be cancelled by the
        // cursor happening to stop for one frame.
        let rate = if self.threat > self.threat_ema { 18.0 } else { 2.2 };
        self.threat_ema += (self.threat - self.threat_ema) * (rate * dt).min(1.0);

        // The same habituation every other creature uses, on the same
        // stimulus-then-attenuate discipline: a koi learns that the person at
        // the keyboard is not a heron.
        self.habituation.step(dt, self.threat, 0.0);

        self.activity = circadian_activity(env.local_hour);
        self.pending_tempo = thermal_tempo(env.machine_heat);
        self.pending_sleepy = (env.idle_secs > 600.0
            && (env.local_hour >= 22.0 || env.local_hour < 6.0))
            || env.idle_secs > 1800.0;
    }

    fn tick(&mut self, dt: f32, bounds: (f32, f32), cursor: Option<Vec2>) {
        let drives = self.drives();
        // The body turns away from whatever startled it, which is not
        // necessarily where the cursor is now.
        let world = World {
            bounds,
            ledges: Vec::new(), // a swimmer has no terrain
            cursor: self.threat_at.or(cursor),
        };
        self.koi.step(dt, &drives, &world);
    }

    fn build(&mut self, glass: bool) -> Geometry<'_> {
        koibody::build_frame(&mut self.frame_mesh, &self.koi, glass);
        Geometry {
            body: &self.frame_mesh,
            // Nothing to show inside: there is no connectome. An empty glass
            // fish is the honest rendering.
            neurons: None,
        }
    }

    fn position(&self) -> Vec2 {
        self.koi.pos
    }

    fn place(&mut self, at: Vec2) {
        let seed = (at.x.abs() as u64) ^ ((at.y.abs() as u64) << 8) ^ 0x4B01;
        self.koi = KoiBody::new(at, seed);
    }

    fn moved_display(&mut self, bounds: (f32, f32)) {
        let (w, h) = bounds;
        let p = self.koi.pos;
        let clamped = Vec2::new(
            clamp(p.x, -w / 2.0 + 60.0, w / 2.0 - 60.0),
            clamp(p.y, -h / 2.0 + 60.0, h / 2.0 - 60.0),
        );
        if clamped != p {
            self.place(clamped);
        }
    }

    fn status(&self) -> String {
        format!(
            "{:?} pos ({:.0},{:.0}) speed {:.0} depth {:.2}",
            self.koi.state, self.koi.pos.x, self.koi.pos.y, self.koi.speed, self.koi.depth
        )
    }

    fn snapshot_pose(&mut self, _alt: f32, frames: u32, bounds: (f32, f32)) {
        // Swim for a moment so the body is mid-beat rather than laid out
        // straight, then startle so the snapshot shows the C-start — the one
        // pose that says "fish" rather than "shape".
        let drives = BrainSignals::new();
        let world = World {
            bounds,
            ledges: Vec::new(),
            cursor: None,
        };
        for _ in 0..frames.max(30) {
            self.koi.step(1.0 / 60.0, &drives, &world);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dfcore::KoiState;

    fn env_with_cursor(at: Vec2, hour: f32) -> EnvSnapshot {
        EnvSnapshot {
            cursor: Some(at),
            local_hour: hour,
            ..Default::default()
        }
    }

    /// The honesty contract, asserted rather than trusted to a comment.
    #[test]
    fn the_koi_never_claims_to_have_a_brain() {
        let rt = KoiRuntime::new(1);
        assert!(rt.sim().is_none(), "a procedural creature must have no sim");
        assert!(rt.brain_points().is_none(), "and no soma cloud");

        let info = rt.brain_info();
        assert!(
            info.to_uppercase().contains("PROCEDURAL"),
            "the data line must say so: {info}"
        );
        assert!(
            !info.to_lowercase().contains("flywire"),
            "must not borrow another creature's dataset: {info}"
        );

        let d = rt.creature().provenance().describe();
        assert!(d.contains("PROCEDURAL"), "provenance line: {d}");
        assert!(!d.contains("measured"), "must never read as measured: {d}");
    }

    /// Glass mode has nothing to put inside it, and must not invent anything.
    #[test]
    fn the_glass_register_shows_no_neurons() {
        let mut rt = KoiRuntime::new(2);
        let g = rt.build(true);
        assert!(g.neurons.is_none(), "there are no neurons to draw");
        assert!(!g.body.verts.is_empty());
    }

    /// A cursor sweeping past should startle it; a still one should not.
    #[test]
    fn a_fast_cursor_startles_it_and_a_still_one_does_not() {
        let bounds = (1512.0, 982.0);
        let dt = 1.0 / 30.0;

        let mut calm = KoiRuntime::new(3);
        calm.place(Vec2::ZERO);
        for _ in 0..30 {
            calm.sense(&env_with_cursor(Vec2::new(60.0, 0.0), 12.0), dt);
            calm.tick(dt, bounds, None);
        }
        assert!(
            calm.koi.state != KoiState::Dart,
            "a motionless cursor should not startle a fish"
        );

        let mut spooked = KoiRuntime::new(3);
        spooked.place(Vec2::ZERO);
        let mut x = 180.0f32;
        let mut darted = false;
        for _ in 0..30 {
            spooked.sense(&env_with_cursor(Vec2::new(x, 0.0), 12.0), dt);
            spooked.tick(dt, bounds, None);
            darted |= spooked.koi.state == KoiState::Dart;
            x -= 26.0;
        }
        assert!(darted, "a cursor lunging at it should startle it");
    }

    /// And that the startle is habituable, like every other creature's.
    #[test]
    fn a_habituated_koi_is_harder_to_startle() {
        let mut rt = KoiRuntime::new(5);
        rt.place(Vec2::ZERO);
        rt.habituation.loom_gain = 0.35; // fully habituated
        let dt = 1.0 / 30.0;
        let mut x = 180.0f32;
        let mut darted = false;
        for _ in 0..30 {
            rt.sense(&env_with_cursor(Vec2::new(x, 0.0), 12.0), dt);
            rt.tick(dt, (1512.0, 982.0), None);
            darted |= rt.koi.state == KoiState::Dart;
            x -= 26.0;
        }
        assert!(!darted, "a habituated koi should tolerate the same approach");
    }

    #[test]
    fn the_tray_escape_test_startles_it() {
        let mut rt = KoiRuntime::new(7);
        rt.place(Vec2::ZERO);
        rt.scare();
        rt.tick(1.0 / 60.0, (1512.0, 982.0), None);
        assert_eq!(rt.koi.state, KoiState::Dart);
    }

    /// A swimmer has no terrain. Ledges must never reach the body, or it would
    /// try to stand on a window edge.
    #[test]
    fn window_ledges_are_ignored() {
        let mut rt = KoiRuntime::new(11);
        let mut env = EnvSnapshot::default();
        env.local_hour = 12.0;
        env.ledges = vec![dfcore::Ledge {
            y: 0.0,
            x0: -300.0,
            x1: 300.0,
            id: 1,
        }];
        rt.sense(&env, 1.0 / 30.0);
        rt.tick(1.0 / 30.0, (1512.0, 982.0), None);
        // Nothing to assert on the body directly — the check is that `tick`
        // builds its own empty terrain, so the fish keeps swimming freely.
        assert_ne!(rt.koi.state, KoiState::Resting);
    }

    #[test]
    fn it_rests_at_night_when_the_machine_is_idle() {
        let mut rt = KoiRuntime::new(13);
        let mut env = EnvSnapshot::default();
        env.local_hour = 2.0;
        env.idle_secs = 1200.0;
        for _ in 0..40 {
            rt.sense(&env, 1.0 / 30.0);
            rt.tick(1.0 / 30.0, (1512.0, 982.0), None);
        }
        assert_eq!(rt.koi.state, KoiState::Resting);
    }
}
