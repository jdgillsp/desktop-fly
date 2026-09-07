//! The sandworm's body: a burrower, from a novel.
//!
//! Creature #9. Shai-Hulud is Frank Herbert's, not biology's, so there is not
//! merely *no connectome* — there is no animal. That makes it the most
//! clearly procedural creature in the set, and the label says so: it is
//! [`Provenance::Procedural`](crate::creature::Provenance::Procedural) with a
//! `why` that names the book. Nothing about it pretends to be measured. See
//! DESERT_PLAN.md.
//!
//! ## What the model keeps from the fiction
//!
//! * **It lives under the sand.** Most of the time nothing is visible but a
//!   moving ripple. [`Sandworm::surface`] is how much of the body is up.
//! * **It comes to rhythm.** A thumper is a rhythmic pounding staked into the
//!   sand; the worm homes on it. The runtime turns regular clicks into that
//!   signal and hands it over as `pursuit` plus the world's attractor, and
//!   the body goes to it and *breaches* there: rears, gapes, then settles.
//!   In the books the worm **swallows** the thumper, and the pounding stops.
//!   [`Sandworm::swallowed`] is raised at the moment of the breach over a
//!   lure; the runtime reads it and forgets the beats, so a new call is
//!   needed for the next visit. Fremen also know that *regular* footsteps
//!   call a worm and walk without rhythm to avoid it — so a cursor moving
//!   at a steady pace is a weak lure too, and an erratic one is nothing.
//! * **It is not afraid of anything.** No startle. The only thing that sends
//!   it down is the tray's escape test (a hard `escape`), which it obeys
//!   because every runtime must.
//!
//! Locomotion is a straight-running crawl with a faint lateral wave and a
//! peristaltic ring wave along the body ([`Sandworm::ring_phase`]), which is
//! what the rendering uses to make the segments seem to grip and release. The
//! centreline follows the head's recorded path, as every long body here does.

use crate::creature::{Body, Proprioception, Substrate, World};
use crate::rng::Pcg32;
use crate::signals::BrainSignals;
use crate::util::{angle_diff, clamp, hypot, Vec2};

