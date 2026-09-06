//! The koi's body: a swimmer.
//!
//! Creature #4, and the first one with **no brain at all**. The fly and the
//! spider run real FlyWire circuits; the worm runs a graded integrator when its
//! data is present. There is no fly-scale goldfish or koi connectome, so this
//! animal is driven by a hand-written behaviour model and is labelled
//! [`Provenance::Procedural`](crate::creature::Provenance::Procedural)
//! everywhere the app can show it. Inventing a connectome to keep the set
//! uniform would be the one thing this project must not do.
//!
//! ## Why swimming is not walking with different geometry
//!
//! A fly walks a tripod gait; a worm undulates uniformly along its whole
//! length. A fish does neither. Koi are **carangiform** swimmers: the head yaws
//! very little, the wave amplitude grows toward the tail, and most of the
//! thrust comes from the last third of the body and the caudal fin. Applying
//! the worm's uniform wave to a fish gives a swimming eel, which is a different
//! animal.
//!
//! Two other things make it read as a fish rather than a drifting sprite:
//!
//! * **Burst-and-coast.** Fish do not swim at constant effort. They beat for a
//!   moment, then glide with the body nearly straight while speed decays. The
//!   glide is most of the duty cycle at cruise, and it is what makes the motion
//!   look unhurried rather than mechanical.
//! * **The C-start.** A startled fish bends into a C, then straightens
//!   explosively and accelerates away — a single, very fast, very asymmetric
//!   bend. That is the escape, and it is nothing like the fly's takeoff.
//!
//! The centreline follows the head's recorded path, exactly as the worm's does
//! and for the same reason: a body of fixed length cannot be turned by moving
//! one end without retracing its own track. What differs is that the visible
//! spine is that centreline **plus** a lateral travelling wave, rather than
//! being the track itself.

use crate::creature::{Body, Proprioception, Substrate, World};
use crate::rng::Pcg32;
use crate::signals::BrainSignals;
use crate::util::{angle_diff, clamp, hypot, Vec2};

