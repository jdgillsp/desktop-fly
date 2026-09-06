//! The fly's body: state machine, gait, flight, ledges, sleep.
//!
//! Port of `Fly` (FlyModel.swift:253-682) with one structural change: the Swift
//! version writes straight into SceneKit nodes, so behaviour and rendering are
//! entangled. Here the body produces a **pose** as plain data and a renderer
//! consumes it. Nothing in this file knows what a triangle is.
//!
//! That split is also what makes `Body` implementable by a second creature
//! later (PORT_PLAN.md §5) — a worm has no legs and no flight, but it still
//! produces a pose.

use crate::habitat::Region;
use crate::rng::Pcg32;
use crate::signals::BrainSignals;
use crate::util::{angle_diff, clamp, hypot, smoothstep, Ledge, Vec2};

pub const FLY_SCALE: f32 = 1.15;
pub const EDGE_MARGIN: f32 = 50.0;
/// Legacy distance-based behaviour, used only by the extra brainless flies.
pub const SCARE_RADIUS: f32 = 110.0;
pub const NERVOUS_RADIUS: f32 = 240.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Walking,
    Idle,
    Grooming,
    Flying,
    Sleeping,
}

/// One leg's animated state. Geometry (femur/tibia lengths, attachment) belongs
/// to the renderer; only the driven angles live here.
#[derive(Debug, Clone, Copy)]
pub struct Leg {
    pub swing_sign: f32,
    pub phase: f32,
    pub is_front: bool,
    pub angle: f32,
    pub lift: f32,
}

/// Tripod gait: legs {0,3,4} step together, {1,2,5} step together.
/// From the `specs` table at FlyModel.swift:204.
const LEG_SPECS: [(f32, f32, bool); 6] = [
    (1.0, 0.0, true),
    (-1.0, 0.5, true),
    (1.0, 0.5, false),
    (-1.0, 0.0, false),
    (1.0, 0.0, false),
    (-1.0, 0.5, false),
];

/// Everything a renderer needs to draw the body this frame.
#[derive(Debug, Clone, Copy)]
pub struct Pose {
    pub pos: Vec2,
    pub heading: f32,
    pub pitch: f32,
    /// Uniform scale — grows with altitude so height reads as "closer".
    pub scale: f32,
    /// Height above the desktop plane.
    pub z: f32,
    /// Euler angles for the two folded wings, `[left, right]`.
    pub wings: [[f32; 3]; 2],
    pub abdomen_breathe: f32,
    pub blur_wings_visible: bool,
}

/// How hard a creature turns toward something interesting in its enclosure,
/// relative to how hard it turns away from a wall (4.0 for the fly and the
/// spider). Deliberately weak, and deliberately *weaker than the wall reflex*:
/// a pet wedged in a corner because it is transfixed by a ball is worse than
/// one that wanders past the ball. Shared by all four bodies so curiosity does
/// not need re-tuning per animal.
pub const CURIOSITY: f32 = 1.0;

pub struct Fly {
    pub pos: Vec2,
    /// Something in the enclosure worth ambling toward; `None` in free roam.
    /// Set from `World` on every step rather than threaded through `update`,
    /// so the ground-truth suites call the same `update` they always did.
    pub attractor: Option<Vec2>,
    pub heading: f32,
    pub speed: f32,
    pub state: State,
    pub state_timer: f32,
    pub gait_phase: f32,
    pub time: f32,
    pub scare_cooldown: f32,
    pub dart_cooldown: f32,
    pub backward_timer: f32,
    pub dart_timer: f32,
    pub state_age: f32,

    /// Walkable window edges, refreshed by the platform layer.
    pub terrain: Vec<Ledge>,
    pub ledge: Option<Ledge>,

    pub legs: [Leg; 6],

    // flight
    pub flight_from: Vec2,
    pub flight_to: Vec2,
    pub flight_t: f32,
    pub flight_dur: f32,
    pub flight_effort: f32,
    pub effort_current: f32,
    pub alt: f32,
    pub pitch: f32,
    pub flap_phase: f32,
    pub wing_raise: f32,

    brain_live: bool,
    live_arousal: f32,
    live_wing: f32,

    // pose bits the renderer reads
    z: f32,
    wings: [[f32; 3]; 2],
    blur_wings_visible: bool,

