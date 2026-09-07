//! The hognose snake's body: a burrower.
//!
//! Creature #8, and procedural for the same reason the koi is: **there is no
//! snake connectome.** No reptile has a synapse-resolution wiring diagram at
//! any scale the fly's extract could be compared with, so this animal is a
//! hand-written behaviour model labelled
//! [`Provenance::Procedural`](crate::creature::Provenance::Procedural)
//! everywhere the app can show it. See DESERT_PLAN.md.
//!
//! ## What makes it a hognose and not a rope
//!
//! *Heterodon* is the snake with the theatre. Three behaviours are so
//! characteristic of the genus that a model without them is a generic snake:
//!
//! * **The bluff.** Threatened, it spreads its neck into a hood like a small
//!   cobra, hisses, and mock-strikes — with its mouth *closed*. It is almost
//!   never a real bite. The hood is [`Hognose::hood`], the lunge is
//!   [`Hognose::strike`].
//! * **Playing dead.** If the bluff fails it flips onto its back, gapes, lets
//!   its tongue hang out, and goes limp; turned right side up it will roll
//!   over again. [`Hognose::belly_up`] is that, and the body is committed to
//!   it — a fresh threat during the act does not interrupt it, which is the
//!   whole point of the act.
//! * **The nose.** The upturned rostral scale is a shovel, and the animal
//!   spends much of its day under loose substrate. [`Hognose::buried`] sinks
//!   it; in a terrarium the sand hides it and in free roam it fades.
//!
//! Locomotion is lateral undulation: unlike the koi's carangiform wave, the
//! amplitude is nearly uniform along the body — the whole animal throws the
//! same S-curves, and the head steers by following them. The centreline
//! follows the head's recorded path, as the worm's and the koi's do, because
//! a body of fixed length cannot be turned by moving one end without
//! retracing its track.

use crate::creature::{Body, Proprioception, Substrate, World};
use crate::rng::Pcg32;
use crate::signals::BrainSignals;
use crate::util::{angle_diff, clamp, hypot, Vec2};

