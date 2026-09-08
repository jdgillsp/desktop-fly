//! *Agelenopsis*: the sheet web with a funnel, as a walked path (WEB_PLAN.md
//! §5.3).
//!
//! The primary source is Rojas (2011), *Sheet-web construction by Melpomene
//! sp. (Araneae: Agelenidae)*, J. Arachnol. 39: 189–193, the one direct
//! observation of agelenid sheet building. Its abstract: construction
//! "consisted basically of two alternating behaviors: laying support threads
//! and the filling in the sheet", "repeated during several construction
//! sessions until the available area was filled". The paper's sections name
//! the two movements — a *bee line* movement for the support threads and a
//! *sheet filling* movement — and a resting phase between them. **The full
//! text is paywalled**: the order and the alternation here are the abstract's;
//! the shape of the filling path (a meander that favours the least-covered
//! part of the sheet) is this program's, not the paper's, and WEB_PLAN.md §12
//! says so.
//!
//! What the family accounts add and this program keeps: the funnel is a tube
//! of dense silk at one edge, the spider waits at its mouth with its front
//! legs on the sheet, the sheet is non-sticky and *thickens* with use rather
//! than being rebuilt, and the run to prey is very fast. Threat response is
//! into the funnel, never a drop (`weaver::Traits`).

use std::collections::VecDeque;

use crate::creature::Weaver as Species;
use crate::anchors::Anchors;
use crate::habitat::Region;
use crate::rng::Pcg32;
use crate::silk::{Anchor, Silk, ThreadKind};
use crate::util::Vec2;
use crate::weaver::{Move, Op, WebProgram};

/// Construction sessions in the first build; each is one bee-line bout and
/// one filling bout.
pub const SESSIONS: usize = 4;
/// Support (bee-line) threads per session.
pub const SUPPORTS_PER_SESSION: usize = 3;
/// Filling moves per session.
pub const FILL_MOVES: usize = 12;
/// Coverage grid over the sheet area, for choosing where to fill next.
pub const GRID: (usize, usize) = (6, 4);
/// Seconds resting in the funnel between bouts.
pub const FUNNEL_DWELL: f32 = 2.0;
pub const WALL_INSET: f32 = 14.0;
/// The sheet's extent as fractions of the region.
pub const SHEET_W: f32 = 0.55;
pub const SHEET_H: f32 = 0.45;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    SeekingSite,
    Funnel,
    Supports,
    Filling,
    Done,
    Adding,
}

pub struct FunnelProgram {
    /// Where the funnel meets the wall, and its mouth on the sheet side.
    wall: Vec2,
    mouth: Vec2,
    /// Sheet rectangle.
    lo: Vec2,
    hi: Vec2,
    /// Which wall the funnel is on: -1 left, +1 right.
    side: f32,
    stage: Stage,
    queue: VecDeque<Move>,
    sessions_done: usize,
    sessions_target: usize,
    in_session_fill: bool,
    /// Threads laid per grid cell.
    coverage: Vec<u32>,
}

impl FunnelProgram {
    pub fn new(world: &Anchors, rng: &mut Pcg32) -> Self {
        let region = world.bounds;
        let mut p = FunnelProgram {
            wall: region.center,
            mouth: region.center,
            lo: region.min(),
            hi: region.max(),
            side: 1.0,
            stage: Stage::Funnel,
            queue: VecDeque::new(),
            sessions_done: 0,
            sessions_target: SESSIONS,
            in_session_fill: false,
            coverage: vec![0; GRID.0 * GRID.1],
        };
        p.reset(world, rng);
        p
    }

    pub fn mouth(&self) -> Vec2 {
        self.mouth
    }
    pub fn stage_enum(&self) -> Stage {
        self.stage
    }

