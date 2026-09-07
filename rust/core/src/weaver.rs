//! The web builders' body (WEB_PLAN.md §4, §5, §7).
//!
//! One chassis for three species. An orb weaver, a gumfoot-tangle weaver and
//! a sheet-and-funnel weaver share the eight-leg rig, the silk, the prey, the
//! drop-on-a-dragline and the brain-driven state machine here; what differs
//! is the **construction program** each runs ([`WebProgram`]) and a few
//! species constants ([`Traits`]).
//!
//! The program is procedural and labelled so: it has no neurons in it. The
//! seam with the brain is one-directional and narrow, and it is the thing the
//! honesty rule in WEB_PLAN.md §3.2 turns into a test:
//!
//! - The brain decides **whether** the animal moves (DNp09 walk drive gates
//!   construction), whether it flees (a giant-fiber spike), grooms, backs up,
//!   sleeps, and — through the authored strike node — whether something in
//!   the web is worth going to.
//! - The program decides **where the next thread goes**. It reads the silk
//!   and the region and returns [`Move`]s; the body walks them and performs
//!   the silk operation on arrival. It never touches the simulation.
//!
//! Prey localisation is a modelled readout: the loudest node of the silk is
//! where the spider goes, in the same category as the salticid's head-orient
//! readout of LC11 (SPIDER_PLAN.md §3).

use crate::arachnid::{new_legs, step_legs, LegMode, SpiderLeg};
use crate::creature::{Body, Proprioception, Substrate, Weaver as Species, World};
use crate::habitat::Region;
use crate::rng::Pcg32;
use crate::signals::BrainSignals;
use crate::silk::{Anchor, Silk, ThreadKind};
use crate::util::{angle_diff, clamp, hypot, Ledge, Vec2};

pub const WEAVER_SCALE: f32 = 1.0;
pub const EDGE_MARGIN: f32 = 30.0;
/// Walking speed while laying thread, scene units per second. This is the
/// tempo compression of WEB_PLAN.md §5: an orb weaver builds for about an
/// hour; at this speed the same path takes a few minutes. One constant.
pub const BUILD_SPEED: f32 = 150.0;
/// A landing this close to a stuck bug catches it.
pub const CAPTURE_RADIUS: f32 = 18.0;
/// Bugs are gone after this long, caught or not.
pub const BUG_LIFETIME: f32 = 120.0;
/// A bug touching a sticky thread closer than this is stuck.
pub const STICK_RADIUS: f32 = 4.0;
/// Local-hour window in which a daily rebuilder tears its web down.
pub const DUSK: (f32, f32) = (19.0, 22.0);
/// A web older than this at dusk gets rebuilt (seconds of body time).
pub const REBUILD_AGE: f32 = 6.0 * 3600.0;
/// In free roam a web does not span the whole display: it hangs in a box
/// this size, under a window's top edge when one is wide enough.
pub const FREE_ROAM_WEB: (f32, f32) = (560.0, 440.0);
/// A region no bigger than this is an enclosure and is used whole.
pub const ENCLOSURE_MAX: f32 = 1000.0;
/// A ledge must be at least this wide to hang a web from.
pub const LEDGE_MIN_WIDTH: f32 = 220.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaverState {
    /// Waiting for prey at the sit point: the hub, the retreat, the funnel.
    Sitting,
    /// Walking a construction move.
    Building,
    /// A brain-driven wander over the web, or the walk back to the sit point.
    Walking,
    /// Going to where the silk is loudest.
    Hunting,
    /// At the prey, wrapping.
    Wrapping,
    /// Back to the sit point with (or without) the catch.
    Carrying,
    /// Still, after a catch.
    Feeding,
    /// Falling on the dragline after a giant-fiber spike.
    Dropping,
    /// Hanging on the line.
    Hanging,
    /// Climbing back up the line.
    Climbing,
    /// Running for the retreat (funnel weavers do this instead of dropping).
    Retreating,
    Grooming,
    Sleeping,
    /// Taking the old web down before a rebuild.
    Eating,
}

/// What a giant-fiber spike makes this species do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreatResponse {
    /// Drop from the web on the dragline, hang, climb back.
    Drop,
    /// Run into the retreat.
    Retreat,
}

/// Species constants the shared chassis reads.
#[derive(Debug, Clone, Copy)]
pub struct Traits {
    /// Speed of the run to prey.
    pub hunt_speed: f32,
    pub threat: ThreatResponse,
    /// Heading held at the sit point, if the species has a characteristic
    /// one (an orb weaver sits head-down at the hub).
    pub sit_heading: Option<f32>,
}

impl Traits {
    pub fn of(species: Species) -> Traits {
        match species {
            Species::Araneus => Traits {
                hunt_speed: 220.0,
                threat: ThreatResponse::Drop,
                sit_heading: Some(-std::f32::consts::FRAC_PI_2),
            },
            Species::Parasteatoda => Traits {
                hunt_speed: 170.0,
                threat: ThreatResponse::Drop,
                sit_heading: None,
            },
            Species::Agelenopsis => Traits {
                // The fastest thing in the app: the rush from the funnel.
                hunt_speed: 460.0,
                threat: ThreatResponse::Retreat,
                sit_heading: None,
            },
        }
    }
}

/// A silk operation performed on arrival at a move's target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Nothing,
    /// Start a new line from a fresh fixed node here.
    PayOut(ThreadKind),
    /// Start a line of this kind from an existing node within `radius` of
    /// here, or from a fresh free node if there is none.
    StartLine { kind: ThreadKind, radius: f32 },
    /// Start a line of this kind from a point on an existing thread within
    /// `radius` (of one kind, if given), splitting it — the drop from the
    /// bridge's midpoint. Falls back to `StartLine`.
    StartOn { kind: ThreadKind, on: Option<ThreadKind>, radius: f32 },
    /// Fix the line to a new node here.
    Attach(Anchor),
    /// Fix the line to an existing node within `radius`, else to a new free
    /// node here.
    AttachNear(f32),
    /// Fix the line onto an existing thread within `radius` (of one kind, if
    /// given), splitting it — a spiral onto a radius, a radius onto the
    /// frame. Falls back to a node, then to a new free node.
    AttachOn { kind: Option<ThreadKind>, radius: f32 },
    /// Let the line go.
    Release,
    /// Remove every thread of `kind` whose ends are both farther than
    /// `min_dist` from `center`: the auxiliary spiral being cut away as the
    /// capture spiral passes inward.
    CutKindBeyond { kind: ThreadKind, center: Vec2, min_dist: f32 },
    /// Remove threads of `kind` within `radius` of here.
    CutHere { kind: Option<ThreadKind>, radius: f32 },
}

