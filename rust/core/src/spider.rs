//! The jumping spider's body (SPIDER_PLAN.md §4).
//!
//! A salticid is not a fly with two more legs. It spends most of its time
//! **watching** — stationary, turning its cephalothorax to track things — and
//! moves in short walks, slow stalks and ballistic jumps. It has no wings, so
//! there is no flight and no altitude; there is a dragline instead, attached
//! before every jump and used to abseil off ledges. Its gait is an alternating
//! tetrapod: legs 1 and 3 on one side step with 2 and 4 on the other.
//!
//! Like the fly, every decision in `brain_behavior` reads a population rate
//! from the chimera circuit. Two readouts are worth naming because they are
//! where the invention lives:
//!
//! - **Head orientation** follows the LC11 left-minus-right rate. In the
//!   extract LC11 does not reach the steering DNs, so this readout is modelled,
//!   not wired (SPIDER_PLAN.md §3, third note). Body steering while walking
//!   still comes from DNa01/DNa02, which is measured.
//! - **The pounce** fires from the authored pounce node — the one neuron in
//!   the circuit that exists in no animal — gated by range to a target.
//!
//! Prey ("bugs") live here rather than in the shell so the closed loop —
//! bug → LC11 drive → circuit → pounce → capture — is testable end to end in
//! `suites::spider_behavior_test` with no window involved.

use crate::arachnid::{step_legs, LegMode};
use crate::creature::{Body, Proprioception, Substrate, World};
use crate::habitat::Region;
use crate::rng::Pcg32;
use crate::signals::BrainSignals;
use crate::silk::{Silk, ThreadKind};
use crate::util::{angle_diff, clamp, hypot, smoothstep, Ledge, Vec2};

pub const SPIDER_SCALE: f32 = 1.05;
pub const EDGE_MARGIN: f32 = 50.0;
/// A pounce is only attempted at a target closer than this (scene units).
pub const POUNCE_RANGE: f32 = 150.0;
/// A landing this close to a bug catches it.
pub const CAPTURE_RADIUS: f32 = 22.0;
/// How far the spider can see a small moving object.
pub const PREY_RANGE: f32 = 620.0;
/// Bugs drift off after this long, caught or not.
pub const BUG_LIFETIME: f32 = 45.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpiderState {
    /// Stationary, tracking with the head. The default and the characteristic one.
    Watching,
    Walking,
    /// Slow, low approach toward a fixated target.
    Stalking,
    /// Ballistic, dragline attached. Both the escape jump and the predatory pounce.
    Jumping,
    /// Hanging from a ledge on the line, descending then climbing back.
    Abseiling,
    Grooming,
    Sleeping,
}

/// A small moving thing the spider can see, and catch.
#[derive(Debug, Clone, Copy)]
pub struct Bug {
    pub pos: Vec2,
    pub vel: Vec2,
    pub age: f32,
}

pub use crate::arachnid::SpiderLeg;

/// Everything a renderer needs this frame.
#[derive(Debug, Clone)]
pub struct SpiderPose {
    pub pos: Vec2,
    pub heading: f32,
    /// Cephalothorax yaw relative to the heading.
    pub head_yaw: f32,
    pub z: f32,
    pub scale: f32,
    /// 0 = standing tall, 1 = flattened (stalking, or a threat nearby).
    pub crouch: f32,
    pub legs: [(f32, f32); 8],
    /// Where the dragline is anchored, if one is out.
    pub dragline: Option<Vec2>,
    pub abdomen_breathe: f32,
}

pub struct Spider {
    pub pos: Vec2,
    /// See `Fly::attractor`.
    pub attractor: Option<Vec2>,
    pub heading: f32,
    pub head_yaw: f32,
    pub speed: f32,
    pub state: SpiderState,
    pub state_timer: f32,
    pub state_age: f32,
    pub gait_phase: f32,
    pub time: f32,
    pub crouch: f32,

    pub pounce_cooldown: f32,
    pub escape_cooldown: f32,
    pub backward_timer: f32,
    /// Seconds of post-capture stillness.
    pub feed_timer: f32,

    pub terrain: Vec<Ledge>,
    pub ledge: Option<Ledge>,
    pub legs: [SpiderLeg; 8],

    // jump
    pub jump_from: Vec2,
    pub jump_to: Vec2,
    pub jump_t: f32,
    pub jump_dur: f32,
    pub jump_height: f32,
    pub jump_is_escape: bool,
    pub z: f32,
    /// The silk this spider has out. Today that is only ever the dragline,
    /// paid out before a jump or a descent and released on landing; the
    /// web builders share the same line (WEB_PLAN.md §4).
    pub silk: Silk,