    fn cell_of(&self, p: Vec2) -> Option<usize> {
        let w = self.hi.x - self.lo.x;
        let h = self.hi.y - self.lo.y;
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let cx = (((p.x - self.lo.x) / w) * GRID.0 as f32).floor();
        let cy = (((p.y - self.lo.y) / h) * GRID.1 as f32).floor();
        if cx < 0.0 || cy < 0.0 || cx >= GRID.0 as f32 || cy >= GRID.1 as f32 {
            return None;
        }
        Some(cy as usize * GRID.0 + cx as usize)
    }

    fn cell_centre(&self, i: usize) -> Vec2 {
        let (cx, cy) = ((i % GRID.0) as f32, (i / GRID.0) as f32);
        let w = (self.hi.x - self.lo.x) / GRID.0 as f32;
        let h = (self.hi.y - self.lo.y) / GRID.1 as f32;
        Vec2::new(self.lo.x + (cx + 0.5) * w, self.lo.y + (cy + 0.5) * h)
    }

    /// The least-covered cell, ties broken at random.
    fn thinnest_cell(&self, rng: &mut Pcg32) -> usize {
        let min = *self.coverage.iter().min().unwrap_or(&0);
        let cands: Vec<usize> = (0..self.coverage.len()).filter(|&i| self.coverage[i] == min).collect();
        cands[rng.int_range(0, cands.len() as i64 - 1) as usize]
    }

    fn queue_supports(&mut self, world: &Anchors, rng: &mut Pcg32) {
        let m = self.mouth;
        let region = world.bounds;
        let rlo = region.min();
        let rhi = region.max();
        for _ in 0..SUPPORTS_PER_SESSION {
            // A bee line: straight from the mouth to an anchor beyond the
            // sheet — whatever is there in that direction, or the far
            // corners of the area.
            let (target, outward) = match rng.int_range(0, 3) {
                0 => (
                    Vec2::new(
                        if self.side > 0.0 { rlo.x + WALL_INSET } else { rhi.x - WALL_INSET },
                        rng.range(self.lo.y, self.hi.y),
                    ),
                    true,
                ),
                1 => (Vec2::new(rng.range(self.lo.x, self.hi.x), rhi.y - WALL_INSET), true),
                2 => (Vec2::new(rng.range(self.lo.x, self.hi.x), rlo.y + WALL_INSET), true),
                _ => (
                    Vec2::new(
                        if self.side > 0.0 { self.lo.x } else { self.hi.x },
                        if rng.f32() < 0.5 { self.lo.y } else { self.hi.y },
                    ),
                    false,
                ),
            };
            let target = if outward {
                world.anchor_toward(m, target, WALL_INSET)
            } else {
                target
            };
            let fixed = (target.x - rlo.x).abs() < WALL_INSET + 0.5
                || (rhi.x - target.x).abs() < WALL_INSET + 0.5
                || (target.y - rlo.y).abs() < WALL_INSET + 0.5
                || (rhi.y - target.y).abs() < WALL_INSET + 0.5
                || world.on(target, WALL_INSET + 3.0).is_some();
            self.queue.push_back(Move::new(
                m,
                Op::StartLine {
                    kind: ThreadKind::Sheet,
                    radius: 10.0,
                },
            ));
            self.queue.push_back(Move {
                to: target,
                ops: vec![
                    if fixed { Op::Attach(Anchor::Fixed) } else { Op::AttachOn { kind: None, radius: 10.0 } },
                    Op::Release,
                ],
                dwell: 0.0,
            });
            if let Some(c) = self.cell_of(target) {
                self.coverage[c] += 1;
            }
        }
        self.queue.push_back(Move {
            to: m,
            ops: Vec::new(),
            dwell: FUNNEL_DWELL * 0.5,
        });
    }