/// Spine points, head first. 18 is enough for a smooth curve at desktop scale.
pub const SEGMENTS: usize = 18;
/// Distance between adjacent spine points, in scene units.
pub const SEGMENT_LEN: f32 = 2.4;
pub const BODY_LEN: f32 = SEGMENT_LEN * (SEGMENTS as f32 - 1.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KoiState {
    /// Steady swimming: beat, then glide.
    Cruise,
    /// Almost stationary, holding position on the pectoral fins.
    Hover,
    /// A deliberate change of direction.
    Turn,
    /// C-start escape, then a fast run.
    Dart,
    /// Night-time stillness. Fish do not close their eyes, but they do stop.
    Resting,
}

pub struct Koi {
    pub pos: Vec2,
    pub heading: f32,
    pub state: KoiState,
    /// Tail-beat phase, 0..1.
    pub phase: f32,
    /// Visible spine, head first, including the lateral wave.
    pub spine: Vec<Vec2>,
    pub speed: f32,
    /// Current tail-beat amplitude scale, 0..1. Near zero during a glide.
    pub beat: f32,
    /// Signed C-start bend, -1..1. Nonzero only for the first moments of a dart.
    pub c_bend: f32,
    /// Apparent depth, 0 = deep, 1 = at the surface. Drives scale and speed:
    /// the desktop is flat, so depth has to read as size.
    pub depth: f32,
    depth_target: f32,
    pub state_timer: f32,
    /// Heading change still owed to the current manoeuvre, in radians. Turns
    /// are *committed* rather than instantaneous, so a fish rolls into them.
    pending_turn: f32,
    /// Blocks a second startle while one is still playing out.
    pub dart_cooldown: f32,
    pub time: f32,

    /// Recorded centreline the body follows, oldest first.
    path: Vec<Vec2>,
    arc: Vec<f32>,
    head_s: f32,

    rng: Pcg32,
}

impl Koi {
    pub fn new(at: Vec2, seed: u64) -> Self {
        let mut rng = Pcg32::new(seed);
        let heading = rng.range(0.0, std::f32::consts::TAU);
        let start = BODY_LEN * 1.5;
        let tail = Vec2::new(
            at.x - heading.cos() * start,
            at.y - heading.sin() * start,
        );
        let mut koi = Koi {
            pos: at,
            heading,
            state: KoiState::Cruise,
            phase: rng.range(0.0, 1.0),
            spine: vec![at; SEGMENTS],
            speed: 18.0,
            beat: 1.0,
            c_bend: 0.0,
            depth: rng.range(0.35, 0.75),
            depth_target: 0.5,
            state_timer: rng.range(1.5, 4.0),
            pending_turn: 0.0,
            dart_cooldown: 0.0,
            time: rng.range(0.0, 100.0),
            path: vec![tail, at],
            arc: vec![0.0, start],
            head_s: start,
            rng,
        };
        koi.resample_spine();
        koi
    }

    /// Uniform scale from apparent depth. A koi near the surface reads larger.
    pub fn scale(&self) -> f32 {
        0.82 + 0.36 * self.depth
    }

    /// Lateral wave amplitude at body fraction `s` (0 = snout, 1 = tail tip).
    ///
    /// Cubic growth toward the tail is what makes this carangiform rather than
    /// anguilliform: the head barely moves, the tail sweeps widely. The small
    /// constant keeps the head from being perfectly rigid, which looks dead.
    fn wave_amplitude(&self, s: f32) -> f32 {
        let taper = 0.06 + 0.94 * s * s * s;
        taper * 3.4 * self.beat
    }

    fn sample(&self, s: f32) -> Vec2 {
        let total = *self.arc.last().unwrap_or(&0.0);
        let s = s.clamp(0.0, total);
        let mut i = 0;
        while i + 1 < self.arc.len() && self.arc[i + 1] < s {
            i += 1;
        }
        if i + 1 >= self.path.len() {
            return *self.path.last().unwrap();
        }
        let seg = (self.arc[i + 1] - self.arc[i]).max(1e-5);
        let t = ((s - self.arc[i]) / seg).clamp(0.0, 1.0);
        Vec2::new(
            self.path[i].x + (self.path[i + 1].x - self.path[i].x) * t,
            self.path[i].y + (self.path[i + 1].y - self.path[i].y) * t,
        )
    }

    /// Rebuild the visible spine: centreline from the path, plus the travelling
    /// wave and any C-start bend laid across it.
    fn resample_spine(&mut self) {
        // Wavelength is a little under one body length, as in the animal.
        const WAVELENGTH: f32 = 0.85;
        for i in 0..SEGMENTS {
            let s = i as f32 / (SEGMENTS - 1) as f32;
            let centre = self.sample(self.head_s - SEGMENT_LEN * i as f32);
            let ahead = self.sample(self.head_s - SEGMENT_LEN * (i as f32 - 1.0));
            // Normal to the local centreline, so the wave is always lateral.
            let mut tx = ahead.x - centre.x;
            let mut ty = ahead.y - centre.y;
            let l = hypot(tx, ty);
            if l < 1e-4 {
                tx = self.heading.cos();
                ty = self.heading.sin();
            } else {
                tx /= l;
                ty /= l;
            }
            let travelling = (std::f32::consts::TAU * (self.phase - s / WAVELENGTH)).sin();
            // The C-start is a standing bend, not a travelling one: the whole
            // body curves one way at once. Weighted toward the tail as well,
            // but far stronger than any cruising beat.
            let c = self.c_bend * (s * s) * 9.0;
            let offset = self.wave_amplitude(s) * travelling + c;
            self.spine[i] = Vec2::new(centre.x - ty * offset, centre.y + tx * offset);
        }
        self.pos = self.spine[0];
    }

    fn trim_path(&mut self) {
        let keep_from = self.head_s - BODY_LEN * 2.0;
        if keep_from <= 0.0 || self.path.len() < 8 {
            return;
        }
        let mut cut = 0;
        while cut + 1 < self.arc.len() && self.arc[cut + 1] < keep_from {
            cut += 1;
        }
        if cut > 0 {
            self.path.drain(0..cut);
            self.arc.drain(0..cut);
        }
    }

    fn advance(&mut self, step: f32) {
        let head = self.sample(self.head_s);
        let next = Vec2::new(
            head.x + self.heading.cos() * step,
            head.y + self.heading.sin() * step,
        );
        self.path.push(next);
        self.arc.push(self.head_s + step);
        self.head_s += step;
        self.trim_path();
    }

    /// Startle. The escape is a real behaviour with a shape, not a teleport:
    /// a C-bend away from the threat, then a fast run in the new direction.
    pub fn startle(&mut self, away_from: Option<Vec2>) {
        if self.dart_cooldown > 0.0 {
            return;
        }
        self.state = KoiState::Dart;
        self.state_timer = self.rng.range(0.55, 0.95);
        self.dart_cooldown = 1.6;
        let turn_away = match away_from {
            Some(t) => {
                let bearing = (self.pos.y - t.y).atan2(self.pos.x - t.x);
                angle_diff(self.heading, bearing)
            }
            None => self.rng.range(-2.2, 2.2),
        };
        // The C bends toward the escape direction; sign carries the turn.
        self.c_bend = if turn_away >= 0.0 { 1.0 } else { -1.0 };
        self.pending_turn = turn_away;
        self.speed = self.speed.max(120.0);
        self.beat = 1.0;
    }

    fn take_turn(&mut self, dt: f32, rate: f32) {
        if self.pending_turn.abs() < 1e-3 {
            return;
        }
        let step = rate * dt * self.pending_turn.signum();
        if step.abs() >= self.pending_turn.abs() {
            self.heading += self.pending_turn;
            self.pending_turn = 0.0;
        } else {
            self.heading += step;
            self.pending_turn -= step;
        }
    }
}

impl Body for Koi {
    fn substrate(&self) -> Substrate {
        Substrate::Swimmer
    }

    fn step(&mut self, dt: f32, drives: &BrainSignals, world: &World) {
        self.time += dt;
        self.dart_cooldown = (self.dart_cooldown - dt).max(0.0);
        self.state_timer -= dt;

        if drives.sleep {
            self.state = KoiState::Resting;
        } else if self.state == KoiState::Resting {
            self.state = KoiState::Cruise;
            self.state_timer = self.rng.range(2.0, 5.0);
        }

        // The escape is the one drive that overrides everything.
        if drives.escape {
            self.startle(world.cursor);
        }

        if self.state_timer <= 0.0 {
            match self.state {
                KoiState::Dart => {
                    self.state = KoiState::Cruise;
                    self.state_timer = self.rng.range(2.0, 4.5);
                }
                KoiState::Turn => {
                    self.state = KoiState::Cruise;
                    self.state_timer = self.rng.range(2.5, 6.0);
                }
                KoiState::Resting => self.state_timer = 2.0,
                KoiState::Cruise | KoiState::Hover => {
                    // Koi mostly cruise, sometimes hang, occasionally turn for
                    // no visible reason — which is most of what makes a fish
                    // in a pond look alive.
                    let r = self.rng.f32();
                    if r < 0.22 {
                        self.state = KoiState::Hover;
                        self.state_timer = self.rng.range(1.5, 4.0);
                    } else if r < 0.55 {
                        self.state = KoiState::Turn;
                        self.state_timer = self.rng.range(0.8, 1.8);
                        self.pending_turn = self.rng.range(-1.5, 1.5);
                    } else {
                        self.state = KoiState::Cruise;
                        self.state_timer = self.rng.range(2.5, 6.0);
                    }
                    // Drift to a new depth now and then: a slow rise or sink.
                    if self.rng.f32() < 0.5 {
                        self.depth_target = self.rng.range(0.15, 1.0);
                    }
                }
            }
        }

        // Depth eases, never jumps: a fish changing level is a slow thing.
        self.depth += (self.depth_target - self.depth) * (0.55 * dt).min(1.0);
        self.depth = clamp(self.depth, 0.0, 1.0);

        // Target speed by state, scaled by temperature and by depth — fish are
        // ectotherms too, and a deeper fish reads as slower because it is
        // further away.
        let depth_speed = 0.75 + 0.35 * self.depth;
        let target = match self.state {
            KoiState::Resting => 1.5,
            KoiState::Hover => 4.0,
            KoiState::Turn => 16.0,
            KoiState::Cruise => 22.0 + drives.walk_drive * 14.0,
            KoiState::Dart => 150.0,
        } * drives.tempo
            * depth_speed;

        // Burst-and-coast. Under a dart or a turn the fish is actively beating;
        // at cruise it alternates, and the glide is where the speed bleeds off.
        let beating = match self.state {
            KoiState::Dart | KoiState::Turn => true,
            KoiState::Resting => false,
            KoiState::Hover => (self.time * 0.7).sin() > 0.75,
            // Roughly a third beating, two thirds gliding.
            KoiState::Cruise => (self.time * 0.9).sin() > 0.25,
        };
        let beat_target = if beating { 1.0 } else { 0.12 };
        self.beat += (beat_target - self.beat) * (5.0 * dt).min(1.0);

        // Thrust only while beating; drag always. This is what produces the
        // coast rather than a constant-velocity slide.
        if beating {
            self.speed += (target - self.speed) * (2.6 * dt).min(1.0);
        } else {
            self.speed += (target * 0.45 - self.speed) * (0.9 * dt).min(1.0);
        }
        self.speed = self.speed.max(0.0);

        // The C-start straightens as the fish accelerates out of it.
        if self.c_bend.abs() > 1e-3 {
            self.c_bend *= (1.0 - (7.0 * dt)).max(0.0);
        }

        // Turning: fast during a dart's escape turn, leisurely otherwise.
        let turn_rate = if self.state == KoiState::Dart { 7.0 } else { 1.6 };
        self.take_turn(dt, turn_rate);
        // Steering drive nudges the heading between manoeuvres.
        self.heading += drives.turn_bias * 0.5 * dt;

        // Tail-beat frequency rises with effort; a gliding fish barely beats.
        let freq = 0.5 + (self.speed / 60.0).min(2.4) * self.beat;
        self.phase = (self.phase + freq * dt) % 1.0;

        let step = self.speed * dt;
        if step > 0.0 {
            self.advance(step);
        }

        // Turn away from the edge of the display rather than stopping at it —
        // a fish in a pond turns at the wall.
        let hw = world.bounds.0 / 2.0 - 40.0;
        let hh = world.bounds.1 / 2.0 - 40.0;
        if self.pos.x.abs() > hw || self.pos.y.abs() > hh {
            let to_center = (-self.pos.y).atan2(-self.pos.x);
            self.heading += angle_diff(self.heading, to_center) * (2.5 * dt).min(1.0);
        }

        self.resample_spine();
    }

    fn position(&self) -> Vec2 {
        self.pos
    }

    fn heading(&self) -> f32 {
        self.heading
    }

    fn proprioception(&self) -> Proprioception {
        Proprioception {
            drive: clamp(self.speed / 60.0, 0.0, 1.0),
            phase: self.phase,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        World {
            bounds: (1512.0, 982.0),
            ledges: Vec::new(),
            cursor: None,
        }
    }

    fn drives() -> BrainSignals {
        BrainSignals::new()
    }

    fn run(k: &mut Koi, secs: f32, d: &BrainSignals) {
        let dt = 1.0 / 60.0;
        for _ in 0..((secs / dt) as u32) {
            k.step(dt, d, &world());
        }
    }

    #[test]
    fn a_koi_is_a_swimmer() {
        assert_eq!(Koi::new(Vec2::ZERO, 1).substrate(), Substrate::Swimmer);
    }

    #[test]
    fn it_swims_forward() {
        let mut k = Koi::new(Vec2::ZERO, 1);
        k.heading = 0.0;
        let start = k.position();
        run(&mut k, 3.0, &drives());
        assert!(k.position().dist(start) > 25.0, "barely moved");
    }

    /// The body is inextensible along its centreline, so the spine must not
    /// stretch. Measured on the centreline rather than the visible spine,
    /// which the lateral wave legitimately shortens end to end.
    #[test]
    fn the_body_does_not_stretch() {
        let mut k = Koi::new(Vec2::ZERO, 3);
        run(&mut k, 8.0, &drives());
        for i in 1..SEGMENTS {
            let a = k.sample(k.head_s - SEGMENT_LEN * (i - 1) as f32);
            let b = k.sample(k.head_s - SEGMENT_LEN * i as f32);
            let d = a.dist(b);
            assert!(
                (d - SEGMENT_LEN).abs() < 0.05,
                "centreline segment {i} is {d}, expected {SEGMENT_LEN}"
            );
        }
    }

    /// Carangiform, not anguilliform: the tail must sweep far more than the
    /// head. A uniform wave would be a swimming eel, not a koi.
    #[test]
    fn the_tail_sweeps_much_further_than_the_head() {
        let mut k = Koi::new(Vec2::ZERO, 5);
        k.beat = 1.0;
        let head_amp = k.wave_amplitude(0.0);
        let mid_amp = k.wave_amplitude(0.5);
        let tail_amp = k.wave_amplitude(1.0);
        assert!(tail_amp > mid_amp * 3.0, "tail {tail_amp} vs mid {mid_amp}");
        assert!(tail_amp > head_amp * 10.0, "tail {tail_amp} vs head {head_amp}");
        assert!(head_amp > 0.0, "a perfectly rigid head looks dead");
    }

    /// Burst-and-coast: at cruise the fish must actually spend time gliding
    /// with the tail nearly still, not beat continuously.
    #[test]
    fn cruising_alternates_beating_and_gliding() {
        let mut k = Koi::new(Vec2::ZERO, 7);
        k.state = KoiState::Cruise;
        k.state_timer = 60.0;
        let dt = 1.0 / 60.0;
        let (mut min_beat, mut max_beat) = (f32::MAX, f32::MIN);
        for _ in 0..900 {
            k.step(dt, &drives(), &world());
            if k.state != KoiState::Cruise {
                break;
            }
            min_beat = min_beat.min(k.beat);
            max_beat = max_beat.max(k.beat);
        }
        assert!(max_beat > 0.8, "never beats: {max_beat}");
        assert!(min_beat < 0.35, "never glides: {min_beat}");
    }

    /// The escape has a shape. A startled fish bends into a C before it goes.
    #[test]
    fn a_startle_produces_a_c_bend_then_straightens() {
        let mut k = Koi::new(Vec2::ZERO, 11);
        k.heading = 0.0;
        run(&mut k, 1.0, &drives());
        k.startle(Some(Vec2::new(200.0, 0.0)));
        assert_eq!(k.state, KoiState::Dart);
        assert!(k.c_bend.abs() > 0.5, "no C-start bend: {}", k.c_bend);

        run(&mut k, 0.5, &drives());
        assert!(
            k.c_bend.abs() < 0.2,
            "the C must straighten as it accelerates: {}",
            k.c_bend
        );
        assert!(k.speed > 60.0, "a dart should be fast: {}", k.speed);
    }

    #[test]
    fn a_startle_turns_away_from_the_threat() {
        let mut k = Koi::new(Vec2::ZERO, 13);
        k.heading = 0.0; // pointing +x
        run(&mut k, 0.5, &drives());
        // Threat directly ahead: the fish must end up heading away from it.
        k.startle(Some(Vec2::new(300.0, 0.0)));
        run(&mut k, 0.8, &drives());
        let facing_x = k.heading.cos();
        assert!(facing_x < 0.3, "still heading at the threat: cos {facing_x}");
    }

    #[test]
    fn startles_are_rate_limited() {
        let mut k = Koi::new(Vec2::ZERO, 17);
        k.startle(None);
        let first = k.state_timer;
        k.startle(None); // immediately again
        assert_eq!(k.state_timer, first, "a second startle should be ignored");
    }

    #[test]
    fn resting_nearly_stops_it() {
        let mut k = Koi::new(Vec2::ZERO, 19);
        run(&mut k, 2.0, &drives());
        let mut sleepy = drives();
        sleepy.sleep = true;
        run(&mut k, 4.0, &sleepy);
        assert_eq!(k.state, KoiState::Resting);
        assert!(k.speed < 6.0, "a resting koi should barely move: {}", k.speed);
    }

    /// Depth is the only cue a flat desktop has for a third dimension, so it
    /// must move — and must ease rather than jump.
    #[test]
    fn depth_drifts_smoothly_and_changes_apparent_size() {
        let mut k = Koi::new(Vec2::ZERO, 23);
        k.depth = 0.2;
        k.depth_target = 1.0;
        let small = k.scale();
        let mut prev = k.depth;
        let mut max_jump = 0.0f32;
        for _ in 0..600 {
            k.step(1.0 / 60.0, &drives(), &world());
            max_jump = max_jump.max((k.depth - prev).abs());
            prev = k.depth;
        }
        assert!(max_jump < 0.02, "depth jumped by {max_jump}");
        assert!(k.scale() > small, "rising should make it larger");
        assert!((0.0..=1.0).contains(&k.depth));
    }

    #[test]
    fn it_stays_on_screen() {
        let mut k = Koi::new(Vec2::new(700.0, 430.0), 29);
        k.heading = 0.5;
        run(&mut k, 30.0, &drives());
        assert!(k.position().x.abs() < 1512.0 / 2.0, "x {}", k.position().x);
        assert!(k.position().y.abs() < 982.0 / 2.0, "y {}", k.position().y);
    }

    #[test]
    fn proprioception_tracks_swimming() {
        let mut k = Koi::new(Vec2::ZERO, 31);
        run(&mut k, 2.0, &drives());
        let p = k.proprioception();
        assert!((0.0..=1.0).contains(&p.drive));
        assert!((0.0..1.0).contains(&p.phase));
    }
}
