//! *Araneus diadematus*: orb-web construction as a walked path (WEB_PLAN.md
//! §5.1).
//!
//! The sequence is Zschokke & Vollrath's (1995) and Zschokke's (1996), stage
//! by stage, with Vollrath's largest-gap rule for where each new radius goes:
//!
//! 1. **Exploration and bridge** — a few walks between anchors trailing a
//!    line that is let go again (the variable early stage), then the bridge.
//! 2. **Proto-hub and the Y** — drop from the bridge's midpoint to the hub
//!    and on to the anchor below; radii back up to the bridge ends.
//! 3. **Frame** — a polygon of frame threads at the web's radius, each
//!    vertex guyed out to a wall anchor.
//! 4. **Radii** — each one an out-and-back: out along an existing radius,
//!    along the frame to the new attachment point, back to the hub laying
//!    the thread, into the largest remaining angular gap.
//! 5. **Auxiliary spiral** — outward from the hub, few turns, wide pitch.
//! 6. **Capture spiral** — inward from the periphery, sticky, tight; the
//!    auxiliary spiral is cut away as the capture spiral passes it.
//! 7. **Finish** — back to the hub, head down.
//!
//! What this is not: a physics simulation, and not a claim that these are the
//! animal's rules at the neural level. It is a motor program with **no
//! neurons in it**, and the creature's provenance line says so.

use std::collections::VecDeque;

use crate::creature::Weaver as Species;
use crate::anchors::Anchors;
use crate::habitat::Region;
use crate::rng::Pcg32;
use crate::silk::{Silk, ThreadKind};
use crate::util::{hypot, Vec2};
use crate::weaver::{Move, Op, WebProgram};

/// Species band for the number of radii.
pub const RADII: (usize, usize) = (25, 35);
/// Turns of the auxiliary (scaffold) spiral.
pub const AUX_TURNS: f32 = 5.0;
/// Species band for capture-spiral turns.
pub const CAPTURE_TURNS: (f32, f32) = (16.0, 24.0);
/// Inner edge of the capture spiral, as a fraction of the radius: the free
/// zone around the hub.
pub const FREE_ZONE: f32 = 0.2;
/// The web's radius as a fraction of the smaller side of the region, and its
/// bounds in scene units.
pub const RADIUS_FRACTION: f32 = 0.40;
pub const RADIUS_BOUNDS: (f32, f32) = (110.0, 300.0);
/// Frame polygon sides.
pub const FRAME_SIDES: usize = 6;
/// How far inside the region's edge a wall anchor sits.
pub const WALL_INSET: f32 = 14.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Explore,
    Bridge,
    Y,
    Frame,
    Radii,
    Auxiliary,
    Capture,
    Finish,
    Done,
    /// Re-laying radii after damage.
    Repair,
}

pub struct OrbProgram {
    hub: Vec2,
    radius: f32,
    frame: Vec<Vec2>,
    anchors: Vec<Vec2>,
    stage: Stage,
    queue: VecDeque<Move>,
    /// Angles of the radii laid so far.
    radii: Vec<f32>,
    target_radii: usize,
    capture_turns: f32,
    spiral_i: usize,
    spiral_n: usize,
    explored: u32,
    explore_target: u32,
    /// Capture-spiral length when the web was last finished, for the
    /// damage judgement.
    capture_full: f32,
}

impl OrbProgram {
    pub fn new(world: &Anchors, rng: &mut Pcg32) -> Self {
        let mut p = OrbProgram {
            hub: world.bounds.center,
            radius: 100.0,
            frame: Vec::new(),
            anchors: Vec::new(),
            stage: Stage::Explore,
            queue: VecDeque::new(),
            radii: Vec::new(),
            target_radii: 30,
            capture_turns: 20.0,
            spiral_i: 0,
            spiral_n: 0,
            explored: 0,
            explore_target: 2,
            capture_full: 0.0,
        };
        p.reset(world, rng);
        p
    }

    pub fn hub(&self) -> Vec2 {
        self.hub
    }
    pub fn radius(&self) -> f32 {
        self.radius
    }
    pub fn stage_enum(&self) -> Stage {
        self.stage
    }
    pub fn radii_count(&self) -> usize {
        self.radii.len()
    }