    fn queue_filling(&mut self, rng: &mut Pcg32) {
        let m = self.mouth;
        self.queue.push_back(Move::new(
            m,
            Op::StartLine {
                kind: ThreadKind::Sheet,
                radius: 10.0,
            },
        ));
        for _ in 0..FILL_MOVES {
            // Meander toward whichever part of the sheet is thinnest, with
            // some scatter so the mesh is a mesh and not a grid.
            let cell = self.thinnest_cell(rng);
            let c = self.cell_centre(cell);
            let w = (self.hi.x - self.lo.x) / GRID.0 as f32;
            let h = (self.hi.y - self.lo.y) / GRID.1 as f32;
            let p = Vec2::new(
                (c.x + rng.range(-0.45, 0.45) * w).clamp(self.lo.x, self.hi.x),
                (c.y + rng.range(-0.45, 0.45) * h).clamp(self.lo.y, self.hi.y),
            );
            self.queue.push_back(Move::new(p, Op::AttachOn { kind: None, radius: 9.0 }));
            self.coverage[cell] += 1;
        }
        self.queue.push_back(Move {
            to: m,
            ops: vec![Op::AttachNear(10.0), Op::Release],
            dwell: FUNNEL_DWELL,
        });
    }

    fn plan(&mut self, world: &Anchors, rng: &mut Pcg32) {
        match self.stage {
            Stage::SeekingSite => {},
            Stage::Funnel => {
                // A tube of dense silk from the wall to the mouth: zigzag
                // attachments along both sides of the tube.
                let w = self.wall;
                let m = self.mouth;
                self.queue.push_back(Move::new(w, Op::PayOut(ThreadKind::Retreat)));
                let n = 8;
                for k in 0..=n {
                    let t = k as f32 / n as f32;
                    let across = if k % 2 == 0 { 9.0 } else { -9.0 };
                    let p = Vec2::new(w.x + (m.x - w.x) * t, w.y + (m.y - w.y) * t + across);
                    let op = if k == 0 || k == n {
                        Op::Attach(Anchor::Free)
                    } else if k % 4 == 1 {
                        Op::Attach(Anchor::Fixed)
                    } else {
                        Op::Attach(Anchor::Free)
                    };
                    self.queue.push_back(Move::new(p, op));
                }
                self.queue.push_back(Move {
                    to: m,
                    ops: vec![Op::AttachNear(8.0), Op::Release],
                    dwell: FUNNEL_DWELL,
                });
                self.stage = Stage::Supports;
            }
            Stage::Supports => {
                self.queue_supports(world, rng);
                self.stage = Stage::Filling;
            }
            Stage::Filling => {
                self.queue_filling(rng);
                self.sessions_done += 1;
                self.stage = if self.sessions_done >= self.sessions_target {
                    self.queue.push_back(Move::walk(self.mouth));
                    Stage::Done
                } else {
                    Stage::Supports
                };
            }
            Stage::Done | Stage::Adding => {
                if self.queue.is_empty() {
                    self.stage = Stage::Done;
                }
            }
        }
        let _ = self.in_session_fill;
    }

    /// How many sheet threads the coverage grid has seen, for tests.
    pub fn coverage_total(&self) -> u32 {
        self.coverage.iter().sum()
    }
    pub fn thinnest_coverage(&self) -> u32 {
        *self.coverage.iter().min().unwrap_or(&0)
    }
}

impl WebProgram for FunnelProgram {
    fn species(&self) -> Species {
        Species::Agelenopsis
    }

    fn next(&mut self, _silk: &Silk, world: &Anchors, rng: &mut Pcg32, _pos: Vec2) -> Option<Move> {
        if self.stage==Stage::SeekingSite {self.reset(world,rng); if self.stage==Stage::SeekingSite{return None;}}
        if self.queue.is_empty() && self.stage != Stage::Done {
            self.plan(world, rng);
        }
        self.queue.pop_front()
    }

    fn sit_point(&self, _silk: &Silk) -> Option<Vec2> {
        // Just inside the mouth, front legs on the sheet.
        Some(Vec2::new(
            self.mouth.x - self.side * 6.0,
            self.mouth.y,
        ))
    }

    fn progress(&self) -> f32 {
        match self.stage {
            Stage::Funnel => 0.0,
            Stage::Done => 1.0,
            _ => (0.1 + 0.9 * self.sessions_done as f32 / self.sessions_target as f32).min(0.99),
        }
    }