    rng: Pcg32,
}

impl Fly {
    pub fn new(at: Vec2, seed: u64) -> Self {
        let mut rng = Pcg32::new(seed);
        let heading = rng.range(0.0, std::f32::consts::TAU);
        let state_timer = rng.range(1.5, 4.0);
        let gait_phase = rng.range(0.0, 1.0);
        let time = rng.range(0.0, 100.0);
        let legs = std::array::from_fn(|i| {
            let (swing_sign, phase, is_front) = LEG_SPECS[i];
            Leg {
                swing_sign,
                phase,
                is_front,
                angle: 0.0,
                lift: 0.0,
            }
        });
        Fly {
            attractor: None,
            pos: at,
            heading,
            speed: 30.0,
            state: State::Walking,
            state_timer,
            gait_phase,
            time,
            scare_cooldown: 0.0,
            dart_cooldown: 0.0,
            backward_timer: 0.0,
            dart_timer: 0.0,
            state_age: 0.0,
            terrain: Vec::new(),
            ledge: None,
            legs,
            flight_from: Vec2::ZERO,
            flight_to: Vec2::ZERO,
            flight_t: 0.0,
            flight_dur: 1.0,
            flight_effort: 0.6,
            effort_current: 0.6,
            alt: 0.0,
            pitch: 0.0,
            flap_phase: 0.0,
            wing_raise: 0.0,
            brain_live: false,
            live_arousal: 0.0,
            live_wing: 0.0,
            z: 0.0,
            // Wings start folded flat over the abdomen (FlyModel.swift:223).
            wings: [[0.0, 0.0, -0.13], [0.0, 0.0, 0.13]],
            blur_wings_visible: false,
            rng,
        }
    }

    pub fn walking_intensity(&self) -> f32 {
        if self.state == State::Walking {
            clamp(self.speed / 60.0, 0.0, 1.0)
        } else {
            0.0
        }
    }

    pub fn scale(&self) -> f32 {
        FLY_SCALE * (1.0 + 0.8 * self.alt)
    }

    pub fn pose(&self) -> Pose {
        let breathe = if self.state == State::Sleeping {
            1.0 + 0.05 * (self.time * 1.1).sin()
        } else {
            1.0 + 0.03 * (self.time * 3.0).sin()
        };
        Pose {
            pos: self.pos,
            heading: self.heading,
            pitch: self.pitch,
            scale: self.scale(),
            z: self.z,
            wings: self.wings,
            abdomen_breathe: breathe,
            blur_wings_visible: self.blur_wings_visible,
        }
    }

    fn set_state(&mut self, s: State) {
        if s == self.state {
            return;
        }
        self.state = s;
        self.state_age = 0.0;
    }

    pub fn start_flight(
        &mut self,
        region: Region,
        away_from: Option<Vec2>,
        escape: bool,
        effort: Option<f32>,
    ) {
        self.state = State::Flying;
        self.ledge = None;
        let base = effort.unwrap_or(if escape { 1.0 } else { self.rng.range(0.4, 0.75) });
        self.flight_effort = clamp(base, 0.25, 1.0);
        self.effort_current = self.flight_effort;
        self.flap_phase = 0.0;
        self.wing_raise = 0.0;
        self.flight_from = self.pos;

        let (hw, hh) = region.half();
        let (hw, hh) = (hw - EDGE_MARGIN, hh - EDGE_MARGIN);
        let mut target = region.center;
        let mut chosen = false;

        // Casual flights often land on a window edge.
        if !escape && away_from.is_none() && !self.terrain.is_empty() && self.rng.f32() < 0.45 {
            let k = self.rng.int_range(0, self.terrain.len() as i64 - 1) as usize;
            let l = self.terrain[k];
            if l.x1 - l.x0 > 90.0 {
                target = Vec2::new(self.rng.range(l.x0 + 25.0, l.x1 - 25.0), l.y);
                chosen = hypot(target.x - self.pos.x, target.y - self.pos.y) > 180.0;
            }
        }
        if !chosen {
            for _ in 0..16 {
                target = Vec2::new(
                    region.center.x + self.rng.range(-hw, hw),
                    region.center.y + self.rng.range(-hh, hh),
                );
                let far = hypot(target.x - self.pos.x, target.y - self.pos.y)
                    > if escape { 350.0 } else { 260.0 };
                if !far {
                    continue;
                }
                if let Some(a) = away_from {
                    // Escape must land on the far side of the fly from the threat.
                    let to_t = Vec2::new(target.x - self.pos.x, target.y - self.pos.y);
                    let to_a = Vec2::new(a.x - self.pos.x, a.y - self.pos.y);
                    if to_t.x * to_a.x + to_t.y * to_a.y > 0.0 {
                        continue;
                    }
                }
                break;
            }
        }
        self.flight_to = target;
        let dist = hypot(target.x - self.pos.x, target.y - self.pos.y);
        self.flight_dur = if escape {
            clamp(dist / 650.0, 0.45, 1.2)
        } else {
            clamp(dist / 420.0, 0.7, 2.0)
        };
        self.flight_t = 0.0;
        self.scare_cooldown = if escape { 2.0 } else { 2.5 };
        self.blur_wings_visible = true;
    }