    /// Where a ray from the hub at `angle` meets the frame polygon.
    fn frame_point(&self, angle: f32) -> Vec2 {
        let (dx, dy) = (angle.cos(), angle.sin());
        let mut best = f32::MAX;
        let n = self.frame.len();
        for k in 0..n {
            let a = self.frame[k];
            let b = self.frame[(k + 1) % n];
            // Ray hub + t*d against segment a + u*(b-a).
            let ex = b.x - a.x;
            let ey = b.y - a.y;
            let den = dx * ey - dy * ex;
            if den.abs() < 1e-6 {
                continue;
            }
            let ax = a.x - self.hub.x;
            let ay = a.y - self.hub.y;
            let t = (ax * ey - ay * ex) / den;
            let u = (ax * dy - ay * dx) / den;
            if t > 0.0 && (-1e-4..=1.0 + 1e-4).contains(&u) {
                best = best.min(t);
            }
        }
        if best == f32::MAX {
            best = self.radius;
        }
        Vec2::new(self.hub.x + dx * best, self.hub.y + dy * best)
    }

    fn point_at(&self, angle: f32, r: f32) -> Vec2 {
        Vec2::new(self.hub.x + angle.cos() * r, self.hub.y + angle.sin() * r)
    }

    fn sorted_radii(&self) -> Vec<f32> {
        let mut v: Vec<f32> = self
            .radii
            .iter()
            .map(|a| a.rem_euclid(std::f32::consts::TAU))
            .collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v
    }

    /// The largest angular gap between radii: its midpoint and its size.
    fn largest_gap(&self) -> (f32, f32) {
        let v = self.sorted_radii();
        let n = v.len();
        let mut best = (0.0f32, 0.0f32);
        for i in 0..n {
            let a = v[i];
            let b = if i + 1 < n { v[i + 1] } else { v[0] + std::f32::consts::TAU };
            let gap = b - a;
            if gap > best.1 {
                best = (a + gap * 0.5, gap);
            }
        }
        best
    }

    /// The existing radius nearest an angle.
    fn nearest_radius(&self, angle: f32) -> f32 {
        let a = angle.rem_euclid(std::f32::consts::TAU);
        let mut best = (0.0f32, f32::MAX);
        for &r in &self.radii {
            let rr = r.rem_euclid(std::f32::consts::TAU);
            let mut d = (rr - a).abs();
            if d > std::f32::consts::PI {
                d = std::f32::consts::TAU - d;
            }
            if d < best.1 {
                best = (rr, d);
            }
        }
        best.0
    }

    fn queue_radius(&mut self, angle: f32) {
        let hub = self.hub;
        let near_end = self.frame_point(self.nearest_radius(angle));
        let f = self.frame_point(angle);
        self.queue.push_back(Move::walk(hub));
        self.queue.push_back(Move::walk(near_end));
        self.queue.push_back(Move::new(
            f,
            Op::StartOn {
                kind: ThreadKind::Radius,
                on: Some(ThreadKind::Frame),
                radius: 10.0,
            },
        ));
        self.queue.push_back(Move {
            to: hub,
            ops: vec![Op::AttachNear(8.0), Op::Release],
            dwell: 0.0,
        });
        self.radii.push(angle);
    }