/// Spine points, head first.
pub const SEGMENTS: usize = 22;
/// Distance between adjacent spine points, in scene units.
pub const SEGMENT_LEN: f32 = 2.6;
pub const BODY_LEN: f32 = SEGMENT_LEN * (SEGMENTS as f32 - 1.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HognoseState {
    /// Lateral undulation across the substrate.
    Slither,
    /// Coiled and still, tongue-flicking now and then.
    Rest,
    /// Hood spread, hissing, mock-striking at the threat.
    Bluff,
    /// On its back, gaping, limp. Committed.
    PlayDead,
    /// Under the substrate. Sinks in, waits, comes back up.
    Burrow,
}

pub struct Hognose {
    pub pos: Vec2,
    pub heading: f32,
    pub state: HognoseState,
    /// Undulation phase, 0..1.
    pub phase: f32,
    /// Visible spine, head first.
    pub spine: Vec<Vec2>,
    pub speed: f32,
    /// Wave amplitude scale, 0..1. Falls toward zero when the animal stops.
    pub beat: f32,
    /// Neck spread, 0..1. The hood.
    pub hood: f32,
    /// Mock-strike envelope, 0..1: the head lunges forward and returns.
    pub strike: f32,
    /// How far over it is, 0..1. 1 is fully belly-up.
    pub belly_up: f32,
    /// How far under the substrate it is, 0..1.
    pub buried: f32,
    /// Tongue flick envelope, 0..1.
    pub tongue: f32,
    pub state_timer: f32,
    /// Heading change still owed to the current manoeuvre, in radians.
    pending_turn: f32,
    /// Blocks a second bluff for a while after one ends.
    pub bluff_cooldown: f32,
    /// Seconds of continuous threat while bluffing: the fuse on playing dead.
    threat_time: f32,
    /// Seconds since the last threat, so the act can end when it is safe.
    calm_time: f32,
    /// Where the threat was, so the strikes aim at it.
    threat_at: Option<Vec2>,
    /// Slow drive toward burrowing: the more it has been out, the likelier.
    exposure: f32,
    pub time: f32,

    path: Vec<Vec2>,
    arc: Vec<f32>,
    head_s: f32,

    rng: Pcg32,
}

impl Hognose {
    pub fn new(at: Vec2, seed: u64) -> Self {
        let mut rng = Pcg32::new(seed);
        let heading = rng.range(0.0, std::f32::consts::TAU);
        let start = BODY_LEN * 1.5;
        let tail = Vec2::new(
            at.x - heading.cos() * start,
            at.y - heading.sin() * start,
        );
        let mut h = Hognose {
            pos: at,
            heading,
            state: HognoseState::Slither,
            phase: rng.range(0.0, 1.0),
            spine: vec![at; SEGMENTS],
            speed: 10.0,
            beat: 1.0,
            hood: 0.0,
            strike: 0.0,
            belly_up: 0.0,
            buried: 0.0,
            tongue: 0.0,
            state_timer: rng.range(3.0, 7.0),
            pending_turn: 0.0,
            bluff_cooldown: 0.0,
            threat_time: 0.0,
            calm_time: 10.0,
            threat_at: None,
            exposure: 0.0,
            time: rng.range(0.0, 100.0),
            path: vec![tail, at],
            arc: vec![0.0, start],
            head_s: start,
            rng,
        };
        h.resample_spine();
        h
    }

    /// Lateral wave amplitude at body fraction `s` (0 = snout, 1 = tail tip).
    /// Nearly uniform — this is a snake, not a fish — with a little less at
    /// the head, which leads, and a little more toward the thinner tail.
    fn wave_amplitude(&self, s: f32) -> f32 {
        let taper = 0.55 + 0.45 * s;
        taper * 3.2 * self.beat
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

    fn resample_spine(&mut self) {
        // About 1.7 wavelengths along the body: the classic double S.
        const WAVELENGTH: f32 = 0.6;
        let strike_reach = self.strike * 9.0;
        for i in 0..SEGMENTS {
            let s = i as f32 / (SEGMENTS - 1) as f32;
            let centre = self.sample(self.head_s - SEGMENT_LEN * i as f32);
            let ahead = self.sample(self.head_s - SEGMENT_LEN * (i as f32 - 1.0));
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
            // Playing dead: the body goes limp into a loose, static curl.
            let limp = self.belly_up * (s * 2.4).sin() * 5.0;
            let offset = self.wave_amplitude(s) * travelling * (1.0 - self.belly_up) + limp;
            // The mock strike carries the front of the body forward and back.
            let lunge = strike_reach * (1.0 - s * 4.0).max(0.0);
            self.spine[i] = Vec2::new(
                centre.x - ty * offset + tx * lunge,
                centre.y + tx * offset + ty * lunge,
            );
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

    /// Something has frightened it. From the ground the answer is the bluff;
    /// while bluffing, a *second* fright is what tips it into playing dead.
    /// Underground it stays put, and while playing dead it is committed.
    pub fn threaten(&mut self, from: Option<Vec2>) {
        self.threat_at = from;
        self.calm_time = 0.0;
        match self.state {
            HognoseState::Slither | HognoseState::Rest => {
                if self.bluff_cooldown > 0.0 {
                    return;
                }
                self.state = HognoseState::Bluff;
                self.state_timer = self.rng.range(4.0, 7.0);
                self.threat_time = 0.0;
                self.speed = 0.0;
                if let Some(t) = from {
                    let bearing = (t.y - self.pos.y).atan2(t.x - self.pos.x);
                    self.pending_turn = angle_diff(self.heading, bearing);
                }
            }
            HognoseState::Bluff => {
                // A repeat threat *after* the bluff has had a moment to work
                // is the failed bluff. One arriving in the first half second
                // is the same fright still landing.
                if self.threat_time > 0.5 {
                    self.play_dead();
                }
            }
            HognoseState::PlayDead | HognoseState::Burrow => {}
        }
    }

    pub fn play_dead(&mut self) {
        self.state = HognoseState::PlayDead;
        self.state_timer = self.rng.range(8.0, 15.0);
        self.hood = 0.0;
        self.strike = 0.0;
        self.speed = 0.0;
        self.bluff_cooldown = 6.0;
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

impl Body for Hognose {
    fn substrate(&self) -> Substrate {
        Substrate::Burrower
    }

    fn step(&mut self, dt: f32, drives: &BrainSignals, world: &World) {
        self.time += dt;
        self.bluff_cooldown = (self.bluff_cooldown - dt).max(0.0);
        self.state_timer -= dt;
        self.calm_time += dt;

        if drives.escape {
            self.threaten(world.cursor);
        }
        if drives.nervous > 0.3 {
            self.calm_time = 0.0;
        }

        // Night, or a long-idle machine: under the sand until morning.
        if drives.sleep && self.state != HognoseState::PlayDead {
            self.state = HognoseState::Burrow;
            self.state_timer = 5.0;
        }

        if self.state_timer <= 0.0 {
            match self.state {
                HognoseState::Bluff => {
                    // The bluff is over: either it worked and the threat
                    // went away, or it did not.
                    if self.calm_time < 1.5 {
                        self.play_dead();
                    } else {
                        self.state = HognoseState::Slither;
                        self.state_timer = self.rng.range(3.0, 6.0);
                        self.bluff_cooldown = 5.0;
                        self.pending_turn = self.rng.range(2.4, 3.8)
                            * if self.rng.f32() < 0.5 { 1.0 } else { -1.0 };
                    }
                }
                HognoseState::PlayDead => {
                    // Only get up once nothing has bothered it for a while;
                    // a hognose turned right side up while the threat is
                    // still there rolls straight back over.
                    if self.calm_time > 3.0 {
                        self.state = HognoseState::Slither;
                        self.state_timer = self.rng.range(3.0, 6.0);
                    } else {
                        self.state_timer = 2.0;
                    }
                }
                HognoseState::Burrow => {
                    if drives.sleep {
                        self.state_timer = 5.0;
                    } else if self.buried > 0.95 {
                        // Come back up somewhere on the far side of a nap.
                        self.state = HognoseState::Slither;
                        self.state_timer = self.rng.range(4.0, 8.0);
                        self.exposure = 0.0;
                        self.heading += self.rng.range(-1.2, 1.2);
                    } else {
                        self.state_timer = 1.0;
                    }
                }
                HognoseState::Slither | HognoseState::Rest => {
                    let r = self.rng.f32();
                    let dig = clamp(self.exposure / 90.0, 0.0, 0.6);
                    if r < dig {
                        self.state = HognoseState::Burrow;
                        self.state_timer = self.rng.range(8.0, 20.0);
                    } else if r < dig + 0.30 {
                        self.state = HognoseState::Rest;
                        self.state_timer = self.rng.range(2.0, 6.0);
                    } else {
                        self.state = HognoseState::Slither;
                        self.state_timer = self.rng.range(3.0, 7.0);
                        self.pending_turn = self.rng.range(-1.4, 1.4);
                    }
                }
            }
        }

        if self.state != HognoseState::Burrow {
            self.exposure += dt;
        }

        // --- the theatre.
        let hood_target = if self.state == HognoseState::Bluff { 1.0 } else { 0.0 };
        self.hood += (hood_target - self.hood) * (6.0 * dt).min(1.0);
        if self.state == HognoseState::Bluff {
            self.threat_time += dt;
            // A mock strike every second or so: a sharp lunge, a slower
            // recovery. The head snaps out and the mouth stays shut.
            let cycle = (self.time * 0.9) % 1.0;
            let target = if cycle < 0.12 { 1.0 } else { 0.0 };
            let rate = if target > self.strike { 30.0 } else { 6.0 };
            self.strike += (target - self.strike) * (rate * dt).min(1.0);
            // Face the threat.
            if let Some(t) = self.threat_at {
                let bearing = (t.y - self.pos.y).atan2(t.x - self.pos.x);
                self.pending_turn = angle_diff(self.heading, bearing);
            }
        } else {
            self.strike += (0.0 - self.strike) * (8.0 * dt).min(1.0);
        }
        let over = if self.state == HognoseState::PlayDead { 1.0 } else { 0.0 };
        // Flops over fast; rights itself slowly and a little reluctantly.
        let roll_rate = if over > self.belly_up { 2.2 } else { 1.1 };
        self.belly_up += (over - self.belly_up) * (roll_rate * dt).min(1.0);
        let under = if self.state == HognoseState::Burrow { 1.0 } else { 0.0 };
        self.buried += (under - self.buried) * (0.8 * dt).min(1.0);
        // Tongue: flicks in little bursts while resting or slithering, hangs
        // out constantly when playing dead.
        let flick = if self.state == HognoseState::PlayDead {
            1.0
        } else if matches!(self.state, HognoseState::Slither | HognoseState::Rest) {
            let c = (self.time * 1.3) % 1.0;
            if c < 0.18 { ((c / 0.18) * std::f32::consts::PI).sin() } else { 0.0 }
        } else {
            0.0
        };
        self.tongue += (flick - self.tongue) * (20.0 * dt).min(1.0);

        // --- locomotion.
        let target = match self.state {
            HognoseState::Slither => 11.0 + drives.walk_drive * 7.0,
            HognoseState::Rest => 0.0,
            HognoseState::Bluff => 0.0,
            HognoseState::PlayDead => 0.0,
            // Digging in is a slow shuffle; buried, it is still.
            HognoseState::Burrow => 2.0 * (1.0 - self.buried),
        } * drives.tempo;
        self.speed += (target - self.speed) * (2.0 * dt).min(1.0);
        self.speed = self.speed.max(0.0);

        // The S-curves stay while it is still: a resting snake is coiled and a
        // bluffing one holds its posture. Only the limp act and the burrow
        // straighten it (the act lays its own curl over the spine).
        let beat_target = match self.state {
            HognoseState::Slither | HognoseState::Rest | HognoseState::Bluff => 1.0,
            HognoseState::PlayDead | HognoseState::Burrow => 0.0,
        };
        self.beat += (beat_target - self.beat) * (3.0 * dt).min(1.0);

        let turn_rate = if self.state == HognoseState::Bluff { 4.0 } else { 1.2 };
        self.take_turn(dt, turn_rate);
        if self.state == HognoseState::Slither {
            self.heading += drives.turn_bias * 0.4 * dt;
        }

        // The wave travels only when the animal does: a snake's S-curves are
        // its footsteps. A bluffing one sways a little in place.
        let sway = if self.state == HognoseState::Bluff { 0.12 } else { 0.0 };
        let freq = sway + (self.speed / 14.0).min(1.6) * self.beat;
        self.phase = (self.phase + freq * dt) % 1.0;

        let step = self.speed * dt;
        if step > 0.0 {
            self.advance(step);
        }

        if world.region.outside(self.pos, 30.0) {
            let home = world.region.bearing_home(self.pos);
            self.heading += angle_diff(self.heading, home) * (2.0 * dt).min(1.0);
        } else if let (Some(a), HognoseState::Slither) = (world.attractor, self.state) {
            let to_a = (a.y - self.pos.y).atan2(a.x - self.pos.x);
            self.heading += angle_diff(self.heading, to_a) * (crate::body::CURIOSITY * dt).min(1.0);
            // Reached its hide: settle there.
            if self.pos.dist(a) < 22.0 && self.state_timer > 1.0 {
                self.state = HognoseState::Rest;
                self.state_timer = self.rng.range(3.0, 8.0);
            }
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
            drive: clamp(self.speed / 18.0, 0.0, 1.0),
            phase: self.phase,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::habitat::Region;

    fn world() -> World {
        World {
            region: Region::centered((1512.0, 982.0)),
            ledges: Vec::new(),
            cursor: None,
            attractor: None,
        }
    }

    fn drives() -> BrainSignals {
        BrainSignals::new()
    }

    fn run(h: &mut Hognose, secs: f32, d: &BrainSignals) {
        let dt = 1.0 / 60.0;
        for _ in 0..((secs / dt) as u32) {
            h.step(dt, d, &world());
        }
    }

    fn slithering(seed: u64) -> Hognose {
        let mut h = Hognose::new(Vec2::ZERO, seed);
        h.state = HognoseState::Slither;
        h.state_timer = 60.0;
        h
    }

    #[test]
    fn a_hognose_is_a_burrower() {
        assert_eq!(Hognose::new(Vec2::ZERO, 1).substrate(), Substrate::Burrower);
    }

    #[test]
    fn it_slithers_forward() {
        let mut h = slithering(1);
        let start = h.position();
        run(&mut h, 4.0, &drives());
        assert!(h.position().dist(start) > 20.0, "barely moved");
    }

    #[test]
    fn the_body_does_not_stretch() {
        let mut h = slithering(3);
        run(&mut h, 8.0, &drives());
        for i in 1..SEGMENTS {
            let a = h.sample(h.head_s - SEGMENT_LEN * (i - 1) as f32);
            let b = h.sample(h.head_s - SEGMENT_LEN * i as f32);
            let d = a.dist(b);
            assert!((d - SEGMENT_LEN).abs() < 0.05, "segment {i} is {d}");
        }
    }

    /// Lateral undulation, not a fish's tail beat: the head throws nearly as
    /// much of a curve as the tail does.
    #[test]
    fn the_wave_is_nearly_uniform_along_the_body() {
        let mut h = slithering(5);
        h.beat = 1.0;
        let head = h.wave_amplitude(0.1);
        let tail = h.wave_amplitude(1.0);
        assert!(tail < head * 2.0, "tail {tail} vs head {head}: that is a fish");
        assert!(head > 1.0, "the head must move too");
    }

    /// The bluff: a threat on the ground spreads the hood, stops the animal
    /// and produces mock strikes — and does not, on its own, produce a corpse.
    #[test]
    fn a_threat_produces_a_hooded_bluff_with_strikes() {
        let mut h = slithering(7);
        run(&mut h, 1.0, &drives());
        h.threaten(Some(Vec2::new(100.0, 0.0)));
        assert_eq!(h.state, HognoseState::Bluff);
        let mut max_hood: f32 = 0.0;
        let mut max_strike: f32 = 0.0;
        let mut max_speed: f32 = 0.0;
        let dt = 1.0 / 60.0;
        for _ in 0..180 {
            h.step(dt, &drives(), &world());
            max_hood = max_hood.max(h.hood);
            max_strike = max_strike.max(h.strike);
            max_speed = max_speed.max(h.speed);
        }
        assert!(max_hood > 0.9, "no hood: {max_hood}");
        assert!(max_strike > 0.6, "no mock strike: {max_strike}");
        assert!(max_speed < 3.0, "a bluffing snake stands its ground: {max_speed}");
        assert_ne!(h.state, HognoseState::PlayDead, "one fright is not a failed bluff");
    }

    /// A threat that keeps coming *after* the bluff is what tips it over.
    #[test]
    fn a_failed_bluff_becomes_playing_dead_and_is_committed() {
        let mut h = slithering(11);
        run(&mut h, 1.0, &drives());
        h.threaten(Some(Vec2::new(100.0, 0.0)));
        run(&mut h, 1.5, &drives());
        h.threaten(Some(Vec2::new(60.0, 0.0)));
        assert_eq!(h.state, HognoseState::PlayDead);
        run(&mut h, 1.5, &drives());
        assert!(h.belly_up > 0.8, "not on its back: {}", h.belly_up);
        assert!(h.hood < 0.1, "the hood goes down when it dies: {}", h.hood);
        // Poke it again: it stays dead.
        h.threaten(Some(Vec2::new(20.0, 0.0)));
        assert_eq!(h.state, HognoseState::PlayDead);
        // And it does not get up while the threat is still present.
        let mut nervous = drives();
        nervous.nervous = 0.8;
        run(&mut h, 16.0, &nervous);
        assert_eq!(h.state, HognoseState::PlayDead, "got up under a standing threat");
        // Left alone, it eventually rights itself.
        run(&mut h, 6.0, &drives());
        assert_ne!(h.state, HognoseState::PlayDead);
        run(&mut h, 4.0, &drives());
        assert!(h.belly_up < 0.2, "still on its back: {}", h.belly_up);
    }

    #[test]
    fn a_bluff_that_works_ends_with_the_snake_leaving() {
        let mut h = slithering(13);
        run(&mut h, 1.0, &drives());
        h.threaten(Some(Vec2::new(100.0, 0.0)));
        run(&mut h, 8.0, &drives());
        assert_ne!(h.state, HognoseState::PlayDead);
        assert_ne!(h.state, HognoseState::Bluff);
        assert!(h.hood < 0.1);
    }

    #[test]
    fn it_burrows_at_night_and_the_sand_hides_it() {
        let mut h = slithering(17);
        run(&mut h, 2.0, &drives());
        let mut sleepy = drives();
        sleepy.sleep = true;
        run(&mut h, 8.0, &sleepy);
        assert_eq!(h.state, HognoseState::Burrow);
        assert!(h.buried > 0.9, "not under: {}", h.buried);
        assert!(h.speed < 0.5, "moving while buried: {}", h.speed);
        // A fright underground is ignored.
        h.threaten(Some(Vec2::ZERO));
        assert_eq!(h.state, HognoseState::Burrow);
        // Morning: it comes back up.
        run(&mut h, 12.0, &drives());
        assert_ne!(h.state, HognoseState::Burrow);
        assert!(h.buried < 0.2, "still under: {}", h.buried);
    }

    #[test]
    fn the_tongue_flicks_while_it_moves() {
        let mut h = slithering(19);
        let mut max_t: f32 = 0.0;
        let mut min_t: f32 = 1.0;
        let dt = 1.0 / 60.0;
        for _ in 0..240 {
            h.step(dt, &drives(), &world());
            max_t = max_t.max(h.tongue);
            min_t = min_t.min(h.tongue);
        }
        assert!(max_t > 0.6, "never flicks: {max_t}");
        assert!(min_t < 0.2, "never withdraws: {min_t}");
    }

    #[test]
    fn it_stays_on_screen() {
        let mut h = Hognose::new(Vec2::new(720.0, 460.0), 23);
        h.heading = 0.6;
        run(&mut h, 40.0, &drives());
        assert!(h.position().x.abs() < 1512.0 / 2.0, "x {}", h.position().x);
        assert!(h.position().y.abs() < 982.0 / 2.0, "y {}", h.position().y);
    }

    #[test]
    fn proprioception_is_in_range() {
        let mut h = slithering(29);
        run(&mut h, 2.0, &drives());
        let p = h.proprioception();
        assert!((0.0..=1.0).contains(&p.drive));
        assert!((0.0..1.0).contains(&p.phase));
    }
}