    fn land(&mut self) {
        self.state = State::Idle;
        self.state_timer = self.rng.range(0.3, 0.8);
        self.speed = 0.0;
        self.alt = 0.0;
        self.pitch = 0.0;
        self.z = 0.0;
        // Refold the wings flat over the abdomen.
        self.wings = [[0.0, 0.0, -0.13], [0.0, 0.0, 0.13]];
        self.blur_wings_visible = false;
    }

    fn pick_next_state(&mut self) {
        match self.state {
            State::Walking => {
                let r = self.rng.f32();
                if r < 0.30 {
                    self.state = State::Idle;
                    self.state_timer = self.rng.range(0.8, 3.0);
                    self.speed = 0.0;
                } else if r < 0.55 {
                    self.state_timer = self.rng.range(0.3, 0.8);
                    self.speed = self.rng.range(95.0, 150.0);
                    self.heading += self.rng.range(-1.2, 1.2);
                } else {
                    self.state_timer = self.rng.range(1.5, 5.0);
                    self.speed = self.rng.range(18.0, 45.0);
                }
            }
            State::Idle => {
                let r = self.rng.f32();
                if r < 0.35 {
                    self.state = State::Grooming;
                    self.state_timer = self.rng.range(1.0, 2.5);
                } else {
                    self.state = State::Walking;
                    self.state_timer = self.rng.range(1.5, 5.0);
                    self.speed = self.rng.range(18.0, 45.0);
                    self.heading += self.rng.range(-1.5, 1.5);
                }
            }
            State::Grooming => {
                self.state = State::Idle;
                self.state_timer = self.rng.range(0.3, 1.0);
            }
            State::Flying | State::Sleeping => {}
        }
    }

    pub fn update(
        &mut self,
        dt: f32,
        region: Region,
        mouse: Option<Vec2>,
        signals: Option<BrainSignals>,
    ) {
        self.time += dt;
        self.scare_cooldown = (self.scare_cooldown - dt).max(0.0);
        self.dart_cooldown = (self.dart_cooldown - dt).max(0.0);
        self.backward_timer = (self.backward_timer - dt).max(0.0);
        self.state_age += dt;
        self.dart_timer = (self.dart_timer - dt).max(0.0);

        // Live brain drives reach the wings even mid-flight.
        self.brain_live = signals.is_some();
        self.live_arousal = signals.map(|s| s.arousal).unwrap_or(0.0);
        self.live_wing = signals.map(|s| s.wing_drive).unwrap_or(0.0);

        if self.state == State::Flying {
            self.update_flight(dt);
        } else if let Some(s) = signals {
            self.brain_behavior(&s, dt, region, mouse);
            if self.state == State::Walking {
                self.update_walk(dt, region);
            }
        } else {
            // Legacy distance-based fear, for the extra brainless flies.
            if self.scare_cooldown == 0.0 {
                if let Some(m) = mouse {
                    let d = hypot(m.x - self.pos.x, m.y - self.pos.y);
                    if d < SCARE_RADIUS {
                        self.start_flight(region, Some(m), false, None);
                    } else if d < NERVOUS_RADIUS && self.state != State::Walking {
                        self.set_state(State::Walking);
                        self.heading = (self.pos.y - m.y).atan2(self.pos.x - m.x)
                            + self.rng.range(-0.4, 0.4);
                        self.speed = self.rng.range(110.0, 150.0);
                        self.state_timer = self.rng.range(0.4, 0.9);
                        self.scare_cooldown = 1.0;
                    }
                }
            }
            if self.state != State::Flying {
                self.state_timer -= dt;
                if self.state_timer <= 0.0 {
                    if self.state == State::Walking && self.rng.f32() < 0.10 {
                        self.start_flight(region, None, false, None);
                    } else {
                        self.pick_next_state();
                    }
                }
                if self.state == State::Walking {
                    self.update_walk(dt, region);
                }
            }
        }

        self.update_legs(dt);
        self.update_wings(dt);
    }