    // abseil
    abseil_anchor: Vec2,
    abseil_len: f32,
    abseil_hang: f32,
    abseil_phase: u8,

    pub prey: Vec<Bug>,
    pub captured: u32,
    /// Which bug the head is fixated on, by index.
    pub target: Option<usize>,
    /// The user is in an editor or a terminal: sit tighter, look around more.
    /// A posture bias on the same command rates, not a new behaviour.
    pub settled: bool,

    live_nervous: f32,
    rng: Pcg32,
}

impl Spider {
    pub fn new(at: Vec2, seed: u64) -> Self {
        let mut rng = Pcg32::new(seed);
        let heading = rng.range(0.0, std::f32::consts::TAU);
        let legs = crate::arachnid::new_legs();
        Spider {
            attractor: None,
            pos: at,
            heading,
            head_yaw: 0.0,
            speed: 0.0,
            state: SpiderState::Watching,
            state_timer: rng.range(2.0, 5.0),
            state_age: 0.0,
            gait_phase: rng.range(0.0, 1.0),
            time: rng.range(0.0, 100.0),
            crouch: 0.0,
            pounce_cooldown: 0.0,
            escape_cooldown: 0.0,
            backward_timer: 0.0,
            feed_timer: 0.0,
            terrain: Vec::new(),
            ledge: None,
            legs,
            jump_from: Vec2::ZERO,
            jump_to: Vec2::ZERO,
            jump_t: 0.0,
            jump_dur: 0.3,
            jump_height: 0.0,
            jump_is_escape: false,
            z: 0.0,
            silk: Silk::new(),
            abseil_anchor: Vec2::ZERO,
            abseil_len: 0.0,
            abseil_hang: 0.0,
            abseil_phase: 0,
            prey: Vec::new(),
            captured: 0,
            target: None,
            settled: false,
            live_nervous: 0.0,
            rng,
        }
    }

    pub fn walking_intensity(&self) -> f32 {
        match self.state {
            SpiderState::Walking | SpiderState::Stalking => clamp(self.speed / 45.0, 0.0, 1.0),
            _ => 0.0,
        }
    }

    /// Where the dragline is anchored, if one is out.
    pub fn dragline(&self) -> Option<Vec2> {
        self.silk.trailing_anchor()
    }

    pub fn scale(&self) -> f32 {
        SPIDER_SCALE * (1.0 + 0.35 * clamp(self.z / 60.0, 0.0, 1.0))
    }

    pub fn pose(&self) -> SpiderPose {
        let breathe = if self.state == SpiderState::Sleeping {
            1.0 + 0.04 * (self.time * 1.0).sin()
        } else {
            1.0 + 0.025 * (self.time * 2.6).sin()
        };
        SpiderPose {
            pos: self.pos,
            heading: self.heading,
            head_yaw: self.head_yaw,
            z: self.z,
            scale: self.scale(),
            crouch: self.crouch,
            legs: std::array::from_fn(|i| (self.legs[i].angle, self.legs[i].lift)),
            dragline: self.silk.trailing_anchor(),
            abdomen_breathe: breathe,
        }
    }

    fn set_state(&mut self, s: SpiderState) {
        if s == self.state {
            return;
        }
        self.state = s;
        self.state_age = 0.0;
    }

    /// Put a bug into the world. The shell does this on a failed build; the
    /// suite does it directly.
    pub fn spawn_bug(&mut self, at: Vec2, vel: Vec2) {
        self.prey.push(Bug {
            pos: at,
            vel,
            age: 0.0,
        });
    }

    /// The facing direction the eyes look along: body heading plus head yaw.
    pub fn gaze(&self) -> f32 {
        self.heading + self.head_yaw
    }