    fn plan(&mut self, world: &Anchors, rng: &mut Pcg32) {
        use std::f32::consts::{FRAC_PI_2, PI, TAU};
        let hub = self.hub;
        match self.stage {
            Stage::Explore => {
                if self.explored >= self.explore_target {
                    self.stage = Stage::Bridge;
                    return self.plan(world, rng);
                }
                // A fixed behavioural pattern in a random order: walk to an
                // anchor trailing a line, walk on, let it go.
                let a = world.anchor(hub, rng.range(0.0, TAU), WALL_INSET);
                let b = world.anchor(hub, rng.range(0.0, TAU), WALL_INSET);
                self.queue.push_back(Move::new(a, Op::PayOut(ThreadKind::Dragline)));
                self.queue.push_back(Move::new(b, Op::Release));
                self.explored += 1;
            }
            Stage::Bridge => {
                let a = world.anchor(hub, FRAC_PI_2 + 0.55, WALL_INSET);
                let b = world.anchor(hub, FRAC_PI_2 - 0.55, WALL_INSET);
                self.anchors = vec![a, b];
                self.queue.push_back(Move::new(a, Op::PayOut(ThreadKind::Bridge)));
                self.queue.push_back(Move {
                    to: b,
                    ops: vec![Op::Attach(crate::silk::Anchor::Fixed), Op::Release],
                    dwell: 0.0,
                });
                self.stage = Stage::Y;
            }
            Stage::Y => {
                let a = self.anchors[0];
                let b = self.anchors[1];
                let m = Vec2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
                let c = world.anchor(hub, -FRAC_PI_2, WALL_INSET);
                // Drop from the bridge's midpoint through the hub to the
                // anchor below: the first radius, and the proto-hub.
                self.queue.push_back(Move::new(
                    m,
                    Op::StartOn {
                        kind: ThreadKind::Radius,
                        on: Some(ThreadKind::Bridge),
                        radius: 10.0,
                    },
                ));
                self.queue.push_back(Move::new(hub, Op::Attach(crate::silk::Anchor::Free)));
                self.queue.push_back(Move {
                    to: c,
                    ops: vec![Op::Attach(crate::silk::Anchor::Fixed), Op::Release],
                    dwell: 0.0,
                });
                // Back to the hub, then radii up to both bridge ends: the Y.
                for end in [a, b] {
                    self.queue.push_back(Move::walk(hub));
                    self.queue.push_back(Move::new(
                        end,
                        Op::StartLine {
                            kind: ThreadKind::Radius,
                            radius: 10.0,
                        },
                    ));
                    self.queue.push_back(Move {
                        to: hub,
                        ops: vec![Op::AttachNear(8.0), Op::Release],
                        dwell: 0.0,
                    });
                }
                let up = (m.y - hub.y).atan2(m.x - hub.x);
                self.radii = vec![
                    up,
                    -FRAC_PI_2,
                    (a.y - hub.y).atan2(a.x - hub.x),
                    (b.y - hub.y).atan2(b.x - hub.x),
                ];
                self.stage = Stage::Frame;
            }
            Stage::Frame => {
                // Vertices at the web's radius, guyed to the walls. The top
                // vertex sits on the first radius, the bottom one on the
                // drop to the anchor below.
                let n = FRAME_SIDES;
                let step = TAU / n as f32;
                self.frame = (0..n)
                    .map(|k| self.point_at(FRAC_PI_2 + k as f32 * step, self.radius))
                    .collect();
                self.anchors = (0..n)
                    .map(|k| world.anchor(hub, FRAC_PI_2 + k as f32 * step, WALL_INSET))
                    .collect();
                let v0 = self.frame[0];
                self.queue.push_back(Move::walk(hub));
                self.queue.push_back(Move::new(
                    v0,
                    Op::StartOn {
                        kind: ThreadKind::Frame,
                        on: Some(ThreadKind::Radius),
                        radius: 8.0,
                    },
                ));
                for k in 1..n {
                    let v = self.frame[k];
                    let w = self.anchors[k];
                    self.queue.push_back(Move::new(v, Op::AttachOn { kind: None, radius: 6.0 }));
                    self.queue.push_back(Move::new(w, Op::Attach(crate::silk::Anchor::Fixed)));
                    self.queue.push_back(Move::new(
                        v,
                        Op::StartLine {
                            kind: ThreadKind::Frame,
                            radius: 6.0,
                        },
                    ));
                }
                self.queue.push_back(Move {
                    to: v0,
                    ops: vec![Op::AttachNear(6.0), Op::Release],
                    dwell: 0.0,
                });
                self.stage = Stage::Radii;
            }
            Stage::Radii => {
                let (mid, gap) = self.largest_gap();
                let min_gap = TAU / self.target_radii as f32;
                if gap < min_gap * 1.15 || self.radii.len() >= RADII.1 {
                    self.stage = Stage::Auxiliary;
                    self.spiral_i = 0;
                    self.spiral_n = (AUX_TURNS * self.radii.len() as f32) as usize;
                    return self.plan(world, rng);
                }
                // Into the largest gap, a little off its centre so the
                // wheel is not a protractor.
                let angle = mid + rng.range(-0.12, 0.12) * gap;
                self.queue_radius(angle);
            }
            Stage::Auxiliary => {
                let order = self.sorted_radii();
                let n = order.len();
                if self.spiral_i == 0 {
                    self.queue.push_back(Move::new(
                        hub,
                        Op::StartLine {
                            kind: ThreadKind::Auxiliary,
                            radius: 8.0,
                        },
                    ));
                }
                if self.spiral_i >= self.spiral_n {
                    self.queue.push_back(Move::new(hub, Op::Release));
                    self.stage = Stage::Capture;
                    self.spiral_i = 0;
                    self.spiral_n = (self.capture_turns * n as f32) as usize;
                    return;
                }
                // A batch of crossings per plan call.
                for _ in 0..n {
                    if self.spiral_i >= self.spiral_n {
                        break;
                    }
                    let angle = order[self.spiral_i % n];
                    let f = self.frame_point(angle);
                    let reach = hypot(f.x - hub.x, f.y - hub.y);
                    let frac = self.spiral_i as f32 / self.spiral_n as f32;
                    let r = (0.12 + 0.80 * frac) * reach;
                    self.queue.push_back(Move::new(
                        self.point_at(angle, r),
                        Op::AttachOn {
                            kind: Some(ThreadKind::Radius),
                            radius: 7.0,
                        },
                    ));
                    self.spiral_i += 1;
                }
            }
            Stage::Capture => {
                let order = self.sorted_radii();
                let n = order.len();
                if self.spiral_i >= self.spiral_n {
                    self.queue.push_back(Move {
                        to: hub,
                        ops: vec![
                            Op::Release,
                            Op::CutKindBeyond {
                                kind: ThreadKind::Auxiliary,
                                center: hub,
                                min_dist: 0.0,
                            },
                        ],
                        dwell: 0.0,
                    });
                    self.stage = Stage::Finish;
                    return;
                }
                for _ in 0..n {
                    if self.spiral_i >= self.spiral_n {
                        break;
                    }
                    // Inward, the other way round from the scaffold.
                    let angle = order[n - 1 - (self.spiral_i % n)];
                    let f = self.frame_point(angle);
                    let reach = hypot(f.x - hub.x, f.y - hub.y);
                    let frac = self.spiral_i as f32 / self.spiral_n as f32;
                    let r = (0.88 - (0.88 - FREE_ZONE) * frac) * reach;
                    let at = self.point_at(angle, r);
                    let mut ops = Vec::new();
                    if self.spiral_i == 0 {
                        ops.push(Op::StartOn {
                            kind: ThreadKind::Capture,
                            on: Some(ThreadKind::Radius),
                            radius: 7.0,
                        });
                    } else {
                        ops.push(Op::AttachOn {
                            kind: Some(ThreadKind::Radius),
                            radius: 7.0,
                        });
                    }
                    // Once a turn, the scaffold outside the capture spiral
                    // has served its purpose and is cut away.
                    if self.spiral_i % n == n - 1 {
                        ops.push(Op::CutKindBeyond {
                            kind: ThreadKind::Auxiliary,
                            center: hub,
                            min_dist: r * 1.02,
                        });
                    }
                    self.queue.push_back(Move { to: at, ops, dwell: 0.0 });
                    self.spiral_i += 1;
                }
            }
            Stage::Finish => {
                self.queue.push_back(Move::walk(hub));
                self.stage = Stage::Done;
            }
            Stage::Done | Stage::Repair => {
                if self.queue.is_empty() {
                    self.stage = Stage::Done;
                }
            }
        }
        let _ = PI;
    }
}