    /// Every decision here reads a real neuron population's rate.
    fn brain_behavior(
        &mut self,
        s: &BrainSignals,
        dt: f32,
        region: Region,
        mouse: Option<Vec2>,
    ) {
        // Giant fiber spike -> escape takeoff (even out of sleep).
        if s.escape && self.scare_cooldown == 0.0 {
            self.start_flight(region, mouse, true, None);
            return;
        }
        // Circadian sleep: enter, hold, wake into grooming.
        if s.sleep {
            if self.state != State::Sleeping {
                self.set_state(State::Sleeping);
                self.speed = 0.0;
                self.dart_timer = 0.0;
                self.backward_timer = 0.0;
            }
            return;
        } else if self.state == State::Sleeping {
            self.set_state(State::Grooming); // flies groom after waking
            return;
        }
        // Looming detectors hot but GF quiet -> nervous dart away.
        if s.nervous > 0.40 && self.dart_cooldown == 0.0 {
            self.ledge = None;
            self.set_state(State::Walking);
            match mouse {
                Some(m) => {
                    self.heading =
                        (self.pos.y - m.y).atan2(self.pos.x - m.x) + self.rng.range(-0.4, 0.4)
                }
                None => self.heading += self.rng.range(-1.5, 1.5),
            }
            self.speed = self.rng.range(110.0, 155.0);
            self.dart_timer = self.rng.range(0.4, 0.9);
            self.dart_cooldown = 1.2;
        }
        // DNg11 (grooming command) hysteresis.
        if self.state != State::Walking || self.dart_timer == 0.0 {
            if self.state != State::Grooming
                && s.groom_drive > 0.5
                && s.nervous < 0.3
                && self.state_age > 0.4
            {
                self.set_state(State::Grooming);
            } else if self.state == State::Grooming && s.groom_drive < 0.3 && self.state_age > 0.6 {
                self.set_state(State::Idle);
            }
        }
        // DNp09 (forward-walking command) hysteresis.
        if self.state == State::Idle && s.walk_drive > 0.22 && self.state_age > 0.4 {
            self.set_state(State::Walking);
            self.heading += self.rng.range(-0.8, 0.8);
        } else if self.state == State::Walking
            && self.dart_timer == 0.0
            && s.walk_drive < 0.08
            && self.state_age > 0.5
        {
            self.set_state(State::Idle);
            self.speed = 0.0;
        }
        // MDN burst -> backward walk, from any grounded state.
        if s.backward && self.backward_timer == 0.0 && self.dart_timer == 0.0 {
            if self.state != State::Walking {
                self.set_state(State::Walking);
                self.speed = 0.0;
            }
            self.backward_timer = 0.5;
        }
        // Walking speed follows the forward command rate; tempo = temperature.
        if self.state == State::Walking {
            if self.dart_timer == 0.0 && self.backward_timer == 0.0 {
                let target = (14.0 + s.walk_drive * 55.0) * s.tempo;
                self.speed += (target - self.speed) * (3.0 * dt).min(1.0);
            }
            if self.ledge.is_none() {
                self.heading += s.turn_bias * dt; // DNa01/DNa02 steering
            }
        }
        // Spontaneous takeoff, gated on whole-population arousal.
        let flight_chance = if s.arousal > 0.5 { 0.6 } else { 0.005 };
        if self.state == State::Walking && self.rng.f32() < flight_chance * dt {
            self.start_flight(region, None, false, Some(0.35 + s.arousal * 0.6));
        }
    }

    fn effective_speed(&self) -> f32 {
        if self.backward_timer > 0.0 {
            -22.0
        } else {
            self.speed
        }
    }