    /// Small-object drive per eye from everything small and moving in view:
    /// the bugs, plus an optional extra object (the cursor, when the shell
    /// decides it is small enough to count).
    ///
    /// This is transduction, and therefore modelled: LC11's size selectivity
    /// arises in the lobula, outside the extract, so what reaches the
    /// population here is already filtered to *small, moving, in range*. What
    /// the population does with it is the wiring's business.
    pub fn prey_drive(&self, extra: Option<(Vec2, Vec2)>) -> (f32, f32) {
        let mut l: f32 = 0.0;
        let mut r: f32 = 0.0;
        let gaze = self.gaze();
        let f = Vec2::new(gaze.cos(), gaze.sin());
        let mut consider = |p: Vec2, v: Vec2| {
            let rel = Vec2::new(p.x - self.pos.x, p.y - self.pos.y);
            let d = hypot(rel.x, rel.y).max(8.0);
            if d > PREY_RANGE {
                return;
            }
            // Motion matters: LC11 is a motion detector, and a still bug is a
            // speck. Nearer is stronger, as a bigger retinal image.
            let motion = clamp(hypot(v.x, v.y) / 40.0, 0.25, 1.0);
            let drive = clamp(1.0 - d / PREY_RANGE, 0.0, 1.0).powf(0.6) * motion;
            let rd = Vec2::new(rel.x / d, rel.y / d);
            // Behind the animal is out of view, though salticids have nearly
            // 360° coverage from the lateral eyes; keep a weak rear response.
            let facing = clamp(0.35 + 0.65 * (f.x * rd.x + f.y * rd.y), 0.0, 1.0);
            let cross_z = f.x * rd.y - f.y * rd.x; // > 0: on the left
            let lw = clamp(0.5 + 0.5 * cross_z, 0.12, 1.0);
            let rw = clamp(0.5 - 0.5 * cross_z, 0.12, 1.0);
            l = l.max(drive * facing * lw);
            r = r.max(drive * facing * rw);
        };
        for b in &self.prey {
            consider(b.pos, b.vel);
        }
        if let Some((p, v)) = extra {
            consider(p, v);
        }
        (l, r)
    }