pub const SEGMENTS: usize = 30;
pub const SEGMENT_LEN: f32 = 3.4;
pub const BODY_LEN: f32 = SEGMENT_LEN * (SEGMENTS as f32 - 1.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandwormState {
    /// Under the sand, moving. A ripple.
    Submerged,
    /// Under the sand, still. Deep.
    Idle,
    /// Coming up and rearing: the head rises and the mouth opens.
    Breach,
    /// On the surface, moving slowly.
    Cruise,
    /// Going back under.
    Dive,
}

pub struct Sandworm {
    pub pos: Vec2,
    pub heading: f32,
    pub state: SandwormState,
    /// Lateral wave phase, 0..1.
    pub phase: f32,
    /// Peristaltic ring wave phase, 0..1. Runs whenever the body moves.
    pub ring_phase: f32,
    pub spine: Vec<Vec2>,
    pub speed: f32,
    /// How much of the body is above the sand, 0..1.
    pub surface: f32,
    /// How far the head is reared, 0..1.
    pub rear: f32,
    /// How far the mouth is open, 0..1.
    pub gape: f32,
    pub state_timer: f32,
    pending_turn: f32,
    /// Seconds until it is allowed another spontaneous breach.
    breach_cooldown: f32,
    /// Where it was called to, if anywhere. Cleared when reached.
    pub lure: Option<Vec2>,
    /// Raised for one step when the worm breaches over its lure: it has
    /// eaten the thumper. The runtime clears the rhythm on seeing it.
    pub swallowed: bool,
    pub time: f32,

    path: Vec<Vec2>,
    arc: Vec<f32>,
    head_s: f32,

    rng: Pcg32,
}

impl Sandworm {
    pub fn new(at: Vec2, seed: u64) -> Self {
        let mut rng = Pcg32::new(seed);
        let heading = rng.range(0.0, std::f32::consts::TAU);
        let start = BODY_LEN * 1.5;
        let tail = Vec2::new(
            at.x - heading.cos() * start,
            at.y - heading.sin() * start,
        );
        let mut w = Sandworm {
            pos: at,
            heading,
            state: SandwormState::Submerged,
            phase: rng.range(0.0, 1.0),
            ring_phase: 0.0,
            spine: vec![at; SEGMENTS],
            speed: 24.0,
            surface: 0.0,
            rear: 0.0,
            gape: 0.0,
            state_timer: rng.range(6.0, 12.0),
            pending_turn: 0.0,
            breach_cooldown: rng.range(10.0, 25.0),
            lure: None,
            swallowed: false,
            time: rng.range(0.0, 100.0),
            path: vec![tail, at],
            arc: vec![0.0, start],
            head_s: start,
            rng,
        };
        w.resample_spine();
        w
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
        // A long, lazy wave: the body mostly runs straight.
        const WAVELENGTH: f32 = 1.4;
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
            let offset = 1.4 * travelling * (0.3 + 0.7 * s);
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

    /// Come up, rear and gape. From under the sand or from the surface.
    pub fn breach(&mut self) {
        if self.state == SandwormState::Breach {
            return;
        }
        self.state = SandwormState::Breach;
        self.state_timer = self.rng.range(2.2, 3.4);
        self.breach_cooldown = self.rng.range(18.0, 40.0);
    }

    /// Go under. The escape test, and the end of a surface cruise.
    pub fn dive(&mut self) {
        if matches!(self.state, SandwormState::Submerged | SandwormState::Idle) {
            return;
        }
        self.state = SandwormState::Dive;
        self.state_timer = 2.5;
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

impl Body for Sandworm {
    fn substrate(&self) -> Substrate {
        Substrate::Burrower
    }

    fn step(&mut self, dt: f32, drives: &BrainSignals, world: &World) {
        self.time += dt;
        self.state_timer -= dt;
        self.breach_cooldown = (self.breach_cooldown - dt).max(0.0);
        self.swallowed = false;

        if drives.escape {
            self.dive();
        }

        // A thumper. `pursuit` is the rhythm's strength; the attractor is
        // where it is. Only while it is going: a worm that kept heading for
        // where a thumper *was* would be remembering, and the fiction is that
        // it follows the sound.
        if drives.pursuit > 0.2 {
            if let Some(a) = world.attractor {
                self.lure = Some(a);
                if self.state == SandwormState::Idle {
                    self.state = SandwormState::Submerged;
                    self.state_timer = 10.0;
                }
            }
        } else {
            self.lure = None;
        }

        if drives.sleep && matches!(self.state, SandwormState::Submerged | SandwormState::Cruise) {
            self.state = SandwormState::Idle;
            self.state_timer = 4.0;
        }

        if self.state_timer <= 0.0 {
            match self.state {
                SandwormState::Breach => {
                    self.state = SandwormState::Cruise;
                    self.state_timer = self.rng.range(5.0, 10.0);
                }
                SandwormState::Cruise => {
                    self.dive();
                }
                SandwormState::Dive => {
                    self.state = SandwormState::Submerged;
                    self.state_timer = self.rng.range(8.0, 16.0);
                    self.pending_turn = self.rng.range(-1.0, 1.0);
                }
                SandwormState::Idle => {
                    if drives.sleep {
                        self.state_timer = 4.0;
                    } else {
                        self.state = SandwormState::Submerged;
                        self.state_timer = self.rng.range(8.0, 16.0);
                    }
                }
                SandwormState::Submerged => {
                    let r = self.rng.f32();
                    if r < 0.25 {
                        self.state = SandwormState::Idle;
                        self.state_timer = self.rng.range(4.0, 9.0);
                    } else if r < 0.45 && self.breach_cooldown <= 0.0 {
                        // Now and then it surfaces for no reason anyone on
                        // the surface can see.
                        self.breach();
                    } else {
                        self.state_timer = self.rng.range(6.0, 14.0);
                        self.pending_turn = self.rng.range(-1.2, 1.2);
                    }
                }
            }
        }

        // --- the lure: go to it, and breach on arrival.
        if let Some(l) = self.lure {
            let d = self.pos.dist(l);
            if d < 30.0 {
                if !matches!(self.state, SandwormState::Breach | SandwormState::Cruise) {
                    self.breach();
                    self.swallowed = true;
                    self.lure = None;
                }
            } else if matches!(self.state, SandwormState::Submerged | SandwormState::Cruise) {
                let to = (l.y - self.pos.y).atan2(l.x - self.pos.x);
                self.pending_turn = angle_diff(self.heading, to);
            }
        }

        // --- what is visible.
        let (surf_t, rear_t, gape_t) = match self.state {
            SandwormState::Submerged | SandwormState::Idle => (0.0, 0.0, 0.0),
            SandwormState::Breach => {
                // Up, then over: the rear peaks in the first half of the
                // breach and the mouth with it.
                let t = 1.0 - (self.state_timer / 3.0).clamp(0.0, 1.0);
                let peak = if t < 0.5 { 1.0 } else { 1.0 - (t - 0.5) * 2.0 };
                (1.0, peak, peak)
            }
            SandwormState::Cruise => (1.0, 0.0, 0.15),
            SandwormState::Dive => (0.0, 0.0, 0.0),
        };
        let surf_rate = if surf_t > self.surface { 1.6 } else { 0.9 };
        self.surface += (surf_t - self.surface) * (surf_rate * dt).min(1.0);
        self.rear += (rear_t - self.rear) * (3.0 * dt).min(1.0);
        self.gape += (gape_t - self.gape) * (4.0 * dt).min(1.0);

        // --- locomotion.
        let target = match self.state {
            SandwormState::Submerged => {
                if self.lure.is_some() { 60.0 } else { 26.0 + drives.walk_drive * 10.0 }
            }
            SandwormState::Idle => 0.0,
            SandwormState::Breach => 4.0,
            SandwormState::Cruise => 12.0,
            SandwormState::Dive => 18.0,
        } * drives.tempo;
        self.speed += (target - self.speed) * (1.5 * dt).min(1.0);
        self.speed = self.speed.max(0.0);

        let turn_rate = if self.lure.is_some() { 2.0 } else { 0.7 };
        self.take_turn(dt, turn_rate);
        self.heading += drives.turn_bias * 0.2 * dt;

        // The lateral wave is slow; the ring wave runs with the speed.
        self.phase = (self.phase + (0.05 + self.speed / 90.0) * dt) % 1.0;
        self.ring_phase = (self.ring_phase + (self.speed / 12.0) * dt) % 1.0;

        let step = self.speed * dt;
        if step > 0.0 {
            self.advance(step);
        }

        if world.region.outside(self.pos, 50.0) {
            let home = world.region.bearing_home(self.pos);
            self.heading += angle_diff(self.heading, home) * (1.8 * dt).min(1.0);
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
            phase: self.ring_phase,
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

    fn run(w: &mut Sandworm, secs: f32, d: &BrainSignals, world: &World) {
        let dt = 1.0 / 60.0;
        for _ in 0..((secs / dt) as u32) {
            w.step(dt, d, world);
        }
    }

    fn submerged(seed: u64) -> Sandworm {
        let mut w = Sandworm::new(Vec2::ZERO, seed);
        w.state = SandwormState::Submerged;
        w.state_timer = 60.0;
        w.breach_cooldown = 1000.0;
        w
    }

    #[test]
    fn a_sandworm_is_a_burrower() {
        assert_eq!(Sandworm::new(Vec2::ZERO, 1).substrate(), Substrate::Burrower);
    }

    #[test]
    fn it_moves_under_the_sand_and_stays_under() {
        let mut w = submerged(1);
        let start = w.position();
        let mut max_surface: f32 = 0.0;
        for _ in 0..360 {
            w.step(1.0 / 60.0, &drives(), &world());
            max_surface = max_surface.max(w.surface);
        }
        assert!(w.position().dist(start) > 60.0, "barely moved");
        assert!(max_surface < 0.05, "surfaced for no reason: {max_surface}");
    }

    #[test]
    fn the_body_does_not_stretch() {
        let mut w = submerged(3);
        run(&mut w, 8.0, &drives(), &world());
        for i in 1..SEGMENTS {
            let a = w.sample(w.head_s - SEGMENT_LEN * (i - 1) as f32);
            let b = w.sample(w.head_s - SEGMENT_LEN * i as f32);
            let d = a.dist(b);
            assert!((d - SEGMENT_LEN).abs() < 0.05, "segment {i} is {d}");
        }
    }

    /// The thumper. Given a rhythm and a place, the worm goes there and
    /// breaches on arrival — rears, gapes, then settles on the surface.
    #[test]
    fn a_thumper_draws_it_in_and_it_breaches_there() {
        let mut w = submerged(5);
        w.heading = std::f32::consts::PI; // facing away from the lure
        let lure = Vec2::new(260.0, 40.0);
        let mut wd = world();
        wd.attractor = Some(lure);
        let mut d = drives();
        d.pursuit = 1.0;
        let start_d = w.pos.dist(lure);
        let dt = 1.0 / 60.0;
        let mut breached = false;
        let mut max_rear: f32 = 0.0;
        let mut max_gape: f32 = 0.0;
        let mut nearest = f32::MAX;
        for _ in 0..(25.0 / dt) as u32 {
            w.step(dt, &d, &wd);
            nearest = nearest.min(w.pos.dist(lure));
            breached |= w.state == SandwormState::Breach;
            max_rear = max_rear.max(w.rear);
            max_gape = max_gape.max(w.gape);
        }
        assert!(nearest < start_d * 0.3, "never approached: nearest {nearest:.0} of {start_d:.0}");
        assert!(breached, "reached the thumper without breaching");
        assert!(max_rear > 0.7, "no rear: {max_rear}");
        assert!(max_gape > 0.7, "no gape: {max_gape}");
        assert!(w.surface > 0.5 || w.state == SandwormState::Dive, "never surfaced");
    }

    /// Reaching the thumper eats it: the body says so once, and drops the
    /// lure on its own even if the runtime kept the rhythm alive.
    #[test]
    fn breaching_over_the_thumper_swallows_it() {
        let mut w = submerged(23);
        let lure = Vec2::new(20.0, 0.0);
        w.heading = 0.0;
        let mut wd = world();
        wd.attractor = Some(lure);
        let mut d = drives();
        d.pursuit = 1.0;
        let mut ate = 0;
        for _ in 0..600 {
            w.step(1.0 / 60.0, &d, &wd);
            if w.swallowed {
                ate += 1;
            }
        }
        assert_eq!(ate, 1, "swallowed {ate} times");
    }

    /// And when the rhythm stops, so does the interest.
    #[test]
    fn a_silent_thumper_is_forgotten() {
        let mut w = submerged(7);
        let mut wd = world();
        wd.attractor = Some(Vec2::new(400.0, 0.0));
        let mut d = drives();
        d.pursuit = 1.0;
        run(&mut w, 1.0, &d, &wd);
        assert!(w.lure.is_some());
        d.pursuit = 0.0;
        run(&mut w, 0.5, &d, &wd);
        assert!(w.lure.is_none(), "still heading for a dead thumper");
    }

    #[test]
    fn a_breach_ends_with_a_dive() {
        let mut w = submerged(11);
        w.breach();
        assert_eq!(w.state, SandwormState::Breach);
        run(&mut w, 2.0, &drives(), &world());
        assert!(w.surface > 0.8, "not up: {}", w.surface);
        run(&mut w, 20.0, &drives(), &world());
        assert!(
            matches!(w.state, SandwormState::Submerged | SandwormState::Idle),
            "still up after a long time: {:?}",
            w.state
        );
        assert!(w.surface < 0.1, "still visible: {}", w.surface);
    }

    /// The escape test sends it under, and nothing else frightens it.
    #[test]
    fn escape_sends_it_under_and_it_is_otherwise_fearless() {
        let mut w = submerged(13);
        w.breach();
        run(&mut w, 3.0, &drives(), &world());
        assert_eq!(w.state, SandwormState::Cruise);
        let mut nervous = drives();
        nervous.nervous = 1.0;
        run(&mut w, 1.0, &nervous, &world());
        assert_eq!(w.state, SandwormState::Cruise, "a worm does not get nervous");
        let mut esc = drives();
        esc.escape = true;
        w.step(1.0 / 60.0, &esc, &world());
        assert_eq!(w.state, SandwormState::Dive);
    }

    #[test]
    fn it_stays_on_screen() {
        let mut w = Sandworm::new(Vec2::new(700.0, 440.0), 17);
        w.heading = 0.4;
        run(&mut w, 40.0, &drives(), &world());
        assert!(w.position().x.abs() < 1512.0 / 2.0, "x {}", w.position().x);
        assert!(w.position().y.abs() < 982.0 / 2.0, "y {}", w.position().y);
    }

    #[test]
    fn the_rings_run_only_when_it_moves() {
        let mut w = submerged(19);
        let a = w.ring_phase;
        run(&mut w, 1.0, &drives(), &world());
        assert_ne!(a, w.ring_phase, "the rings should travel while it crawls");
        w.state = SandwormState::Idle;
        w.state_timer = 60.0;
        run(&mut w, 6.0, &drives(), &world());
        let b = w.ring_phase;
        run(&mut w, 1.0, &drives(), &world());
        assert!((b - w.ring_phase).abs() < 0.02, "rings running while idle");
    }
}