    fn update_walk(&mut self, dt: f32, region: Region) {
        // Refresh the attached ledge — windows move and close.
        if let Some(l) = self.ledge {
            match self
                .terrain
                .iter()
                .find(|c| c.id == l.id && (c.y - l.y).abs() < 40.0)
            {
                Some(cur) => self.ledge = Some(*cur),
                None => {
                    self.ledge = None;
                    self.start_flight(region, None, false, None); // ground vanished
                    return;
                }
            }
        }

        if let Some(l) = self.ledge {
            // Walk along the window edge.
            self.heading += self.rng.range(-1.0, 1.0) * 0.2 * dt;
            let along = if self.heading.cos() >= 0.0 {
                0.0
            } else {
                std::f32::consts::PI
            };
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
            if self.rng.f32() < 0.05 * dt {
                self.ledge = None; // wander off the edge
            }
        } else {
            self.heading += self.rng.range(-1.0, 1.0) * 1.6 * dt;
            if region.outside(self.pos, EDGE_MARGIN) {
                let to_center = region.bearing_home(self.pos);
                self.heading += angle_diff(self.heading, to_center) * (4.0 * dt).min(1.0);
            } else if let Some(a) = self.attractor {
                let to_a = (a.y - self.pos.y).atan2(a.x - self.pos.x);
                self.heading += angle_diff(self.heading, to_a) * (CURIOSITY * dt).min(1.0);
            }
            let v = self.effective_speed();
            self.pos.x += self.heading.cos() * v * dt;
            self.pos.y += self.heading.sin() * v * dt;
            self.pos = region.clamp_inside(self.pos, 20.0);
            // Walked onto a window edge? Latch on.
            let candidates: Vec<Ledge> = self
                .terrain
                .iter()
                .copied()
                .filter(|l| {
                    self.pos.x > l.x0 - 8.0
                        && self.pos.x < l.x1 + 8.0
                        && (self.pos.y - l.y).abs() < 20.0
                })
                .collect();
            for l in candidates {
                if self.rng.f32() < 0.9 * dt {
                    self.ledge = Some(l);
                    self.heading = if self.heading.cos() >= 0.0 {
                        0.0
                    } else {
                        std::f32::consts::PI
                    };
                    break;
                }
            }
        }
        self.z = 0.35 * (self.gait_phase * std::f32::consts::TAU).sin().abs();
    }

    fn apply_altitude(&mut self) {
        self.z = 90.0 * self.alt;
    }

    fn update_flight(&mut self, dt: f32) {
        self.flight_t = (self.flight_t + dt / self.flight_dur).min(1.0);
        if self.flight_t >= 1.0 {
            // Touchdown flare: the timer ended, but the fly lands only once it
            // has actually descended. Never snap scale/z here — that regression
            // is called out in CLAUDE.md.
            self.pos.x = self.flight_to.x + (self.time * 26.0).sin() * 1.2;
            self.pos.y = self.flight_to.y + (self.time * 22.0).cos() * 1.0;
            self.pitch = clamp(self.alt * 0.4, 0.0, 0.35);
            self.alt += (0.0 - self.alt) * (9.0 * dt).min(1.0);
            self.apply_altitude();
            if self.alt < 0.035 {
                self.pos = self.flight_to;
                self.land();
            }
            return;
        }
        let e = smoothstep(self.flight_t);
        let dx = self.flight_to.x - self.flight_from.x;
        let dy = self.flight_to.y - self.flight_from.y;
        let len = hypot(dx, dy).max(1.0);
        let px = -dy / len;
        let py = dx / len;
        let wob = (self.time * 32.0).sin() * 4.0 * (self.flight_t * std::f32::consts::PI).sin();
        self.pos.x = self.flight_from.x + dx * e + px * wob;
        self.pos.y = self.flight_from.y + dy * e + py * wob;
        self.heading = dy.atan2(dx) + (self.time * 18.0).sin() * 0.12;

        // Effort stays live: ongoing escape-DN and arousal activity make the fly
        // beat harder and fly higher mid-flight. `max(flight_effort, ...)` is
        // load-bearing — a regression once halved escape altitude.
        self.effort_current = if self.brain_live {
            clamp(
                self.flight_effort.max(
                    self.flight_effort * 0.55 + self.live_arousal * 0.25 + self.live_wing * 0.6,
                ),
                0.25,
                1.3,
            )
        } else {
            self.flight_effort
        };
        let rise_env = (self.flight_t / 0.25).min(1.0);
        let fall_env = ((1.0 - self.flight_t) / 0.3).min(1.0);
        let target =
            self.effort_current * rise_env.min(fall_env) * (0.85 + 0.15 * (self.time * 7.0).sin());
        self.pitch = clamp((target - self.alt) * 2.5, -0.45, 0.45);
        self.alt += (target - self.alt) * (6.0 * dt).min(1.0);
        self.apply_altitude();
    }