impl WebProgram for OrbProgram {
    fn species(&self) -> Species {
        Species::Araneus
    }

    fn next(&mut self, _silk: &Silk, world: &Anchors, rng: &mut Pcg32, _pos: Vec2) -> Option<Move> {
        if self.queue.is_empty() && self.stage != Stage::Done {
            self.plan(world, rng);
        }
        self.queue.pop_front()
    }

    fn sit_point(&self, _silk: &Silk) -> Option<Vec2> {
        if matches!(self.stage, Stage::Explore | Stage::Bridge) {
            None
        } else {
            Some(self.hub)
        }
    }

    fn progress(&self) -> f32 {
        match self.stage {
            Stage::Explore => 0.0,
            Stage::Bridge => 0.03,
            Stage::Y => 0.06,
            Stage::Frame => 0.10,
            Stage::Radii => 0.15 + 0.20 * (self.radii.len() as f32 / self.target_radii as f32).min(1.0),
            Stage::Auxiliary => 0.35 + 0.15 * (self.spiral_i as f32 / self.spiral_n.max(1) as f32),
            Stage::Capture => 0.50 + 0.48 * (self.spiral_i as f32 / self.spiral_n.max(1) as f32),
            Stage::Finish => 0.99,
            Stage::Done | Stage::Repair => 1.0,
        }
    }