    fn stage(&self) -> &'static str {
        match self.stage {
            Stage::Funnel => "funnel",
            Stage::SeekingSite => "seeking a supported site",
            Stage::Supports => "support threads",
            Stage::Filling => "filling the sheet",
            Stage::Done => "complete",
            Stage::Adding => "adding",
        }
    }

    fn complete(&self) -> bool {
        self.stage == Stage::Done && self.queue.is_empty()
    }

    fn reset(&mut self, world: &Anchors, rng: &mut Pcg32) {
        let region: Region = world.bounds;
        let rlo = region.min();
        let rhi = region.max();
        let (w, h) = region.size;
        self.side = if rng.f32() < 0.5 { -1.0 } else { 1.0 };
        // A sheet/funnel belongs low beside shelter, rather than halfway up
        // the open enclosure. Desktop frames remain the available substrate.
        let jitter = rng.range(-0.25, 0.25);
        let y = if world.is_enclosure() {
            rlo.y + h * (0.28 + jitter * 0.16)
        } else { region.center.y + jitter * h };
        let wall_x = if self.side > 0.0 { rhi.x - WALL_INSET } else { rlo.x + WALL_INSET };
        // The funnel is against whatever is really there on that side: the
        // wall, or on the desktop the nearest window edge or the screen.
        self.wall = world.anchor_toward(Vec2::new(region.center.x, y), Vec2::new(wall_x, y), WALL_INSET);
        let wall_x = self.wall.x;
        let tube = (0.09 * w).clamp(30.0, 70.0);
        self.mouth = Vec2::new(wall_x - self.side * tube, y);
        let sw = SHEET_W * w;
        let sh = SHEET_H * h;
        let (x0, x1) = if self.side > 0.0 {
            (self.mouth.x - sw, self.mouth.x)
        } else {
            (self.mouth.x, self.mouth.x + sw)
        };
        self.lo = Vec2::new(x0.max(rlo.x + WALL_INSET), (y - sh * 0.5).max(rlo.y + WALL_INSET));
        self.hi = Vec2::new(x1.min(rhi.x - WALL_INSET), (y + sh * 0.5).min(rhi.y - WALL_INSET));
        if self.hi.x - self.lo.x < tube || self.hi.y - self.lo.y < 20.0 {
            // A real UI support can lie outside the proposed planning patch.
            // Re-site the sheet beside that support within the actual world;
            // never clamp into an inverted rectangle or invent an anchor.
            let boundary = world.structures.iter().find(|f| f.id == crate::anchors::SCREEN || f.id == crate::anchors::WALLS)
                .copied().unwrap_or(crate::anchors::Frame::of(region,crate::anchors::WALLS));
            for side in [self.side,-self.side] {
                let mouth = Vec2::new(self.wall.x-side*tube,y);
                let (a,b) = if side>0.0 {(mouth.x-sw,mouth.x)} else {(mouth.x,mouth.x+sw)};
                let lo=Vec2::new(a.max(boundary.lo.x+WALL_INSET),(y-sh*0.5).max(boundary.lo.y+WALL_INSET));
                let hi=Vec2::new(b.min(boundary.hi.x-WALL_INSET),(y+sh*0.5).min(boundary.hi.y-WALL_INSET));
                if hi.x-lo.x>=tube && hi.y-lo.y>=20.0 {
                    self.side=side; self.mouth=mouth; self.lo=lo; self.hi=hi;
                    break;
                }
            }
        }
        self.stage = Stage::Funnel;
        self.queue.clear();
        self.sessions_done = 0;
        self.sessions_target = SESSIONS;
        self.in_session_fill = false;
        self.coverage = vec![0; GRID.0 * GRID.1];
        if self.lo.x>=self.hi.x || self.lo.y>=self.hi.y {
            // No feasible sheet can fit. Wait for the world to change.
            self.stage=Stage::SeekingSite;
        }
    }

    fn on_damage(&mut self, silk: &Silk) -> bool {
        if self.stage != Stage::Done && self.stage != Stage::Adding {
            return false;
        }
        if silk.thread_near(self.mouth, Some(ThreadKind::Retreat), 16.0).is_none() {
            return true;
        }
        // A hole is crossed over and re-threaded as part of the next filling
        // bout; there is no separate repair, which is itself the animal's way.
        let mut rng = Pcg32::new(silk.threads.len() as u64);
        self.queue_filling(&mut rng);
        self.stage = Stage::Adding;
        false
    }

    fn rebuilds_daily(&self) -> bool {
        false
    }

    fn nightly(&mut self, _silk: &Silk, world: &Anchors, rng: &mut Pcg32) {
        // One more session: the sheet thickens.
        self.queue_supports(world, rng);
        self.queue_filling(rng);
        self.queue.push_back(Move::walk(self.mouth));
        self.stage = Stage::Adding;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signals::BrainSignals;
    use crate::util::hypot;
    use crate::weaver::{Weaver, WeaverState};

    const TANK: Region = Region {
        center: Vec2 { x: 100.0, y: -40.0 },
        size: (720.0, 520.0),
    };
    const DT: f32 = 1.0 / 60.0;

    fn grass_spider(seed: u64) -> Weaver {
        let mut rng = Pcg32::new(seed);
        let program = FunnelProgram::new(&Anchors::enclosure(TANK), &mut rng);
        Weaver::new(Species::Agelenopsis, TANK.center, seed, Box::new(program))
    }

    fn build(w: &mut Weaver, max_secs: f32) -> f32 {
        let mut s = BrainSignals::new();
        s.walk_drive = 0.6;
        let mut t = 0.0;
        while !w.web_complete() && t < max_secs {
            w.update(DT, TANK, None, Some(s));
            t += DT;
        }
        for _ in 0..30 {
            w.update(DT, TANK, None, Some(s));
        }
        t
    }

    #[test]
    fn funnel_first_then_alternating_supports_and_filling_and_the_sheet_thickens() {
        let mut w = grass_spider(4);
        let mut s = BrainSignals::new();
        s.walk_drive = 0.6;
        let mut stages: Vec<&'static str> = Vec::new();
        let mut sheet_counts: Vec<usize> = Vec::new();
        let mut t = 0.0;
        while !w.web_complete() && t < 900.0 {
            let st = w.program.stage();
            if stages.last() != Some(&st) {
                stages.push(st);
                sheet_counts.push(w.silk.count_kind(ThreadKind::Sheet));
            }
            w.update(DT, TANK, None, Some(s));
            t += DT;
        }
        assert!(w.web_complete(), "unfinished at {t:.0} s: {}", w.program.stage());
        assert!(t < 600.0, "took {t:.0} s");
        for _ in 0..30 {
            w.update(DT, TANK, None, Some(s));
        }
        assert_eq!(stages[0], "funnel");
        assert!(w.silk.count_kind(ThreadKind::Retreat) >= 8, "a tube of dense silk");
        // Supports and filling alternate, SESSIONS times each.
        let supports = stages.iter().filter(|s| **s == "support threads").count();
        let fills = stages.iter().filter(|s| **s == "filling the sheet").count();
        assert_eq!(supports, SESSIONS, "{stages:?}");
        assert_eq!(fills, SESSIONS, "{stages:?}");
        for k in 1..stages.len() - 1 {
            if stages[k] == "support threads" {
                assert_eq!(stages[k + 1], "filling the sheet", "{stages:?}");
            }
        }
        // Monotone: the sheet only ever gets denser.
        assert!(sheet_counts.windows(2).all(|p| p[1] >= p[0]), "{sheet_counts:?}");
        assert!(w.silk.count_kind(ThreadKind::Sheet) > 40);
        assert_eq!(w.silk.count_kind(ThreadKind::Capture), 0, "nothing sticky");
        assert_eq!(w.state, WeaverState::Sitting);
    }

    #[test]
    fn prey_on_the_sheet_is_rushed_and_a_threat_sends_it_into_the_funnel() {
        let mut w = grass_spider(5);
        build(&mut w, 900.0);
        let mouth = w.sit_point().unwrap();
        // A bug that lands on the sheet: it is slowed and shakes the sheet.
        let target = {
            let th = w.silk.threads.iter().find(|t| t.kind == ThreadKind::Sheet).unwrap();
            let (a, b) = (w.silk.nodes[th.a].pos, w.silk.nodes[th.b].pos);
            Vec2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
        };
        w.spawn_bug(target, Vec2::new(5.0, 0.0), false);
        let calm = BrainSignals::new();
        for _ in 0..60 {
            w.update(DT, TANK, None, Some(calm));
        }
        assert!(w.silk.loudest(0.001).is_some(), "the sheet is shaken");
        let mut s = BrainSignals::new();
        s.strike = true;
        w.update(DT, TANK, None, Some(s));
        assert_eq!(w.state, WeaverState::Hunting, "loudest {:?} pos {:?}", w.silk.loudest(0.0), w.pos);
        let mut peak = 0.0f32;
        for _ in 0..60 * 3 {
            w.update(DT, TANK, None, Some(calm));
            peak = peak.max(w.speed);
            if w.state != WeaverState::Hunting {
                break;
            }
        }
        assert!(peak > 400.0, "the rush: peak {peak}");
        for _ in 0..60 * 10 {
            w.update(DT, TANK, None, Some(calm));
        }
        assert!(hypot(w.pos.x - mouth.x, w.pos.y - mouth.y) < 3.0, "back at the mouth: {:?}", w.pos);
        // A giant-fiber spike: into the funnel, never a drop.
        w.pos = target;
        w.state = WeaverState::Sitting;
        let mut e = BrainSignals::new();
        e.escape = true;
        w.update(DT, TANK, None, Some(e));
        assert_eq!(w.state, WeaverState::Retreating);
        assert!(w.dragline().is_none());
    }

    #[test]
    fn a_night_adds_a_session_and_a_hole_is_refilled_not_rebuilt() {
        let mut w = grass_spider(6);
        build(&mut w, 900.0);
        let before = w.silk.count_kind(ThreadKind::Sheet);
        w.hour = 20.0;
        w.web_age = 7.0 * 3600.0;
        let mut s = BrainSignals::new();
        s.walk_drive = 0.6;
        w.update(DT, TANK, None, Some(s));
        assert_ne!(w.state, WeaverState::Eating);
        build(&mut w, 300.0);
        assert!(w.silk.count_kind(ThreadKind::Sheet) > before);
        let mid = {
            // Cut the sheet away from the retreat; ordering does not guarantee
            // that the first sheet strand is clear of the funnel mouth.
            let home = w.sit_point().unwrap();
            let th = w.silk.threads.iter().filter(|t| t.kind == ThreadKind::Sheet).max_by(|a, b| {
                let distance = |t: &&crate::silk::Thread| {
                    let p = w.silk.nodes[t.a].pos;
                    let q = w.silk.nodes[t.b].pos;
                    crate::util::hypot((p.x + q.x) * 0.5 - home.x, (p.y + q.y) * 0.5 - home.y)
                };
                distance(a).total_cmp(&distance(b))
            }).unwrap();
            let (a, b) = (w.silk.nodes[th.a].pos, w.silk.nodes[th.b].pos);
            Vec2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
        };
        let after_night = w.silk.count_kind(ThreadKind::Sheet);
        let cut = w.damage(mid, 15.0);
        assert!(cut > 0);
        assert_ne!(w.state, WeaverState::Eating, "a hole is not a rebuild");
        build(&mut w, 300.0);
        assert!(w.silk.count_kind(ThreadKind::Sheet) >= after_night - cut + FILL_MOVES / 2);
    }
}