/// One step of a construction program: walk to `to`, then do the ops.
#[derive(Debug, Clone, PartialEq)]
pub struct Move {
    pub to: Vec2,
    pub ops: Vec<Op>,
    /// Seconds to stay put afterwards: a theridiid resting in its retreat
    /// between bouts.
    pub dwell: f32,
}

impl Move {
    pub fn new(to: Vec2, op: Op) -> Self {
        Move {
            to,
            ops: vec![op],
            dwell: 0.0,
        }
    }
    pub fn walk(to: Vec2) -> Self {
        Move {
            to,
            ops: Vec::new(),
            dwell: 0.0,
        }
    }
}

/// A species' web-construction program. Procedural, and labelled so.
pub trait WebProgram {
    fn species(&self) -> Species;
    /// The next move, or `None` when there is nothing to build right now.
    fn next(&mut self, silk: &Silk, region: Region, rng: &mut Pcg32, pos: Vec2) -> Option<Move>;
    /// Where the spider waits for prey, once there is somewhere to wait.
    fn sit_point(&self, silk: &Silk) -> Option<Vec2>;
    /// How far along the current web is, 0..1.
    fn progress(&self) -> f32;
    fn stage(&self) -> &'static str;
    /// Whether the web is finished as far as the program is concerned.
    fn complete(&self) -> bool;
    /// Start over on an empty silk.
    fn reset(&mut self, region: Region, rng: &mut Pcg32);
    /// Threads were cut. Re-plan repairs; return `true` if the damage is
    /// beyond repair and the web should be taken down and rebuilt.
    fn on_damage(&mut self, silk: &Silk) -> bool;
    /// Whether the species takes its web down at dusk and rebuilds.
    fn rebuilds_daily(&self) -> bool;
    /// Called at each dusk for species that add to a standing web.
    fn nightly(&mut self, _silk: &Silk, _region: Region, _rng: &mut Pcg32) {}
}

/// Which construction program a species runs (WEB_PLAN.md §5).
pub fn program_for(species: Species, region: Region, rng: &mut Pcg32) -> Box<dyn WebProgram> {
    match species {
        Species::Araneus => Box::new(crate::orb::OrbProgram::new(region, rng)),
        Species::Parasteatoda => Box::new(crate::cobweb::CobwebProgram::new(region, rng)),
        Species::Agelenopsis => Box::new(crate::funnel::FunnelProgram::new(region, rng)),
    }
}

/// A program that builds nothing: the chassis with no web, for tests.
pub struct Idle(pub Species);

impl WebProgram for Idle {
    fn species(&self) -> Species {
        self.0
    }
    fn next(&mut self, _: &Silk, _: Region, _: &mut Pcg32, _: Vec2) -> Option<Move> {
        None
    }
    fn sit_point(&self, _: &Silk) -> Option<Vec2> {
        None
    }
    fn progress(&self) -> f32 {
        1.0
    }
    fn stage(&self) -> &'static str {
        "idle"
    }
    fn complete(&self) -> bool {
        true
    }
    fn reset(&mut self, _: Region, _: &mut Pcg32) {}
    fn on_damage(&mut self, _: &Silk) -> bool {
        false
    }
    fn rebuilds_daily(&self) -> bool {
        false
    }
}

/// Something small that can blunder into a web.
#[derive(Debug, Clone, Copy)]
pub struct WebBug {
    pub pos: Vec2,
    pub vel: Vec2,
    pub age: f32,
    /// Walking on the floor rather than flying — what a gumfoot line is for.
    pub grounded: bool,
    /// Caught on sticky silk (or lifted by a gumfoot line).
    pub stuck: bool,
    /// How hard it is still struggling, 0..1; this is what excites the silk.
    pub struggle: f32,
    /// A gumfoot line is hauling it upward toward this point.
    pub lift_to: Option<Vec2>,
}

/// Everything a renderer needs this frame.
#[derive(Debug, Clone)]
pub struct WeaverPose {
    pub pos: Vec2,
    pub heading: f32,
    pub z: f32,
    pub scale: f32,
    pub crouch: f32,
    pub legs: [(f32, f32); 8],
    pub abdomen_breathe: f32,
    pub state: WeaverState,
}

pub struct Weaver {
    pub species: Species,
    pub traits: Traits,
    pub pos: Vec2,
    pub heading: f32,
    pub speed: f32,
    pub state: WeaverState,
    pub state_age: f32,
    pub state_timer: f32,
    pub gait_phase: f32,
    pub time: f32,
    pub crouch: f32,
    pub z: f32,
    pub legs: [SpiderLeg; 8],
    pub silk: Silk,
    pub prey: Vec<WebBug>,
    pub captured: u32,
    pub program: Box<dyn WebProgram>,
    current: Option<Move>,
    dwell_timer: f32,
    /// DNp09 says the animal is moving: construction advances.
    pub build_gate: bool,
    pub attractor: Option<Vec2>,
    pub terrain: Vec<Ledge>,
    /// The user is in an editor or a terminal; a posture bias, as the
    /// salticid's.
    pub settled: bool,

    drop_anchor: Vec2,
    drop_len: f32,
    hang_timer: f32,
    target: Option<Vec2>,
    walk_target: Option<Vec2>,
    returning: bool,
    carrying: bool,
    pub feed_timer: f32,
    pub escape_cooldown: f32,
    pub backward_timer: f32,
    strike_cooldown: f32,

    /// Where the current web was planned: the enclosure, or a box on the
    /// desktop under a window edge (WEB_PLAN.md §6).
    pub build_region: Option<Region>,
    /// The window edge the web hangs from, if any. When it moves or closes
    /// the threads on it are cut, and the animal repairs or starts again.
    pub anchor_ledge: Option<Ledge>,
    /// Local hour, for dusk; set by the runtime from the senses.
    pub hour: f32,
    /// Seconds since the web was last completed.
    pub web_age: f32,
    was_complete: bool,
    /// Vibration under the legs this frame, for the transduction layer.
    pub felt: f32,
    live_nervous: f32,
    rng: Pcg32,
}