    fn nearest_bug(&self) -> Option<(usize, f32)> {
        self.prey
            .iter()
            .enumerate()
            .map(|(i, b)| (i, hypot(b.pos.x - self.pos.x, b.pos.y - self.pos.y)))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Attach the dragline here and jump. Both escape and pounce.
    pub fn start_jump(&mut self, to: Vec2, escape: bool) {
        self.set_state(SpiderState::Jumping);
        self.ledge = None;
        self.silk.pay_out(self.pos, ThreadKind::Dragline);
        self.jump_from = self.pos;
        self.jump_to = to;
        self.jump_t = 0.0;
        let dist = hypot(to.x - self.pos.x, to.y - self.pos.y);
        self.jump_dur = clamp(dist / 620.0, 0.16, 0.5);
        self.jump_height = clamp(dist * 0.32, 10.0, 60.0);
        self.jump_is_escape = escape;
        self.heading = (to.y - self.pos.y).atan2(to.x - self.pos.x);
        self.head_yaw = 0.0;
        self.crouch = 0.0;
        if escape {
            self.escape_cooldown = 1.5;
        } else {
            self.pounce_cooldown = 1.2;
        }
    }

    fn escape_target(&mut self, region: Region, away_from: Option<Vec2>) -> Vec2 {
        let (hw, hh) = region.half();
        let (hw, hh) = (hw - EDGE_MARGIN, hh - EDGE_MARGIN);
        let mut target = self.pos;
        for _ in 0..16 {
            let ang = self.rng.range(0.0, std::f32::consts::TAU);
            let dist = self.rng.range(120.0, 220.0);
            let t = Vec2::new(
                clamp(
                    self.pos.x + ang.cos() * dist,
                    region.center.x - hw,
                    region.center.x + hw,
                ),
                clamp(
                    self.pos.y + ang.sin() * dist,
                    region.center.y - hh,
                    region.center.y + hh,
                ),
            );
            if let Some(a) = away_from {
                let to_t = Vec2::new(t.x - self.pos.x, t.y - self.pos.y);
                let to_a = Vec2::new(a.x - self.pos.x, a.y - self.pos.y);
                if to_t.x * to_a.x + to_t.y * to_a.y > 0.0 {
                    continue;
                }
            }
            target = t;
            break;
        }
        target
    }

    fn land(&mut self) {
        self.pos = self.jump_to;
        self.z = 0.0;
        self.silk.release();
        // Did the pounce land on something?
        if !self.jump_is_escape {
            if let Some((i, d)) = self.nearest_bug() {
                if d < CAPTURE_RADIUS {
                    self.prey.remove(i);
                    self.captured += 1;
                    self.feed_timer = 2.5;
                    self.target = None;
                }
            }
        }
        // Settle through a crouch, never a snap.
        self.crouch = 1.0;
        self.set_state(SpiderState::Watching);
        self.state_timer = self.rng.range(1.5, 4.0);
        self.speed = 0.0;
    }

    fn start_abseil(&mut self, anchor: Vec2) {
        self.set_state(SpiderState::Abseiling);
        self.ledge = None;
        self.abseil_anchor = anchor;
        self.abseil_len = self.rng.range(60.0, 180.0);
        self.abseil_hang = self.rng.range(1.5, 4.0);
        self.abseil_phase = 0;
        self.silk.pay_out(anchor, ThreadKind::Dragline);
        self.pos = anchor;
        self.heading = -std::f32::consts::FRAC_PI_2;
        self.speed = 0.0;
    }

    pub fn update(
        &mut self,
        dt: f32,
        region: Region,
        mouse: Option<Vec2>,
        signals: Option<BrainSignals>,
    ) {
        self.time += dt;
        self.state_age += dt;
        self.pounce_cooldown = (self.pounce_cooldown - dt).max(0.0);
        self.escape_cooldown = (self.escape_cooldown - dt).max(0.0);
        self.backward_timer = (self.backward_timer - dt).max(0.0);
        self.feed_timer = (self.feed_timer - dt).max(0.0);
        self.live_nervous = signals.map(|s| s.nervous).unwrap_or(0.0);

        self.update_bugs(dt, region);

        match self.state {
            SpiderState::Jumping => self.update_jump(dt, signals.as_ref()),
            SpiderState::Abseiling => self.update_abseil(dt, region, mouse, signals.as_ref()),
            _ => {
                if let Some(s) = signals {
                    self.brain_behavior(&s, dt, region, mouse);
                } else {
                    self.brainless(dt);
                }
                if matches!(self.state, SpiderState::Walking | SpiderState::Stalking) {
                    self.update_walk(dt, region);
                }
            }
        }

        self.update_head(dt);
        self.update_posture(dt);
        self.update_legs(dt);
    }

    fn update_bugs(&mut self, dt: f32, region: Region) {
        let (hw, hh) = region.half();
        let (hw, hh) = (hw - 20.0, hh - 20.0);
        for b in self.prey.iter_mut() {
            b.age += dt;
            // A jittery drift, like a gnat.
            b.vel.x += self.rng.range(-1.0, 1.0) * 220.0 * dt;
            b.vel.y += self.rng.range(-1.0, 1.0) * 220.0 * dt;
            let sp = hypot(b.vel.x, b.vel.y);
            if sp > 70.0 {
                b.vel.x *= 70.0 / sp;
                b.vel.y *= 70.0 / sp;
            }
            b.pos.x += b.vel.x * dt;
            b.pos.y += b.vel.y * dt;
            if b.pos.x.abs() > hw {
                b.vel.x = -b.vel.x;
                b.pos.x = clamp(b.pos.x, -hw, hw);
            }
            if b.pos.y.abs() > hh {
                b.vel.y = -b.vel.y;
                b.pos.y = clamp(b.pos.y, -hh, hh);
            }
        }
        self.prey.retain(|b| b.age < BUG_LIFETIME);
        if let Some(t) = self.target {
            if t >= self.prey.len() {
                self.target = None;
            }
        }
    }

    /// Every decision here reads a real population's rate — or, for the
    /// pounce and the head, the one readout that is honestly modelled.
    fn brain_behavior(&mut self, s: &BrainSignals, dt: f32, region: Region, mouse: Option<Vec2>) {
        // Giant fiber spike -> escape jump, from any grounded state, even sleep.
        if s.escape && self.escape_cooldown == 0.0 {
            let to = self.escape_target(region, mouse);
            self.start_jump(to, true);
            return;
        }
        if s.sleep {
            if self.state != SpiderState::Sleeping {
                self.set_state(SpiderState::Sleeping);
                self.speed = 0.0;
                self.backward_timer = 0.0;
                self.target = None;
            }
            return;
        } else if self.state == SpiderState::Sleeping {
            self.set_state(SpiderState::Grooming);
            return;
        }

        // Looming detectors hot but no GF: freeze low and face it. A salticid
        // does not dart like a fly; it flattens and watches.
        if s.nervous > 0.4 {
            if let Some(m) = mouse {
                let to = (m.y - self.pos.y).atan2(m.x - self.pos.x);
                self.head_yaw = angle_diff(self.heading, to).clamp(-1.2, 1.2);
            }
            if self.state == SpiderState::Walking || self.state == SpiderState::Stalking {
                self.set_state(SpiderState::Watching);
                self.speed = 0.0;
            }
        }

        // LC11: fixate the nearest bug, orient the head by the L-R readout,
        // and stalk when the drive is sustained.
        if s.pursuit > 0.12 && self.feed_timer == 0.0 {
            if self.target.is_none() {
                self.target = self.nearest_bug().map(|(i, _)| i);
            }
            // Head follows the eye that sees more (modelled readout).
            self.head_yaw = clamp(self.head_yaw + s.prey_bias * 4.0 * dt, -1.3, 1.3);
            if s.pursuit > 0.3
                && self.state_age > 0.4
                && matches!(self.state, SpiderState::Watching | SpiderState::Walking | SpiderState::Grooming)
                && self.backward_timer == 0.0
            {
                self.set_state(SpiderState::Stalking);
                self.ledge = None;
            }
        } else if self.state == SpiderState::Stalking && s.pursuit < 0.08 && self.state_age > 0.6 {
            self.set_state(SpiderState::Watching);
            self.speed = 0.0;
            self.target = None;
        }

        // The authored pounce node: fires the jump, if something is in range.
        if s.pounce && self.pounce_cooldown == 0.0 && self.feed_timer == 0.0 {
            if let Some((i, d)) = self.nearest_bug() {
                if d < POUNCE_RANGE {
                    let b = self.prey[i];
                    let lead = clamp(d / 620.0, 0.16, 0.5);
                    let to = Vec2::new(b.pos.x + b.vel.x * lead, b.pos.y + b.vel.y * lead);
                    self.start_jump(to, false);
                    return;
                }
            }
            // Nothing in range: the node fired for nothing. A cooldown so it
            // does not spam, and a crouch so the intent still shows.
            self.pounce_cooldown = 0.6;
            self.crouch = 1.0;
        }

        // DNg11 grooming hysteresis, from stillness.
        if matches!(self.state, SpiderState::Watching | SpiderState::Grooming) {
            if self.state != SpiderState::Grooming
                && s.groom_drive > 0.5
                && s.nervous < 0.3
                && s.pursuit < 0.2
                && self.state_age > 0.4
            {
                self.set_state(SpiderState::Grooming);
            } else if self.state == SpiderState::Grooming && s.groom_drive < 0.3 && self.state_age > 0.6 {
                self.set_state(SpiderState::Watching);
            }
        }

        // DNp09 walking hysteresis. Watching is the rest state, and while the
        // user is working the bar to leave it is higher.
        let walk_on = if self.settled { 0.34 } else { 0.22 };
        if self.state == SpiderState::Watching && s.walk_drive > walk_on && s.pursuit < 0.3 && self.state_age > 0.4 && self.feed_timer == 0.0 {
            self.set_state(SpiderState::Walking);
            self.heading += self.rng.range(-0.8, 0.8);
        } else if self.state == SpiderState::Walking && s.walk_drive < 0.08 && self.state_age > 0.5 {
            self.set_state(SpiderState::Watching);
            self.speed = 0.0;
        }

        // MDN burst -> back away, from any grounded state.
        if s.backward && self.backward_timer == 0.0 {
            if !matches!(self.state, SpiderState::Walking | SpiderState::Stalking) {
                self.set_state(SpiderState::Walking);
                self.speed = 0.0;
            }
            self.backward_timer = 0.5;
        }

        // Speeds follow the forward command rate; stalking is the same
        // command at a crouch, so it is slower by posture, not by a new state
        // machine.
        match self.state {
            SpiderState::Walking => {
                if self.backward_timer == 0.0 {
                    let target = (10.0 + s.walk_drive * 45.0) * s.tempo;
                    self.speed += (target - self.speed) * (3.0 * dt).min(1.0);
                }
                if self.ledge.is_none() {
                    self.heading += s.turn_bias * dt; // DNa01/DNa02, measured
                }
            }
            SpiderState::Stalking => {
                let target = (6.0 + s.walk_drive * 16.0) * s.tempo;
                self.speed += (target - self.speed) * (3.0 * dt).min(1.0);
                // Body follows the head while stalking.
                self.heading += self.head_yaw * (2.5 * dt).min(1.0);
                self.head_yaw -= self.head_yaw * (2.5 * dt).min(1.0);
            }
            _ => {}
        }

        // Leaving a ledge downward on the line, occasionally.
        if self.state == SpiderState::Walking && self.ledge.is_some() && self.rng.f32() < 0.04 * dt {
            let anchor = self.pos;
            self.start_abseil(anchor);
        }
    }

    /// No brain: sit and watch, walk a little. Labelled brainless by the shell.
    fn brainless(&mut self, dt: f32) {
        self.state_timer -= dt;
        if self.state_timer <= 0.0 {
            match self.state {
                SpiderState::Watching => {
                    self.set_state(SpiderState::Walking);
                    self.speed = self.rng.range(15.0, 35.0);
                    self.heading += self.rng.range(-1.2, 1.2);
                    self.state_timer = self.rng.range(1.0, 3.0);
                }
                _ => {
                    self.set_state(SpiderState::Watching);
                    self.speed = 0.0;
                    self.state_timer = self.rng.range(2.0, 6.0);
                }
            }
        }
    }

    fn effective_speed(&self) -> f32 {
        if self.backward_timer > 0.0 {
            -18.0
        } else {
            self.speed
        }
    }

    fn update_walk(&mut self, dt: f32, region: Region) {
        if let Some(l) = self.ledge {
            match self.terrain.iter().find(|c| c.id == l.id && (c.y - l.y).abs() < 40.0) {
                Some(cur) => self.ledge = Some(*cur),
                None => {
                    // The window vanished underfoot: no wings, so drop on the
                    // line rather than fly — an abseil from where the ledge was.
                    let anchor = self.pos;
                    self.start_abseil(anchor);
                    return;
                }
            }
        }
        if let Some(l) = self.ledge {
            let along = if self.heading.cos() >= 0.0 { 0.0 } else { std::f32::consts::PI };
            self.heading += angle_diff(self.heading, along) * (6.0 * dt).min(1.0);
            let v = self.effective_speed();
            self.pos.x += self.heading.cos() * v * dt;
            self.pos.y += (l.y - self.pos.y) * (10.0 * dt).min(1.0);
            if self.pos.x <= l.x0 + 6.0 && self.heading.cos() < 0.0 {
                self.heading = 0.0;
            }
            if self.pos.x >= l.x1 - 6.0 && self.heading.cos() > 0.0 {
                self.heading = std::f32::consts::PI;
            }
            self.pos.x = clamp(self.pos.x, l.x0, l.x1);
            if self.rng.f32() < 0.04 * dt {
                self.ledge = None;
            }
        } else {
            if self.state == SpiderState::Walking {
                self.heading += self.rng.range(-1.0, 1.0) * 1.2 * dt;
            }
            if region.outside(self.pos, EDGE_MARGIN) {
                let to_center = region.bearing_home(self.pos);
                self.heading += angle_diff(self.heading, to_center) * (4.0 * dt).min(1.0);
            } else if let Some(a) = self.attractor {
                let to_a = (a.y - self.pos.y).atan2(a.x - self.pos.x);
                self.heading +=
                    angle_diff(self.heading, to_a) * (crate::body::CURIOSITY * dt).min(1.0);
            }
            let v = self.effective_speed();
            self.pos.x += self.heading.cos() * v * dt;
            self.pos.y += self.heading.sin() * v * dt;
            self.pos = region.clamp_inside(self.pos, 20.0);
            if self.state == SpiderState::Walking {
                let candidates: Vec<Ledge> = self
                    .terrain
                    .iter()
                    .copied()
                    .filter(|l| {
                        self.pos.x > l.x0 - 8.0 && self.pos.x < l.x1 + 8.0 && (self.pos.y - l.y).abs() < 20.0
                    })
                    .collect();
                for l in candidates {
                    if self.rng.f32() < 0.9 * dt {
                        self.ledge = Some(l);
                        self.heading = if self.heading.cos() >= 0.0 { 0.0 } else { std::f32::consts::PI };
                        break;
                    }
                }
            }
        }
        self.z = 0.25 * (self.gait_phase * std::f32::consts::TAU).sin().abs() * (1.0 - self.crouch);
    }

    fn update_jump(&mut self, dt: f32, signals: Option<&BrainSignals>) {
        // A GF spike mid-pounce redirects into an escape: the dragline is
        // already out, so the animal just changes where it lands.
        if let Some(s) = signals {
            if s.escape && !self.jump_is_escape && self.escape_cooldown == 0.0 {
                self.jump_is_escape = true;
                self.escape_cooldown = 1.5;
            }
        }
        self.jump_t = (self.jump_t + dt / self.jump_dur).min(1.0);
        let e = smoothstep(self.jump_t);
        self.pos.x = self.jump_from.x + (self.jump_to.x - self.jump_from.x) * e;
        self.pos.y = self.jump_from.y + (self.jump_to.y - self.jump_from.y) * e;
        // Ballistic arc: continuous to zero at both ends, so there is nothing
        // to snap at touchdown.
        self.z = self.jump_height * (self.jump_t * std::f32::consts::PI).sin();
        if self.jump_t >= 1.0 {
            self.land();
        }
    }

    fn update_abseil(&mut self, dt: f32, region: Region, mouse: Option<Vec2>, signals: Option<&BrainSignals>) {
        // A startle on the line: let go and drop, then jump clear on landing.
        if let Some(s) = signals {
            if s.escape && self.escape_cooldown == 0.0 {
                let to = self.escape_target(region, mouse);
                self.silk.pay_out(self.abseil_anchor, ThreadKind::Dragline);
                self.set_state(SpiderState::Jumping);
                self.jump_from = self.pos;
                self.jump_to = to;
                self.jump_t = 0.0;
                self.jump_dur = 0.4;
                self.jump_height = 20.0;
                self.jump_is_escape = true;
                self.escape_cooldown = 1.5;
                return;
            }
        }
        let bottom = self.abseil_anchor.y - self.abseil_len;
        match self.abseil_phase {
            0 => {
                self.pos.y -= 55.0 * dt;
                if self.pos.y <= bottom {
                    self.pos.y = bottom;
                    self.abseil_phase = 1;
                }
            }
            1 => {
                // Hang, turning slowly on the line.
                self.abseil_hang -= dt;
                self.heading += 0.6 * dt;
                self.pos.x = self.abseil_anchor.x + (self.time * 1.3).sin() * 3.0;
                if self.abseil_hang <= 0.0 {
                    self.abseil_phase = 2;
                }
            }
            _ => {
                self.pos.y += 40.0 * dt;
                self.pos.x += (self.abseil_anchor.x - self.pos.x) * (4.0 * dt).min(1.0);
                if self.pos.y >= self.abseil_anchor.y {
                    self.pos = self.abseil_anchor;
                    self.silk.release();
                    self.set_state(SpiderState::Watching);
                    self.state_timer = self.rng.range(1.0, 3.0);
                    // Back on the ledge, if it is still there.
                    self.ledge = self
                        .terrain
                        .iter()
                        .copied()
                        .find(|l| self.pos.x > l.x0 - 8.0 && self.pos.x < l.x1 + 8.0 && (self.pos.y - l.y).abs() < 20.0);
                    if self.ledge.is_some() {
                        self.heading = 0.0;
                    }
                }
            }
        }
        self.z = 0.0;
    }

    fn update_head(&mut self, dt: f32) {
        // With nothing to look at, the head drifts back to centre.
        let idle = self.target.is_none() && self.live_nervous < 0.4;
        if idle && self.state != SpiderState::Stalking {
            self.head_yaw -= self.head_yaw * (1.5 * dt).min(1.0);
        }
        // Occasional look-around while watching: the thing everyone notices.
        let look_rate = if self.settled { 0.45 } else { 0.25 };
        if self.state == SpiderState::Watching && idle && self.rng.f32() < look_rate * dt {
            self.head_yaw = self.rng.range(-0.9, 0.9);
        }
    }

    fn update_posture(&mut self, dt: f32) {
        let target = match self.state {
            SpiderState::Stalking => 0.8,
            SpiderState::Jumping => 0.0,
            SpiderState::Sleeping => 0.6,
            _ => {
                if self.live_nervous > 0.4 || self.feed_timer > 0.0 {
                    0.9
                } else {
                    0.0
                }
            }
        };
        let rate = if self.crouch > target { 2.5 } else { 8.0 };
        self.crouch += (target - self.crouch) * (rate * dt).min(1.0);
    }

    fn update_legs(&mut self, dt: f32) {
        // The rig is shared with the web builders (`arachnid.rs`); this only
        // says which of its modes the salticid's state is in.
        let v = self.effective_speed().abs();
        let walking = matches!(self.state, SpiderState::Walking | SpiderState::Stalking) && v > 1.0;
        let mode = if walking {
            LegMode::Walk {
                speed: v,
                backward: self.backward_timer > 0.0,
            }
        } else {
            match self.state {
                SpiderState::Grooming => LegMode::Groom { time: self.time },
                SpiderState::Jumping => LegMode::Launch,
                SpiderState::Abseiling => LegMode::Hang,
                _ => LegMode::Rest,
            }
        };
        step_legs(&mut self.legs, &mut self.gait_phase, dt, mode);
    }
}

impl Body for Spider {
    fn substrate(&self) -> Substrate {
        Substrate::WalkerJumper
    }
    fn step(&mut self, dt: f32, drives: &BrainSignals, world: &World) {
        self.terrain = world.ledges.clone();
        self.attractor = world.attractor;
        self.update(dt, world.region, world.cursor, Some(*drives));
    }
    fn position(&self) -> Vec2 {
        self.pos
    }
    fn heading(&self) -> f32 {
        self.heading
    }
    fn proprioception(&self) -> Proprioception {
        Proprioception {
            drive: self.walking_intensity(),
            phase: self.gait_phase,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::habitat::Region;

    const BOUNDS: Region = Region {
        center: Vec2::ZERO,
        size: (1512.0, 982.0),
    };
    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn the_gait_is_an_alternating_tetrapod() {
        let s = Spider::new(Vec2::ZERO, 1);
        // Legs 1 and 3 on the left step with 2 and 4 on the right.
        let phase = |side: f32, rank: usize| {
            s.legs.iter().find(|l| l.side == side && l.rank == rank).unwrap().phase
        };
        assert_eq!(phase(-1.0, 0), phase(-1.0, 2));
        assert_eq!(phase(-1.0, 0), phase(1.0, 1));
        assert_eq!(phase(-1.0, 0), phase(1.0, 3));
        assert_ne!(phase(-1.0, 0), phase(-1.0, 1));
        assert_eq!(s.legs.iter().filter(|l| l.phase == 0.0).count(), 4);
    }

    #[test]
    fn a_jump_attaches_a_dragline_and_lands_without_a_snap() {
        let mut s = Spider::new(Vec2::ZERO, 3);
        s.start_jump(Vec2::new(120.0, 40.0), true);
        assert_eq!(s.dragline(), Some(Vec2::ZERO));
        let (mut prev_z, mut max_dz, mut max_z) = (0.0f32, 0.0f32, 0.0f32);
        let mut frames = 0;
        while s.state == SpiderState::Jumping && frames < 200 {
            frames += 1;
            s.update(DT, BOUNDS, None, Some(BrainSignals::new()));
            max_dz = max_dz.max((s.z - prev_z).abs());
            max_z = max_z.max(s.z);
            prev_z = s.z;
        }
        assert_eq!(s.state, SpiderState::Watching);
        assert!(max_z > 10.0, "it should actually leave the ground: {max_z}");
        assert!(max_dz < 15.0, "z snapped by {max_dz} in one frame");
        assert!(s.dragline().is_none(), "the line is retracted on landing");
        assert!((s.pos.x - 120.0).abs() < 1e-3);
    }

    #[test]
    fn a_pounce_within_capture_radius_catches_the_bug() {
        let mut s = Spider::new(Vec2::ZERO, 5);
        s.spawn_bug(Vec2::new(60.0, 0.0), Vec2::ZERO);
        s.start_jump(Vec2::new(60.0, 0.0), false);
        for _ in 0..200 {
            if s.state != SpiderState::Jumping {
                break;
            }
            s.update(DT, BOUNDS, None, Some(BrainSignals::new()));
        }
        assert_eq!(s.captured, 1);
        assert!(s.prey.is_empty());
        assert!(s.feed_timer > 0.0);
    }

    #[test]
    fn prey_drive_is_lateralised_and_needs_a_moving_object() {
        let mut s = Spider::new(Vec2::ZERO, 9);
        s.heading = 0.0;
        s.spawn_bug(Vec2::new(120.0, 80.0), Vec2::new(30.0, 0.0));
        let (l, r) = s.prey_drive(None);
        assert!(l > r, "a bug on the left (positive y, heading 0) must drive the left eye: {l} vs {r}");
        assert!(l > 0.3);
        let mut still = Spider::new(Vec2::ZERO, 9);
        still.heading = 0.0;
        still.spawn_bug(Vec2::new(120.0, 80.0), Vec2::ZERO);
        let (l2, _) = still.prey_drive(None);
        assert!(l2 < l, "a still speck drives LC11 less than a moving one");
        let far = Spider::new(Vec2::ZERO, 9);
        assert_eq!(far.prey_drive(Some((Vec2::new(5000.0, 0.0), Vec2::new(50.0, 0.0)))), (0.0, 0.0));
    }

    #[test]
    fn abseil_descends_hangs_and_climbs_back_to_the_anchor() {
        let mut s = Spider::new(Vec2::new(0.0, 100.0), 11);
        s.start_abseil(Vec2::new(0.0, 100.0));
        let mut lowest = 100.0f32;
        let mut frames = 0;
        while s.state == SpiderState::Abseiling && frames < 60 * 30 {
            frames += 1;
            s.update(DT, BOUNDS, None, Some(BrainSignals::new()));
            lowest = lowest.min(s.pos.y);
            if s.state == SpiderState::Abseiling {
                assert!(s.dragline().is_some(), "the line stays attached while hanging");
            }
        }
        assert_eq!(s.state, SpiderState::Watching);
        assert!(lowest < 100.0 - 50.0, "it must actually descend: {lowest}");
        assert!((s.pos.y - 100.0).abs() < 1.0, "and come back: {}", s.pos.y);
        assert!(s.dragline().is_none());
    }

    #[test]
    fn bugs_expire() {
        let mut s = Spider::new(Vec2::ZERO, 2);
        s.spawn_bug(Vec2::new(100.0, 0.0), Vec2::ZERO);
        for _ in 0..((BUG_LIFETIME + 1.0) * 60.0) as usize {
            s.update(DT, BOUNDS, None, None);
        }
        assert!(s.prey.is_empty());
    }
}