    fn update_legs(&mut self, dt: f32) {
        let v = self.effective_speed().abs();
        let walking = self.state == State::Walking && v > 1.0;
        if walking {
            let amp = clamp(0.20 + v * 0.0022, 0.20, 0.50);
            let stride = (2.0 * amp * 13.0).max(5.0);
            let freq = clamp(v / stride, 3.0, 11.0);
            self.gait_phase = (self.gait_phase + freq * dt) % 1.0;
            let stance_frac = 0.6;
            let backward = self.backward_timer > 0.0;
            for leg in self.legs.iter_mut() {
                let p = (self.gait_phase + leg.phase) % 1.0;
                if p < stance_frac {
                    leg.angle = amp * (1.0 - 2.0 * (p / stance_frac));
                    leg.lift = 0.0;
                } else {
                    let s = (p - stance_frac) / (1.0 - stance_frac);
                    leg.angle = -amp + 2.0 * amp * smoothstep(s);
                    leg.lift = (s * std::f32::consts::PI).sin() * 0.55;
                }
                if backward {
                    leg.angle = -leg.angle;
                }
            }
        } else if self.state == State::Grooming {
            let t = self.time;
            for leg in self.legs.iter_mut() {
                if leg.is_front {
                    leg.angle = 0.45 + 0.25 * (t * 20.0 + leg.swing_sign * 1.3).sin();
                    leg.lift = 0.55 + 0.15 * (t * 22.0).sin();
                } else {
                    leg.angle += (0.0 - leg.angle) * (8.0 * dt).min(1.0);
                    leg.lift += (0.0 - leg.lift) * (8.0 * dt).min(1.0);
                }
            }
        } else if self.state == State::Flying {
            for leg in self.legs.iter_mut() {
                leg.angle += (-0.35 - leg.angle) * (6.0 * dt).min(1.0);
                leg.lift += (0.5 - leg.lift) * (6.0 * dt).min(1.0);
            }
        } else {
            for leg in self.legs.iter_mut() {
                leg.angle += (0.0 - leg.angle) * (10.0 * dt).min(1.0);
                leg.lift += (0.0 - leg.lift) * (10.0 * dt).min(1.0);
            }
        }
    }

    fn update_wings(&mut self, dt: f32) {
        if self.state != State::Flying {
            // Grounded threat posture: escape-DN or loom activity raises the wings.
            let raise_target = if self.state != State::Sleeping
                && (self.live_wing > 0.7 || (self.brain_live && self.dart_timer > 0.0))
            {
                1.0
            } else {
                0.0
            };
            self.wing_raise += (raise_target - self.wing_raise) * (8.0 * dt).min(1.0);
            if self.wing_raise > 0.01 {
                for i in 0..2 {
                    let side: f32 = if i == 0 { -1.0 } else { 1.0 };
                    self.wings[i] = [
                        -0.5 * self.wing_raise,
                        0.0,
                        side * (0.13 + 0.3 * self.wing_raise),
                    ];
                }
            }
            return;
        }
        // Visible wing-beat: faster when live effort is higher.
        self.flap_phase = (self.flap_phase + dt * (14.0 + 10.0 * self.effort_current)) % 1.0;
        let stroke = (self.flap_phase * std::f32::consts::TAU).sin();
        for i in 0..2 {
            let side: f32 = if i == 0 { -1.0 } else { 1.0 };
            self.wings[i] = [
                stroke * 0.35,
                0.0,
                side * (0.45 + 0.35 * (0.5 + 0.5 * stroke)),
            ];
        }
    }
}