impl Weaver {
    pub fn new(species: Species, at: Vec2, seed: u64, program: Box<dyn WebProgram>) -> Self {
        let mut rng = Pcg32::new(seed);
        let heading = rng.range(0.0, std::f32::consts::TAU);
        Weaver {
            species,
            traits: Traits::of(species),
            pos: at,
            heading,
            speed: 0.0,
            state: WeaverState::Sitting,
            state_age: 0.0,
            state_timer: 0.0,
            gait_phase: rng.range(0.0, 1.0),
            time: rng.range(0.0, 100.0),
            crouch: 0.0,
            z: 0.0,
            legs: new_legs(),
            silk: Silk::new(),
            prey: Vec::new(),
            captured: 0,
            program,
            current: None,
            dwell_timer: 0.0,
            build_gate: true,
            attractor: None,
            terrain: Vec::new(),
            settled: false,
            drop_anchor: Vec2::ZERO,
            drop_len: 0.0,
            hang_timer: 0.0,
            target: None,
            walk_target: None,
            returning: false,
            carrying: false,
            feed_timer: 0.0,
            escape_cooldown: 0.0,
            backward_timer: 0.0,
            strike_cooldown: 0.0,
            build_region: None,
            anchor_ledge: None,
            hour: 12.0,
            web_age: 0.0,
            was_complete: false,
            felt: 0.0,
            live_nervous: 0.0,
            rng,
        }
    }

    pub fn walking_intensity(&self) -> f32 {
        if self.speed.abs() > 1.0 {
            clamp(self.speed.abs() / 60.0, 0.0, 1.0)
        } else {
            0.0
        }
    }

    pub fn scale(&self) -> f32 {
        WEAVER_SCALE
    }

    pub fn dragline(&self) -> Option<Vec2> {
        self.silk.trailing_anchor()
    }

    pub fn pose(&self) -> WeaverPose {
        let breathe = if self.state == WeaverState::Sleeping {
            1.0 + 0.04 * self.time.sin()
        } else {
            1.0 + 0.025 * (self.time * 2.6).sin()
        };
        WeaverPose {
            pos: self.pos,
            heading: self.heading,
            z: self.z,
            scale: self.scale(),
            crouch: self.crouch,
            legs: std::array::from_fn(|i| (self.legs[i].angle, self.legs[i].lift)),
            abdomen_breathe: breathe,
            state: self.state,
        }
    }

    fn set_state(&mut self, s: WeaverState) {
        if s == self.state {
            return;
        }
        self.state = s;
        self.state_age = 0.0;
    }

    /// Put a bug into the world. Flying bugs cross the web; grounded ones
    /// walk the floor under a gumfoot line.
    pub fn spawn_bug(&mut self, at: Vec2, vel: Vec2, grounded: bool) {
        self.prey.push(WebBug {
            pos: at,
            vel,
            age: 0.0,
            grounded,
            stuck: false,
            struggle: 0.0,
            lift_to: None,
        });
    }

    /// The web as the program sees it: finished or not.
    pub fn web_complete(&self) -> bool {
        self.program.complete()
    }

    pub fn sit_point(&self) -> Option<Vec2> {
        self.program.sit_point(&self.silk)
    }

    /// Something cut the silk near `p` (a cursor sweep, a moving window).
    /// Returns how many threads went.
    pub fn damage(&mut self, p: Vec2, r: f32) -> usize {
        let cut = self.silk.cut_near(p, r);
        if cut > 0 {
            self.current = None;
            if self.program.on_damage(&self.silk) {
                self.begin_rebuild();
            }
        }
        cut
    }

    /// Take the web down and start again — the daily rebuild, or damage
    /// beyond repair.
    pub fn begin_rebuild(&mut self) {
        self.current = None;
        self.target = None;
        self.walk_target = None;
        self.set_state(WeaverState::Eating);
    }

    /// Where to plan a web. An enclosure is used whole. On the open desktop
    /// the web is a box hung under the widest window edge that leaves room
    /// below it, or centred if there is none — never the whole screen.
    pub fn choose_build_region(region: Region, ledges: &[Ledge]) -> (Region, Option<Ledge>) {
        let (w, h) = region.size;
        if w <= ENCLOSURE_MAX && h <= ENCLOSURE_MAX {
            return (region, None);
        }
        let bw = FREE_ROAM_WEB.0.min(w - 40.0);
        let bh = FREE_ROAM_WEB.1.min(h - 40.0);
        let lo = region.min();
        let hi = region.max();
        let mut best: Option<Ledge> = None;
        for l in ledges {
            let width = l.x1 - l.x0;
            if width < LEDGE_MIN_WIDTH || l.y - bh < lo.y + 20.0 || l.y > hi.y - 10.0 {
                continue;
            }
            if best.map(|b| width > b.x1 - b.x0).unwrap_or(true) {
                best = Some(*l);
            }
        }
        match best {
            Some(l) => {
                let cx = ((l.x0 + l.x1) * 0.5).clamp(lo.x + bw * 0.5 + 10.0, hi.x - bw * 0.5 - 10.0);
                let width = (l.x1 - l.x0).min(bw);
                (
                    Region::new(Vec2::new(cx, l.y - bh * 0.5), (width, bh)),
                    Some(l),
                )
            }
            None => (Region::new(region.center, (bw, bh)), None),
        }
    }

    /// Plan (or re-plan) the web in a region chosen from the world.
    fn replan(&mut self, region: Region) {
        let (build, ledge) = Self::choose_build_region(region, &self.terrain);
        self.build_region = Some(build);
        self.anchor_ledge = ledge;
        let mut r = Pcg32::new(self.rng.next_u32() as u64);
        self.program.reset(build, &mut r);
        self.current = None;
    }

    /// The window edge the web hangs from moved or closed: cut every thread
    /// fixed on it, and only those. Returns how many went.
    fn check_anchor_ledge(&mut self) -> usize {
        let Some(l) = self.anchor_ledge else { return 0 };
        let still = self
            .terrain
            .iter()
            .any(|c| c.id == l.id && (c.y - l.y).abs() < 3.0 && c.x0 <= l.x0 + 3.0 && c.x1 >= l.x1 - 3.0);
        if still {
            return 0;
        }
        let fixed: Vec<Vec2> = self
            .silk
            .nodes
            .iter()
            .filter(|n| n.anchor == Anchor::Fixed && (n.pos.y - l.y).abs() < 20.0)
            .map(|n| n.pos)
            .collect();
        let mut cut = 0;
        for p in fixed {
            cut += self.silk.cut_near(p, 3.0);
        }
        self.anchor_ledge = None;
        self.current = None;
        if cut > 0 && self.program.on_damage(&self.silk) {
            self.begin_rebuild();
        }
        cut
    }

    /// Turn toward and step toward `to`. True once there.
    fn walk_toward(&mut self, to: Vec2, speed: f32, dt: f32) -> bool {
        let dx = to.x - self.pos.x;
        let dy = to.y - self.pos.y;
        let d = hypot(dx, dy);
        let step = speed * dt;
        if d <= step.max(1.5) {
            self.pos = to;
            self.speed = 0.0;
            return true;
        }
        let want = dy.atan2(dx);
        self.heading += angle_diff(self.heading, want) * (12.0 * dt).min(1.0);
        self.speed = speed;
        self.pos.x += dx / d * step;
        self.pos.y += dy / d * step;
        false
    }