    fn stage(&self) -> &'static str {
        match self.stage {
            Stage::Explore => "exploring",
            Stage::Bridge => "bridge",
            Stage::Y => "proto-hub",
            Stage::Frame => "frame",
            Stage::Radii => "radii",
            Stage::Auxiliary => "auxiliary spiral",
            Stage::Capture => "capture spiral",
            Stage::Finish => "finishing",
            Stage::Done => "complete",
            Stage::Repair => "repairing",
        }
    }

    fn complete(&self) -> bool {
        self.stage == Stage::Done && self.queue.is_empty()
    }

    fn reset(&mut self, world: &Anchors, rng: &mut Pcg32) {
        let region: Region = world.bounds;
        let (w, h) = region.size;
        self.radius = (RADIUS_FRACTION * w.min(h)).clamp(RADIUS_BOUNDS.0, RADIUS_BOUNDS.1);
        let (hw, hh) = region.half();
        // Somewhere the whole wheel fits, a little off centre.
        let jx = (hw - self.radius - 2.0 * WALL_INSET).max(0.0);
        let jy = (hh - self.radius - 2.0 * WALL_INSET).max(0.0);
        self.hub = Vec2::new(
            region.center.x + rng.range(-0.5, 0.5) * jx,
            region.center.y + rng.range(-0.5, 0.5) * jy,
        );
        self.frame.clear();
        self.anchors.clear();
        self.stage = Stage::Explore;
        self.queue.clear();
        self.radii.clear();
        self.target_radii = rng.int_range(RADII.0 as i64, RADII.1 as i64) as usize;
        self.capture_turns = rng.range(CAPTURE_TURNS.0, CAPTURE_TURNS.1);
        self.spiral_i = 0;
        self.spiral_n = 0;
        self.explored = 0;
        self.explore_target = rng.int_range(1, 3) as u32;
        self.capture_full = 0.0;
    }

    fn on_damage(&mut self, silk: &Silk) -> bool {
        if self.stage != Stage::Done && self.stage != Stage::Repair {
            // Mid-build: whatever was cut, the plan keeps going; the ops fall
            // back to fresh nodes where a thread is missing.
            return false;
        }
        if self.capture_full <= 0.0 {
            self.capture_full = silk.length_of_kind(ThreadKind::Capture).max(1.0);
        }
        let missing: Vec<f32> = self
            .radii
            .iter()
            .copied()
            .filter(|&a| {
                // Probed at several points between hub and frame: a radius
                // cut anywhere along its length is a radius to re-lay.
                let f = self.frame_point(a);
                let reach = hypot(f.x - self.hub.x, f.y - self.hub.y);
                let mut r = reach * 0.1;
                let mut gap = false;
                while r < reach * 0.95 {
                    let p = self.point_at(a, r);
                    if silk.thread_near(p, Some(ThreadKind::Radius), 6.0).is_none() {
                        gap = true;
                        break;
                    }
                    r += 8.0;
                }
                gap
            })
            .collect();
        let frame_gone = (0..self.frame.len()).any(|k| {
            let a = self.frame[k];
            let b = self.frame[(k + 1) % self.frame.len()];
            let m = Vec2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
            silk.thread_near(m, Some(ThreadKind::Frame), 6.0).is_none()
        });
        let capture_left = silk.length_of_kind(ThreadKind::Capture) / self.capture_full;
        if frame_gone || missing.len() * 5 > self.radii.len() * 2 || capture_left < 0.5 {
            return true;
        }
        if !missing.is_empty() {
            self.stage = Stage::Repair;
            let mut radii = std::mem::take(&mut self.radii);
            radii.retain(|a| !missing.contains(a));
            self.radii = radii;
            for a in missing {
                self.queue_radius(a);
            }
        }
        false
    }

    fn rebuilds_daily(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signals::BrainSignals;
    use crate::weaver::{Weaver, WeaverState};

    const TANK: Region = Region {
        center: Vec2 { x: 180.0, y: -90.0 },
        size: (720.0, 520.0),
    };
    const DT: f32 = 1.0 / 60.0;

    fn araneus(seed: u64) -> Weaver {
        let mut rng = Pcg32::new(seed);
        let program = OrbProgram::new(&Anchors::enclosure(TANK), &mut rng);
        Weaver::new(Species::Araneus, TANK.center, seed, Box::new(program))
    }

    /// Run with a steady walk drive until the program reports completion.
    fn build(w: &mut Weaver, max_secs: f32) -> f32 {
        let mut s = BrainSignals::new();
        s.walk_drive = 0.6;
        let mut t = 0.0;
        while !w.web_complete() && t < max_secs {
            w.update(DT, TANK, None, Some(s));
            t += DT;
        }
        // A few frames more, so the body has settled into what it does once
        // there is nothing left to build.
        for _ in 0..30 {
            w.update(DT, TANK, None, Some(s));
        }
        t
    }

    #[test]
    fn a_full_orb_is_built_in_order_and_in_the_species_band() {
        let mut w = araneus(11);
        let mut seen: Vec<&'static str> = Vec::new();
        let mut s = BrainSignals::new();
        s.walk_drive = 0.6;
        let mut t = 0.0;
        while !w.web_complete() && t < 900.0 {
            let st = w.program.stage();
            if seen.last() != Some(&st) {
                seen.push(st);
            }
            w.update(DT, TANK, None, Some(s));
            t += DT;
        }
        assert!(w.web_complete(), "not finished after {t:.0} s: {}", w.program.stage());
        assert!(t < 600.0, "took {t:.0} s; the tempo is meant to be minutes");
        for _ in 0..30 {
            w.update(DT, TANK, None, Some(s));
        }
        let order = [
            "exploring",
            "bridge",
            "proto-hub",
            "frame",
            "radii",
            "auxiliary spiral",
            "capture spiral",
            "finishing",
            "complete",
        ];
        let mut i = 0;
        for st in &seen {
            while i < order.len() && order[i] != *st {
                i += 1;
            }
            assert!(i < order.len(), "stage {st} out of order: {seen:?}");
        }
        let silk = &w.silk;
        assert!(silk.count_kind(ThreadKind::Bridge) >= 1);
        assert!(silk.count_kind(ThreadKind::Frame) >= FRAME_SIDES);
        // Radii: one chain per angle, split many times by the spirals.
        let (lo, hi) = RADII;
        let n = w.program.stage();
        let _ = n;
        let prog = &w.program;
        assert!(prog.progress() >= 1.0);
        assert!(silk.count_kind(ThreadKind::Capture) > 200, "capture segments {}", silk.count_kind(ThreadKind::Capture));
        assert_eq!(silk.count_kind(ThreadKind::Auxiliary), 0, "the scaffold is cut away");
        assert!(silk.trailing_anchor().is_none());
        // The capture spiral stays inside the frame and outside the free zone.
        let hub = w.sit_point().unwrap();
        let mut rmin = f32::MAX;
        let mut rmax = 0.0f32;
        for t in silk.threads.iter().filter(|t| t.kind == ThreadKind::Capture) {
            for n in [t.a, t.b] {
                let p = silk.nodes[n].pos;
                let r = hypot(p.x - hub.x, p.y - hub.y);
                rmin = rmin.min(r);
                rmax = rmax.max(r);
            }
        }
        assert!(rmin > 0.1 * 110.0, "free zone: {rmin}");
        assert!(rmax < 300.0 + 1.0, "inside the frame: {rmax}");
        // Radius count, read off the silk rather than the program.
        let radii_at_hub = silk
            .threads
            .iter()
            .filter(|t| t.kind == ThreadKind::Radius)
            .filter(|t| {
                let (a, b) = (silk.nodes[t.a].pos, silk.nodes[t.b].pos);
                hypot(a.x - hub.x, a.y - hub.y) < 3.0 || hypot(b.x - hub.x, b.y - hub.y) < 3.0
            })
            .count();
        assert!((lo..=hi + 2).contains(&radii_at_hub), "radii at hub {radii_at_hub}");
        assert_eq!(w.state, WeaverState::Sitting);
        assert!(hypot(w.pos.x - hub.x, w.pos.y - hub.y) < 3.0, "sits at the hub");
    }

    #[test]
    fn the_web_stays_inside_the_region_and_on_the_walls() {
        let mut w = araneus(5);
        build(&mut w, 900.0);
        let (lo, hi) = (TANK.min(), TANK.max());
        for n in &w.silk.nodes {
            assert!(n.pos.x >= lo.x - 0.5 && n.pos.x <= hi.x + 0.5, "{:?}", n.pos);
            assert!(n.pos.y >= lo.y - 0.5 && n.pos.y <= hi.y + 0.5, "{:?}", n.pos);
        }
        let fixed = w.silk.nodes.iter().filter(|n| n.anchor == crate::silk::Anchor::Fixed).count();
        assert!(fixed >= FRAME_SIDES, "guyed to the walls: {fixed} fixed nodes");
    }

    #[test]
    fn construction_pauses_when_the_walk_drive_is_down() {
        let mut w = araneus(2);
        let mut on = BrainSignals::new();
        on.walk_drive = 0.6;
        for _ in 0..600 {
            w.update(DT, TANK, None, Some(on));
        }
        let before = w.silk.threads.len();
        let off = BrainSignals::new(); // walk_drive 0
        for _ in 0..600 {
            w.update(DT, TANK, None, Some(off));
        }
        assert_eq!(w.silk.threads.len(), before, "no thread is laid while the brain says rest");
        assert!(w.speed.abs() < 1e-3);
    }

    #[test]
    fn cutting_a_few_radii_is_repaired_and_cutting_half_the_web_is_a_rebuild() {
        let mut w = araneus(7);
        build(&mut w, 900.0);
        let hub = w.sit_point().unwrap();
        let radius = 0.5 * 110.0;
        // One sweep across one side, well inside the capture zone.
        let cut = w.damage(Vec2::new(hub.x + radius, hub.y), 12.0);
        assert!(cut > 0);
        assert_eq!(w.program.stage(), "repairing");
        let t = build(&mut w, 300.0);
        assert!(w.web_complete(), "repair finished in {t:.0} s");
        assert_eq!(w.state, WeaverState::Sitting);
        // Now most of it.
        let mut w2 = araneus(7);
        build(&mut w2, 900.0);
        let hub = w2.sit_point().unwrap();
        for k in 0..12 {
            let a = k as f32 * 0.5;
            w2.damage(Vec2::new(hub.x + a.cos() * 120.0, hub.y + a.sin() * 120.0), 60.0);
        }
        assert_eq!(w2.state, WeaverState::Eating, "beyond repair: taken down");
        let t = build(&mut w2, 1500.0);
        assert!(w2.web_complete(), "rebuilt in {t:.0} s: {}", w2.program.stage());
    }

    #[test]
    fn building_never_excites_the_silk() {
        // The construction program has no neurons in it, and it does not
        // drive the ones there are either: laying a thousand threads leaves
        // the vibration field at exactly zero, so nothing reaches the
        // mechanosensory partners from the program's own activity.
        let mut w = araneus(3);
        let mut s = BrainSignals::new();
        s.walk_drive = 0.6;
        let mut t = 0.0;
        while !w.web_complete() && t < 900.0 {
            w.update(DT, TANK, None, Some(s));
            assert_eq!(w.felt, 0.0);
            assert!(w.silk.loudest(0.0).is_none());
            t += DT;
        }
        assert!(w.web_complete());
    }
}