    fn apply_op(&mut self, at: Vec2, op: Op) {
        match op {
            Op::Nothing => {}
            Op::PayOut(kind) => {
                self.silk.pay_out(at, kind);
            }
            Op::StartLine { kind, radius } => {
                self.silk.release();
                match self.silk.node_at(at, radius) {
                    Some(n) => {
                        self.silk.attach_to(n);
                        self.silk.set_kind(kind);
                    }
                    None => {
                        let n = self.silk.add_node(at, Anchor::Free);
                        self.silk.attach_to(n);
                        self.silk.set_kind(kind);
                    }
                }
            }
            Op::StartOn { kind, on, radius } => {
                self.silk.release();
                if let Some((t, _)) = self.silk.thread_near(at, on, radius) {
                    let m = self.silk.split_thread(t, at);
                    self.silk.attach_to(m);
                    self.silk.set_kind(kind);
                } else {
                    self.apply_op(at, Op::StartLine { kind, radius });
                }
            }
            Op::Attach(anchor) => {
                self.silk.attach(at, anchor);
            }
            Op::AttachNear(r) => match self.silk.node_at(at, r) {
                Some(n) => self.silk.attach_to(n),
                None => {
                    self.silk.attach(at, Anchor::Free);
                }
            },
            Op::AttachOn { kind, radius } => {
                if let Some((t, _)) = self.silk.thread_near(at, kind, radius) {
                    let m = self.silk.split_thread(t, at);
                    self.silk.attach_to(m);
                } else if let Some(n) = self.silk.node_at(at, radius) {
                    self.silk.attach_to(n);
                } else {
                    self.silk.attach(at, Anchor::Free);
                }
            }
            Op::Release => {
                self.silk.release();
            }
            Op::CutKindBeyond {
                kind,
                center,
                min_dist,
            } => {
                let far = |p: Vec2| hypot(p.x - center.x, p.y - center.y) >= min_dist;
                let doomed: Vec<usize> = self
                    .silk
                    .threads
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| {
                        t.kind == kind && far(self.silk.nodes[t.a].pos) && far(self.silk.nodes[t.b].pos)
                    })
                    .map(|(i, _)| i)
                    .collect();
                for i in doomed.into_iter().rev() {
                    self.silk.remove_thread(i);
                }
            }
            Op::CutHere { kind, radius } => {
                loop {
                    match self.silk.thread_near(at, kind, radius) {
                        Some((t, _)) => self.silk.remove_thread(t),
                        None => break,
                    }
                }
            }
        }
    }

    pub fn update(&mut self, dt: f32, region: Region, mouse: Option<Vec2>, signals: Option<BrainSignals>) {
        self.time += dt;
        self.state_age += dt;
        self.escape_cooldown = (self.escape_cooldown - dt).max(0.0);
        self.backward_timer = (self.backward_timer - dt).max(0.0);
        self.feed_timer = (self.feed_timer - dt).max(0.0);
        self.strike_cooldown = (self.strike_cooldown - dt).max(0.0);
        self.live_nervous = signals.map(|s| s.nervous).unwrap_or(0.0);

        // The world decides where the web goes: the first frame, and after
        // every rebuild, the program is planned in a region chosen from the
        // enclosure or the window edges.
        if self.build_region.is_none() {
            self.replan(region);
        }
        self.check_anchor_ledge();
        let region = self.build_region.unwrap_or(region);

        self.update_bugs(dt, region);
        self.silk.step(dt);
        // What the legs feel: the silk under them, and — through the
        // tension of a web the animal is standing in the middle of — a share
        // of everything shaking anywhere in it. The hub of an orb is coupled
        // to every radius; a funnel's mouth to the whole sheet.
        self.felt = self
            .silk
            .excitation_at(self.pos)
            .max(0.5 * self.silk.total_excitation());

        let complete = self.program.complete();
        if complete {
            if !self.was_complete {
                self.web_age = 0.0;
            }
            self.web_age += dt;
        }
        self.was_complete = complete;

        match self.state {
            WeaverState::Dropping | WeaverState::Hanging | WeaverState::Climbing => {
                self.update_drop(dt, signals.as_ref());
            }
            _ => {
                if let Some(s) = signals {
                    self.brain_behavior(&s, dt, region, mouse);
                } else {
                    self.brainless(dt, region);
                }
            }
        }

        self.z = 0.0;
        self.update_posture(dt);
        self.update_legs(dt);
    }

    fn update_bugs(&mut self, dt: f32, region: Region) {
        let (hw, hh) = region.half();
        let floor_y = region.center.y - hh + 6.0;
        // Sticky contact and gumfoot feet, read before the bugs move.
        let mut excites: Vec<(Vec2, f32)> = Vec::new();
        for b in self.prey.iter_mut() {
            b.age += dt;
            if let Some(to) = b.lift_to {
                // Hauled up by a snapped gumfoot line.
                let dx = to.x - b.pos.x;
                let dy = to.y - b.pos.y;
                let d = hypot(dx, dy);
                let step = 160.0 * dt;
                if d <= step {
                    b.pos = to;
                    b.lift_to = None;
                } else {
                    b.pos.x += dx / d * step;
                    b.pos.y += dy / d * step;
                }
                // The line is what shakes, from where it now hangs.
                excites.push((to, 1.5 * dt));
                continue;
            }
            if b.stuck {
                b.struggle *= (-dt / 25.0).exp();
                if b.struggle > 0.02 {
                    excites.push((b.pos, b.struggle * 1.2 * dt));
                }
                continue;
            }
            if b.grounded {
                // Along the floor, turning back at the walls.
                b.pos.y = floor_y;
                b.vel.y = 0.0;
                b.vel.x += self.rng.range(-1.0, 1.0) * 60.0 * dt;
                b.vel.x = clamp(b.vel.x, -35.0, 35.0);
                b.pos.x += b.vel.x * dt;
                if (b.pos.x - region.center.x).abs() > hw - 20.0 {
                    b.vel.x = -b.vel.x;
                    b.pos.x = clamp(b.pos.x, region.center.x - hw + 20.0, region.center.x + hw - 20.0);
                }
            } else {
                b.vel.x += self.rng.range(-1.0, 1.0) * 220.0 * dt;
                b.vel.y += self.rng.range(-1.0, 1.0) * 220.0 * dt;
                let sp = hypot(b.vel.x, b.vel.y);
                if sp > 70.0 {
                    b.vel.x *= 70.0 / sp;
                    b.vel.y *= 70.0 / sp;
                }
                b.pos.x += b.vel.x * dt;
                b.pos.y += b.vel.y * dt;
                if (b.pos.x - region.center.x).abs() > hw - 20.0 {
                    b.vel.x = -b.vel.x;
                    b.pos.x = clamp(b.pos.x, region.center.x - hw + 20.0, region.center.x + hw - 20.0);
                }
                if (b.pos.y - region.center.y).abs() > hh - 20.0 {
                    b.vel.y = -b.vel.y;
                    b.pos.y = clamp(b.pos.y, region.center.y - hh + 20.0, region.center.y + hh - 20.0);
                }
            }
        }
        // Contact with sticky silk.
        for i in 0..self.prey.len() {
            let b = self.prey[i];
            if b.stuck || b.lift_to.is_some() {
                continue;
            }
            if b.grounded {
                // A gumfoot line's foot: the lowest node of a gumfoot thread.
                let foot = self
                    .silk
                    .threads
                    .iter()
                    .filter(|t| t.kind == ThreadKind::Gumfoot)
                    .map(|t| {
                        let (pa, pb) = (self.silk.nodes[t.a].pos, self.silk.nodes[t.b].pos);
                        if pa.y < pb.y {
                            (pa, pb)
                        } else {
                            (pb, pa)
                        }
                    })
                    .find(|(low, _)| hypot(low.x - b.pos.x, low.y - b.pos.y) < 8.0);
                if let Some((low, high)) = foot {
                    // The foot breaks and the tensioned line snaps up with
                    // the prey. The floor node rides up too.
                    if let Some(n) = self.silk.node_at(low, 2.0) {
                        let lifted = Vec2::new(high.x, high.y - 0.25 * (high.y - low.y));
                        self.silk.nodes[n].pos = lifted;
                        self.silk.nodes[n].anchor = Anchor::Free;
                        self.prey[i].stuck = true;
                        self.prey[i].struggle = 1.0;
                        self.prey[i].lift_to = Some(lifted);
                    }
                }
            } else if let Some((t, _)) = self.silk.thread_near(b.pos, None, STICK_RADIUS) {
                if self.silk.threads[t].kind.is_sticky() {
                    let p = self.silk.point_on_thread(t, b.pos);
                    self.prey[i].pos = p;
                    self.prey[i].stuck = true;
                    self.prey[i].struggle = 1.0;
                    self.prey[i].vel = Vec2::ZERO;
                } else if self.silk.threads[t].kind == ThreadKind::Sheet {
                    // A sheet is not sticky; it entangles. The bug slows and
                    // shakes the sheet while it is on it.
                    self.prey[i].vel.x *= 0.5;
                    self.prey[i].vel.y *= 0.5;
                    excites.push((b.pos, 1.0 * dt));
                }
            }
        }
        for (p, amount) in excites {
            self.silk.excite(p, amount);
        }
        self.prey.retain(|b| b.age < BUG_LIFETIME);
    }

    fn start_drop(&mut self) {
        self.silk.pay_out(self.pos, ThreadKind::Dragline);
        self.drop_anchor = self.pos;
        self.drop_len = self.rng.range(50.0, 120.0);
        self.hang_timer = self.rng.range(2.0, 4.5);
        self.set_state(WeaverState::Dropping);
        self.escape_cooldown = 1.5;
        self.speed = 0.0;
        self.current = None;
    }

    fn update_drop(&mut self, dt: f32, signals: Option<&BrainSignals>) {
        if let Some(s) = signals {
            if s.escape && self.escape_cooldown == 0.0 && self.state != WeaverState::Dropping {
                // Startled again on the line: let out more.
                self.drop_len += self.rng.range(30.0, 70.0);
                self.hang_timer = self.rng.range(2.0, 4.0);
                self.escape_cooldown = 1.5;
                self.set_state(WeaverState::Dropping);
            }
        }
        let bottom = self.drop_anchor.y - self.drop_len;
        match self.state {
            WeaverState::Dropping => {
                self.pos.y -= 140.0 * dt;
                if self.pos.y <= bottom {
                    self.pos.y = bottom;
                    self.set_state(WeaverState::Hanging);
                }
            }
            WeaverState::Hanging => {
                self.hang_timer -= dt;
                self.heading += 0.5 * dt;
                self.pos.x = self.drop_anchor.x + (self.time * 1.3).sin() * 2.5;
                if self.hang_timer <= 0.0 {
                    self.set_state(WeaverState::Climbing);
                }
            }
            _ => {
                self.pos.y += 45.0 * dt;
                self.pos.x += (self.drop_anchor.x - self.pos.x) * (4.0 * dt).min(1.0);
                if self.pos.y >= self.drop_anchor.y {
                    self.pos = self.drop_anchor;
                    self.silk.release();
                    self.set_state(WeaverState::Sitting);
                    self.state_timer = self.rng.range(1.0, 3.0);
                }
            }
        }
    }

    /// Every decision here reads a population rate — or, for the strike's
    /// target, the one readout that is honestly modelled.
    fn brain_behavior(&mut self, s: &BrainSignals, dt: f32, region: Region, mouse: Option<Vec2>) {
        // Giant fiber spike: the species' threat response, from any state.
        if s.escape && self.escape_cooldown == 0.0 {
            match self.traits.threat {
                ThreatResponse::Drop => self.start_drop(),
                ThreatResponse::Retreat => {
                    if let Some(sit) = self.sit_point() {
                        self.target = Some(sit);
                        self.set_state(WeaverState::Retreating);
                        self.escape_cooldown = 1.5;
                        self.current = None;
                    } else {
                        self.start_drop();
                    }
                }
            }
            return;
        }
        let _ = mouse;
        if s.sleep {
            if self.state != WeaverState::Sleeping {
                self.set_state(WeaverState::Sleeping);
                self.speed = 0.0;
                self.current = None;
            }
            return;
        } else if self.state == WeaverState::Sleeping {
            self.set_state(WeaverState::Grooming);
            return;
        }

        // Looming detectors hot but no GF: freeze. A web spider does not
        // dart; it holds still (or shakes the web, which is Phase 7's).
        if s.nervous > 0.4 && matches!(self.state, WeaverState::Building | WeaverState::Walking) {
            self.speed = 0.0;
            return;
        }

        // The authored strike node: go to the loudest thing in the web.
        if s.strike
            && self.strike_cooldown == 0.0
            && self.feed_timer == 0.0
            && matches!(
                self.state,
                WeaverState::Sitting | WeaverState::Building | WeaverState::Walking | WeaverState::Grooming
            )
        {
            if let Some((at, _)) = self.silk.loudest(0.005) {
                self.target = Some(at);
                self.set_state(WeaverState::Hunting);
                self.current = None;
                self.strike_cooldown = 1.0;
            } else {
                self.strike_cooldown = 0.6;
            }
        }

        // DNg11 grooming hysteresis, from stillness.
        if matches!(self.state, WeaverState::Sitting | WeaverState::Grooming) {
            if self.state != WeaverState::Grooming && s.groom_drive > 0.5 && s.nervous < 0.3 && self.state_age > 0.4 {
                self.set_state(WeaverState::Grooming);
            } else if self.state == WeaverState::Grooming && s.groom_drive < 0.3 && self.state_age > 0.6 {
                self.set_state(WeaverState::Sitting);
            }
        }

        // DNp09: the walk drive gates construction and wandering.
        if s.walk_drive > 0.12 {
            self.build_gate = true;
        } else if s.walk_drive < 0.05 {
            self.build_gate = false;
        }

        // MDN burst: a short back-step.
        if s.backward && self.backward_timer == 0.0 && matches!(self.state, WeaverState::Sitting | WeaverState::Walking) {
            self.backward_timer = 0.4;
        }

        self.run_state(dt, region, s.tempo, Some(s));
    }

    /// No brain: build and sit. Labelled brainless by the shell.
    fn brainless(&mut self, dt: f32, region: Region) {
        self.build_gate = true;
        self.run_state(dt, region, 1.0, None);
    }

    /// The state machine proper, shared by both paths.
    fn run_state(&mut self, dt: f32, region: Region, tempo: f32, s: Option<&BrainSignals>) {
        let hunt = self.traits.hunt_speed * tempo;
        let build = BUILD_SPEED * tempo;
        match self.state {
            WeaverState::Hunting => {
                if let Some(t) = self.target {
                    if self.walk_toward(t, hunt, dt) {
                        self.set_state(WeaverState::Wrapping);
                        self.state_timer = 1.4;
                    }
                } else {
                    self.set_state(WeaverState::Sitting);
                }
            }
            WeaverState::Wrapping => {
                self.speed = 0.0;
                self.state_timer -= dt;
                if self.state_timer <= 0.0 {
                    // Anything stuck within reach is the catch.
                    let here = self.pos;
                    let caught = self
                        .prey
                        .iter()
                        .position(|b| b.stuck && hypot(b.pos.x - here.x, b.pos.y - here.y) < CAPTURE_RADIUS);
                    if let Some(i) = caught {
                        self.prey.remove(i);
                        self.captured += 1;
                        self.carrying = true;
                    } else {
                        self.carrying = false;
                    }
                    self.target = self.sit_point();
                    self.set_state(WeaverState::Carrying);
                }
            }
            WeaverState::Carrying => {
                let done = match self.target {
                    Some(t) => self.walk_toward(t, build, dt),
                    None => true,
                };
                if done {
                    self.feed_timer = if self.carrying { 4.0 } else { 0.5 };
                    self.set_state(WeaverState::Feeding);
                }
            }
            WeaverState::Feeding => {
                self.speed = 0.0;
                if self.feed_timer <= 0.0 {
                    self.carrying = false;
                    self.set_state(WeaverState::Sitting);
                }
            }
            WeaverState::Retreating => {
                let done = match self.target {
                    Some(t) => self.walk_toward(t, hunt, dt),
                    None => true,
                };
                if done {
                    self.set_state(WeaverState::Sitting);
                    self.crouch = 1.0;
                }
            }
            WeaverState::Eating => {
                // Take the web down thread by thread, walking to each.
                if self.silk.threads.is_empty() {
                    self.silk.clear();
                    self.build_region = None;
                    self.set_state(WeaverState::Building);
                } else {
                    let here = self.pos;
                    let (t, _) = self
                        .silk
                        .threads
                        .iter()
                        .enumerate()
                        .map(|(i, th)| {
                            let (a, b) = (self.silk.nodes[th.a].pos, self.silk.nodes[th.b].pos);
                            let m = Vec2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
                            (i, hypot(m.x - here.x, m.y - here.y))
                        })
                        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                        .unwrap();
                    let th = self.silk.threads[t];
                    let (a, b) = (self.silk.nodes[th.a].pos, self.silk.nodes[th.b].pos);
                    let m = Vec2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
                    if self.walk_toward(m, build * 1.5, dt) {
                        self.silk.remove_thread(t);
                    }
                }
            }
            WeaverState::Grooming | WeaverState::Sleeping => {
                self.speed = 0.0;
            }
            WeaverState::Sitting | WeaverState::Building | WeaverState::Walking => {
                self.run_build_or_sit(dt, region, build, s);
            }
            _ => {}
        }
    }

    fn run_build_or_sit(&mut self, dt: f32, region: Region, build: f32, s: Option<&BrainSignals>) {
        // Dusk: a daily rebuilder takes the old web down; a standing-web
        // species adds to it.
        let dusk = self.hour >= DUSK.0 && self.hour < DUSK.1;
        if dusk && self.program.complete() && self.web_age > REBUILD_AGE {
            if self.program.rebuilds_daily() {
                self.begin_rebuild();
                return;
            } else {
                let mut r = Pcg32::new(self.rng.next_u32() as u64);
                self.program.nightly(&self.silk, region, &mut r);
                self.web_age = 0.0;
            }
        }

        // A back-step from MDN takes precedence over everything below.
        if self.backward_timer > 0.0 {
            let v = -18.0;
            self.pos.x += self.heading.cos() * v * dt;
            self.pos.y += self.heading.sin() * v * dt;
            self.pos = region.clamp_inside(self.pos, 20.0);
            self.speed = v;
            return;
        }

        // A move that ends in a rest: stay put until it is over.
        if self.dwell_timer > 0.0 {
            self.dwell_timer -= dt;
            self.speed = 0.0;
            self.set_state(WeaverState::Sitting);
            return;
        }

        // Construction: one move at a time, while the brain says move.
        if self.current.is_none() && self.build_gate {
            let here = self.pos;
            let mut r = Pcg32::new(self.rng.next_u32() as u64);
            self.current = self.program.next(&self.silk, region, &mut r, here);
        }
        if let Some(mv) = self.current.clone() {
            if !self.build_gate {
                self.speed = 0.0;
                return;
            }
            self.set_state(WeaverState::Building);
            self.walk_target = None;
            if self.walk_toward(mv.to, build, dt) {
                for op in &mv.ops {
                    self.apply_op(mv.to, *op);
                }
                self.current = None;
                self.dwell_timer = mv.dwell;
            }
            return;
        }

        // Nothing to build: sit, or wander over the web a little.
        let sit = self.sit_point();
        if let Some(t) = self.walk_target {
            self.set_state(WeaverState::Walking);
            if self.walk_toward(t, build * 0.6, dt) {
                if self.returning || sit.is_none() {
                    self.walk_target = None;
                    self.returning = false;
                } else {
                    self.walk_target = sit;
                    self.returning = true;
                }
            }
            return;
        }
        match sit {
            Some(p) if hypot(p.x - self.pos.x, p.y - self.pos.y) > 2.0 => {
                self.set_state(WeaverState::Walking);
                self.walk_toward(p, build * 0.8, dt);
            }
            _ => {
                self.set_state(WeaverState::Sitting);
                self.speed = 0.0;
                if let Some(h) = self.traits.sit_heading {
                    self.heading += angle_diff(self.heading, h) * (3.0 * dt).min(1.0);
                }
                // An occasional patrol, when the walk drive is up.
                let drive = s.map(|x| x.walk_drive).unwrap_or(0.0);
                if drive > 0.3 && !self.silk.nodes.is_empty() && self.rng.f32() < 0.03 * dt {
                    let i = self.rng.int_range(0, self.silk.nodes.len() as i64 - 1) as usize;
                    self.walk_target = Some(self.silk.nodes[i].pos);
                    self.returning = false;
                }
            }
        }
    }

    fn update_posture(&mut self, dt: f32) {
        let target = match self.state {
            WeaverState::Wrapping | WeaverState::Feeding => 0.8,
            WeaverState::Sleeping => 0.6,
            WeaverState::Retreating => 0.5,
            _ => {
                if self.live_nervous > 0.4 {
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
        let v = self.speed.abs();
        let mode = if v > 1.0 {
            LegMode::Walk {
                speed: v,
                backward: self.backward_timer > 0.0,
            }
        } else {
            match self.state {
                WeaverState::Grooming => LegMode::Groom { time: self.time },
                WeaverState::Wrapping => LegMode::Groom { time: self.time * 1.6 },
                WeaverState::Dropping | WeaverState::Hanging | WeaverState::Climbing => LegMode::Hang,
                _ => LegMode::Rest,
            }
        };
        step_legs(&mut self.legs, &mut self.gait_phase, dt, mode);
    }
}

impl Body for Weaver {
    fn substrate(&self) -> Substrate {
        Substrate::WalkerWeaver
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

    const BOUNDS: Region = Region {
        center: Vec2::ZERO,
        size: (720.0, 520.0),
    };
    const DT: f32 = 1.0 / 60.0;

    fn idle(species: Species) -> Weaver {
        Weaver::new(species, Vec2::ZERO, 3, Box::new(Idle(species)))
    }

    #[test]
    fn a_giant_fiber_spike_drops_an_orb_weaver_on_its_line_and_it_climbs_back() {
        let mut w = idle(Species::Araneus);
        let mut s = BrainSignals::new();
        s.escape = true;
        w.update(DT, BOUNDS, None, Some(s));
        assert_eq!(w.state, WeaverState::Dropping);
        assert_eq!(w.dragline(), Some(Vec2::ZERO));
        let calm = BrainSignals::new();
        let (mut lowest, mut frames) = (0.0f32, 0);
        while w.state != WeaverState::Sitting && frames < 60 * 30 {
            w.update(DT, BOUNDS, None, Some(calm));
            lowest = lowest.min(w.pos.y);
            frames += 1;
        }
        assert!(lowest < -40.0, "it dropped: {lowest}");
        assert_eq!(w.state, WeaverState::Sitting);
        assert!(w.dragline().is_none(), "the line is retracted");
        assert_eq!(w.pos, Vec2::ZERO);
    }

    #[test]
    fn a_funnel_weaver_retreats_instead_of_dropping() {
        struct Home;
        impl WebProgram for Home {
            fn species(&self) -> Species {
                Species::Agelenopsis
            }
            fn next(&mut self, _: &Silk, _: Region, _: &mut Pcg32, _: Vec2) -> Option<Move> {
                None
            }
            fn sit_point(&self, _: &Silk) -> Option<Vec2> {
                Some(Vec2::new(-200.0, -100.0))
            }
            fn progress(&self) -> f32 {
                1.0
            }
            fn stage(&self) -> &'static str {
                "home"
            }
            fn complete(&self) -> bool {
                true
            }
            fn reset(&mut self, _: Region, _: &mut Pcg32) {}
            fn on_damage(&mut self, _: &Silk) -> bool {
                false
            }
            fn rebuilds_daily(&self) -> bool {
                false
            }
        }
        let mut w = Weaver::new(Species::Agelenopsis, Vec2::new(100.0, 50.0), 3, Box::new(Home));
        let mut s = BrainSignals::new();
        s.escape = true;
        w.update(DT, BOUNDS, None, Some(s));
        assert_eq!(w.state, WeaverState::Retreating);
        assert!(w.dragline().is_none(), "no drop");
        let calm = BrainSignals::new();
        for _ in 0..600 {
            w.update(DT, BOUNDS, None, Some(calm));
        }
        assert_eq!(w.state, WeaverState::Sitting);
        assert_eq!(w.pos, Vec2::new(-200.0, -100.0));
    }

    #[test]
    fn a_bug_that_hits_sticky_silk_sticks_and_shakes_the_web() {
        let mut w = idle(Species::Araneus);
        w.silk.pay_out(Vec2::new(-100.0, 0.0), ThreadKind::Capture);
        w.silk.attach(Vec2::new(100.0, 0.0), Anchor::Fixed);
        w.silk.release();
        w.spawn_bug(Vec2::new(0.0, 30.0), Vec2::new(0.0, -60.0), false);
        let calm = BrainSignals::new();
        let mut stuck_at = None;
        for _ in 0..120 {
            w.update(DT, BOUNDS, None, Some(calm));
            if w.prey[0].stuck {
                stuck_at = Some(w.prey[0].pos);
                break;
            }
        }
        let p = stuck_at.expect("the bug crossed a sticky thread");
        assert!(p.y.abs() < 1e-3, "snapped onto the thread: {p:?}");
        for _ in 0..30 {
            w.update(DT, BOUNDS, None, Some(calm));
        }
        assert!(w.silk.loudest(0.001).is_some(), "a struggle excites the silk");
        // Non-sticky silk does not catch.
        let mut w2 = idle(Species::Araneus);
        w2.silk.pay_out(Vec2::new(-100.0, 0.0), ThreadKind::Radius);
        w2.silk.attach(Vec2::new(100.0, 0.0), Anchor::Fixed);
        w2.silk.release();
        w2.spawn_bug(Vec2::new(0.0, 30.0), Vec2::new(0.0, -60.0), false);
        for _ in 0..120 {
            w2.update(DT, BOUNDS, None, Some(calm));
        }
        assert!(!w2.prey[0].stuck);
    }

    #[test]
    fn a_strike_sends_the_spider_to_the_loudest_node_and_it_catches_what_is_there() {
        let mut w = idle(Species::Araneus);
        w.silk.pay_out(Vec2::new(0.0, 0.0), ThreadKind::Capture);
        w.silk.attach(Vec2::new(150.0, 0.0), Anchor::Fixed);
        w.silk.release();
        w.spawn_bug(Vec2::new(150.0, 0.0), Vec2::ZERO, false);
        w.prey[0].stuck = true;
        w.prey[0].struggle = 1.0;
        let calm = BrainSignals::new();
        for _ in 0..20 {
            w.update(DT, BOUNDS, None, Some(calm));
        }
        let mut s = BrainSignals::new();
        s.strike = true;
        w.update(DT, BOUNDS, None, Some(s));
        assert_eq!(w.state, WeaverState::Hunting);
        for _ in 0..60 * 8 {
            w.update(DT, BOUNDS, None, Some(calm));
            if w.captured > 0 {
                break;
            }
        }
        assert_eq!(w.captured, 1, "state {:?} pos {:?}", w.state, w.pos);
    }

    #[test]
    fn a_grounded_bug_that_touches_a_gumfoot_foot_is_snapped_upward() {
        let mut w = idle(Species::Parasteatoda);
        let floor = BOUNDS.center.y - BOUNDS.half().1 + 6.0;
        w.silk.pay_out(Vec2::new(0.0, 100.0), ThreadKind::Gumfoot);
        w.silk.attach(Vec2::new(0.0, floor), Anchor::Fixed);
        w.silk.release();
        w.spawn_bug(Vec2::new(-30.0, floor), Vec2::new(30.0, 0.0), true);
        let calm = BrainSignals::new();
        let mut snapped = false;
        for _ in 0..60 * 6 {
            w.update(DT, BOUNDS, None, Some(calm));
            if w.prey[0].stuck && w.prey[0].pos.y > floor + 40.0 {
                snapped = true;
                break;
            }
        }
        assert!(snapped, "bug at {:?} stuck {}", w.prey[0].pos, w.prey[0].stuck);
    }

    #[test]
    fn free_roam_hangs_the_web_under_a_wide_window_edge_and_an_enclosure_is_used_whole() {
        let tank = Region::centered((720.0, 520.0));
        let (r, l) = Weaver::choose_build_region(tank, &[]);
        assert_eq!(r, tank);
        assert!(l.is_none());
        let display = Region::centered((2560.0, 1440.0));
        let (r, l) = Weaver::choose_build_region(display, &[]);
        assert!(l.is_none());
        assert!(r.size.0 < 700.0 && r.size.1 < 500.0, "never the whole screen: {:?}", r.size);
        assert_eq!(r.center, display.center);
        let narrow = Ledge { y: 300.0, x0: 0.0, x1: 100.0, id: 1 };
        let wide = Ledge { y: 200.0, x0: -400.0, x1: 200.0, id: 2 };
        let (r, l) = Weaver::choose_build_region(display, &[narrow, wide]);
        assert_eq!(l.map(|l| l.id), Some(2), "the wide one");
        assert!((r.max().y - 200.0).abs() < 1e-3, "hangs from the edge: top at {}", r.max().y);
        assert!(r.size.0 <= 600.0);
    }

    #[test]
    fn a_moved_window_edge_cuts_only_the_threads_fixed_on_it() {
        let display = Region::centered((2560.0, 1440.0));
        let ledge = Ledge { y: 220.0, x0: -420.0, x1: 240.0, id: 9 };
        let mut rng = Pcg32::new(5);
        let program = crate::orb::OrbProgram::new(display, &mut rng);
        let mut w = Weaver::new(Species::Araneus, Vec2::ZERO, 5, Box::new(program));
        w.terrain = vec![ledge];
        let mut s = BrainSignals::new();
        s.walk_drive = 0.6;
        let mut t = 0.0;
        while !w.web_complete() && t < 900.0 {
            w.update(DT, display, None, Some(s));
            t += DT;
        }
        assert!(w.web_complete(), "{}", w.program.stage());
        assert_eq!(w.anchor_ledge.map(|l| l.id), Some(9));
        let top = w.build_region.unwrap().max().y;
        assert!((top - 220.0).abs() < 1e-3);
        let on_ledge: Vec<Vec2> = w
            .silk
            .nodes
            .iter()
            .filter(|n| n.anchor == Anchor::Free || n.anchor == Anchor::Fixed)
            .filter(|n| n.anchor == Anchor::Fixed && (n.pos.y - 220.0).abs() < 20.0)
            .map(|n| n.pos)
            .collect();
        assert!(!on_ledge.is_empty(), "something is fixed on the edge");
        let touching = w
            .silk
            .threads
            .iter()
            .filter(|t| {
                on_ledge.iter().any(|p| {
                    let (a, b) = (w.silk.nodes[t.a].pos, w.silk.nodes[t.b].pos);
                    (a.x - p.x).abs() < 1e-3 && (a.y - p.y).abs() < 1e-3
                        || (b.x - p.x).abs() < 1e-3 && (b.y - p.y).abs() < 1e-3
                })
            })
            .count();
        let before = w.silk.threads.len();
        // The window moves down.
        w.terrain = vec![Ledge { y: 100.0, ..ledge }];
        w.update(DT, display, None, Some(s));
        let after = w.silk.threads.len();
        assert_eq!(before - after, touching, "only the threads on the edge went: {before} -> {after}, {touching} touched it");
        assert!(w.anchor_ledge.is_none());
        assert!(
            matches!(w.state, WeaverState::Eating) || w.program.stage() == "repairing",
            "state {:?} stage {}",
            w.state,
            w.program.stage()
        );
    }

    #[test]
    fn the_program_never_writes_to_the_brain() {
        // Structural: the program's only inputs are the silk, the region, a
        // PRNG and a position, and its only output is a Move. There is no
        // path from a WebProgram to a BrainSignals or a LifSim. This test
        // exists so the seam is named in the suite; the type system does the
        // enforcing.
        fn takes_only_the_world(p: &mut dyn WebProgram, silk: &Silk, region: Region) -> Option<Move> {
            let mut r = Pcg32::new(1);
            p.next(silk, region, &mut r, Vec2::ZERO)
        }
        let mut p = Idle(Species::Araneus);
        assert!(takes_only_the_world(&mut p, &Silk::new(), BOUNDS).is_none());
    }
}
