//! *Parasteatoda tepidariorum*: the gumfoot tangle web as a walked path
//! (WEB_PLAN.md §5.2).
//!
//! The sequence is Benjamin & Zschokke's (2003) for this species, then
//! *Achaearanea tepidariorum*: a **central retreat**, a **tangle** built in
//! bouts with the spider returning to the retreat between them, a **sheet**
//! below the tangle with tangle and sheet bouts alternating, and the
//! **gumfoot lines** last — tensioned lines from the tangle straight down to
//! the floor, sticky at the foot. A theridiid web is not rebuilt: it stands,
//! and each night adds to it.
//!
//! What the mechanics of the foot do — the line snapping up with whatever
//! walked into it — lives in the body (`weaver.rs`), because it is contact,
//! not construction. This file only says where the threads go.

use std::collections::VecDeque;

use crate::creature::Weaver as Species;
use crate::anchors::Anchors;
use crate::habitat::Region;
use crate::rng::Pcg32;
use crate::silk::{Anchor, Silk, ThreadKind};
use crate::util::Vec2;
use crate::weaver::{Move, Op, WebProgram};

/// Tangle bouts in the first build, and how many lines each lays.
pub const TANGLE_BOUTS: usize = 4;
pub const LINES_PER_BOUT: usize = 4;
/// Sheet bouts in the first build.
pub const SHEET_BOUTS: usize = 2;
/// Gumfoot lines in the first build, and added each night.
pub const GUMFOOT_LINES: usize = 4;
pub const GUMFOOT_NIGHTLY: usize = 2;
/// Seconds the spider sits in the retreat between bouts.
pub const RETREAT_DWELL: f32 = 2.5;
/// How far inside the region's edge anchors sit.
pub const WALL_INSET: f32 = 14.0;
/// The tangle occupies this fraction of the region's height, from the top.
pub const TANGLE_DEPTH: f32 = 0.55;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Retreat,
    Tangle,
    Sheet,
    Gumfoot,
    Done,
    /// Nightly additions or repairs on a standing web.
    Adding,
}

pub struct CobwebProgram {
    retreat: Vec2,
    /// Half-width of the web, and the sheet's height.
    half_w: f32,
    sheet_y: f32,
    floor_y: f32,
    stage: Stage,
    queue: VecDeque<Move>,
    tangle_done: usize,
    sheet_done: usize,
    gumfoot_done: usize,
    gumfoot_target: usize,
    /// Which bout comes next while alternating.
    sheet_next: bool,
    /// Where the gumfoot lines were laid, so repairs can tell which are gone.
    feet: Vec<Vec2>,
    /// What the web was planned against, for re-laying after damage.
    world: Anchors,
}

impl CobwebProgram {
    pub fn new(world: &Anchors, rng: &mut Pcg32) -> Self {
        let mut p = CobwebProgram {
            retreat: world.bounds.center,
            half_w: 100.0,
            sheet_y: 0.0,
            floor_y: 0.0,
            stage: Stage::Retreat,
            queue: VecDeque::new(),
            tangle_done: 0,
            sheet_done: 0,
            gumfoot_done: 0,
            gumfoot_target: GUMFOOT_LINES,
            sheet_next: false,
            feet: Vec::new(),
            world: world.clone(),
        };
        p.reset(world, rng);
        p
    }

    pub fn retreat(&self) -> Vec2 {
        self.retreat
    }
    pub fn stage_enum(&self) -> Stage {
        self.stage
    }

    fn top_y(world: &Anchors) -> f32 {
        world.bounds.max().y - WALL_INSET
    }

    /// A point in the tangle volume: under the top, within the web's width.
    fn tangle_point(&self, world: &Anchors, rng: &mut Pcg32) -> Vec2 {
        let top = Self::top_y(world);
        world.bounds.clamp_inside(Vec2::new(
            rng.range((self.retreat.x - self.half_w).max(world.bounds.min().x + WALL_INSET),
                (self.retreat.x + self.half_w).min(world.bounds.max().x - WALL_INSET)),
            top - rng.range(0.08, 1.0) * (top - self.sheet_y),
        ), WALL_INSET)
    }

    /// An anchor in the upper part of the web: on the structure above or to
    /// either side, reached from the retreat. In an enclosure that is a point
    /// on the wall; on the desktop, the window edge or screen edge in that
    /// direction.
    fn upper_anchor(&self, world: &Anchors, rng: &mut Pcg32) -> Vec2 {
        let region = world.bounds;
        let lo = region.min();
        let hi = region.max();
        let top = Self::top_y(world);
        let target = match rng.int_range(0, 2) {
            0 => Vec2::new(
                (self.retreat.x + rng.range(-1.4, 1.4) * self.half_w).clamp(lo.x + WALL_INSET, hi.x - WALL_INSET),
                top,
            ),
            1 => Vec2::new(lo.x + WALL_INSET, top - rng.range(0.0, 0.8) * (top - self.sheet_y)),
            _ => Vec2::new(hi.x - WALL_INSET, top - rng.range(0.0, 0.8) * (top - self.sheet_y)),
        };
        world.anchor_toward(self.retreat, target, WALL_INSET)
    }

    fn queue_tangle_bout(&mut self, world: &Anchors, rng: &mut Pcg32) {
        let r = self.retreat;
        self.queue.push_back(Move::new(
            r,
            Op::StartLine {
                kind: ThreadKind::Tangle,
                radius: 10.0,
            },
        ));
        for k in 0..LINES_PER_BOUT {
            let to = if k == 0 || rng.f32() < 0.35 {
                (self.upper_anchor(world, rng), Op::Attach(Anchor::Fixed))
            } else {
                (self.tangle_point(world, rng), Op::AttachOn { kind: None, radius: 10.0 })
            };
            self.queue.push_back(Move::new(to.0, to.1));
        }
        // Home again, and a rest in the retreat: the repeated returns.
        self.queue.push_back(Move {
            to: r,
            ops: vec![Op::AttachNear(10.0), Op::Release],
            dwell: RETREAT_DWELL,
        });
        self.tangle_done += 1;
    }

    fn queue_sheet_bout(&mut self, world: &Anchors, rng: &mut Pcg32) {
        let r = self.retreat;
        let region = world.bounds;
        let lo = region.min();
        let hi = region.max();
        let left = (r.x - self.half_w * 1.1).max(lo.x + WALL_INSET);
        let right = (r.x + self.half_w * 1.1).min(hi.x - WALL_INSET);
        // Down from the retreat to sheet level, then a zigzag across it,
        // fixing to the tangle where it hangs low enough and to the walls at
        // the ends.
        let start = Vec2::new((r.x + rng.range(-0.3, 0.3) * self.half_w).clamp(left, right), self.sheet_y);
        self.queue.push_back(Move::new(
            start,
            Op::StartOn {
                kind: ThreadKind::Sheet,
                on: None,
                radius: 12.0,
            },
        ));
        let n = 7 + rng.int_range(0, 3) as usize;
        let mut x = start.x;
        let mut dir = if rng.f32() < 0.5 { 1.0 } else { -1.0 };
        for _ in 0..n {
            x += dir * rng.range(0.25, 0.6) * self.half_w;
            if x > right {
                x = right;
                dir = -1.0;
            } else if x < left {
                x = left;
                dir = 1.0;
            }
            let y = self.sheet_y + rng.range(-10.0, 10.0);
            let at_wall = x >= right - 0.5 || x <= left + 0.5;
            let op = if at_wall {
                Op::Attach(Anchor::Fixed)
            } else {
                Op::AttachOn { kind: None, radius: 10.0 }
            };
            // Where the sheet reaches the edge of its box, it is fixed to
            // whatever is really there in that direction.
            let toward = if x >= right - 0.5 { 1.0 } else { -1.0 };
            let at_bound = at_wall
                && ((toward > 0.0 && right >= hi.x - WALL_INSET - 0.5)
                    || (toward < 0.0 && left <= lo.x + WALL_INSET + 0.5));
            let at = if at_bound {
                world.anchor_toward(Vec2::new(x - toward * 10.0, y), Vec2::new(x, y), WALL_INSET)
            } else {
                Vec2::new(x, y)
            };
            self.queue.push_back(Move::new(at, op));
        }
        self.queue.push_back(Move {
            to: r,
            ops: vec![Op::AttachNear(10.0), Op::Release],
            dwell: RETREAT_DWELL,
        });
        self.sheet_done += 1;
    }

    fn queue_gumfoot(&mut self, world: &Anchors, rng: &mut Pcg32) {
        let r = self.retreat;
        let region = world.bounds;
        let lo = region.min();
        let hi = region.max();
        let x = rng.range((r.x - self.half_w).max(lo.x + WALL_INSET), (r.x + self.half_w).min(hi.x - WALL_INSET));
        let top = Vec2::new(x, self.sheet_y + rng.range(-4.0, 12.0));
        // Straight down to whatever the floor is: the enclosure's, or on the
        // desktop the top of a window below or the bottom of the screen.
        let foot = world.anchor_toward(top, Vec2::new(x, self.floor_y), 6.0);
        // From a point on the sheet or tangle straight down to the floor,
        // fixed there, then back up to the retreat.
        self.queue.push_back(Move::new(
            top,
            Op::StartOn {
                kind: ThreadKind::Gumfoot,
                on: None,
                radius: 14.0,
            },
        ));
        self.queue.push_back(Move {
            to: foot,
            ops: vec![Op::Attach(Anchor::Fixed), Op::Release],
            dwell: 0.3,
        });
        self.queue.push_back(Move {
            to: r,
            ops: Vec::new(),
            dwell: RETREAT_DWELL * 0.5,
        });
        self.feet.push(foot);
        self.gumfoot_done += 1;
    }

    fn plan(&mut self, world: &Anchors, rng: &mut Pcg32) {
        let r = self.retreat;
        match self.stage {
            Stage::Retreat => {
                // A small tent of dense silk in the top corner of the web.
                self.queue.push_back(Move::new(r, Op::PayOut(ThreadKind::Retreat)));
                let top = Self::top_y(world);
                for k in 0..7 {
                    let a = k as f32 * 0.9;
                    let p = Vec2::new(
                        r.x + a.cos() * rng.range(6.0, 12.0),
                        (r.y + a.sin() * rng.range(4.0, 9.0)).min(top),
                    );
                    let op = if k % 3 == 0 {
                        Op::Attach(Anchor::Fixed)
                    } else {
                        Op::Attach(Anchor::Free)
                    };
                    self.queue.push_back(Move::new(p, op));
                }
                self.queue.push_back(Move {
                    to: r,
                    ops: vec![Op::AttachNear(6.0), Op::Release],
                    dwell: RETREAT_DWELL,
                });
                self.stage = Stage::Tangle;
            }
            Stage::Tangle => {
                if self.tangle_done < 2 {
                    // Tangle first, twice, before any sheet.
                    self.queue_tangle_bout(world, rng);
                } else if self.tangle_done < TANGLE_BOUTS || self.sheet_done < SHEET_BOUTS {
                    // Then alternate.
                    if self.sheet_next && self.sheet_done < SHEET_BOUTS {
                        self.queue_sheet_bout(world, rng);
                    } else if self.tangle_done < TANGLE_BOUTS {
                        self.queue_tangle_bout(world, rng);
                    } else {
                        self.queue_sheet_bout(world, rng);
                    }
                    self.sheet_next = !self.sheet_next;
                } else {
                    self.stage = Stage::Gumfoot;
                    return self.plan(world, rng);
                }
            }
            Stage::Sheet => {
                self.queue_sheet_bout(world, rng);
                self.stage = Stage::Tangle;
            }
            Stage::Gumfoot => {
                if self.gumfoot_done < self.gumfoot_target {
                    self.queue_gumfoot(world, rng);
                } else {
                    self.queue.push_back(Move::walk(r));
                    self.stage = Stage::Done;
                }
            }
            Stage::Done | Stage::Adding => {
                if self.queue.is_empty() {
                    self.stage = Stage::Done;
                }
            }
        }
    }
}

impl WebProgram for CobwebProgram {
    fn species(&self) -> Species {
        Species::Parasteatoda
    }

    fn next(&mut self, _silk: &Silk, world: &Anchors, rng: &mut Pcg32, _pos: Vec2) -> Option<Move> {
        if self.queue.is_empty() && self.stage != Stage::Done {
            self.plan(world, rng);
        }
        self.queue.pop_front()
    }

    fn sit_point(&self, _silk: &Silk) -> Option<Vec2> {
        Some(self.retreat)
    }

    fn progress(&self) -> f32 {
        let total = 1.0 + TANGLE_BOUTS as f32 + SHEET_BOUTS as f32 + self.gumfoot_target as f32;
        let done = match self.stage {
            Stage::Retreat => 0.0,
            _ => 1.0 + self.tangle_done.min(TANGLE_BOUTS) as f32
                + self.sheet_done.min(SHEET_BOUTS) as f32
                + self.gumfoot_done.min(self.gumfoot_target) as f32,
        };
        if matches!(self.stage, Stage::Done) {
            1.0
        } else {
            (done / total).min(0.99)
        }
    }

    fn stage(&self) -> &'static str {
        match self.stage {
            Stage::Retreat => "retreat",
            Stage::Tangle => "tangle",
            Stage::Sheet => "sheet",
            Stage::Gumfoot => "gumfoot lines",
            Stage::Done => "complete",
            Stage::Adding => "adding",
        }
    }

    fn complete(&self) -> bool {
        self.stage == Stage::Done && self.queue.is_empty()
    }

    fn reset(&mut self, world: &Anchors, rng: &mut Pcg32) {
        let region: Region = world.bounds;
        let (w, h) = region.size;
        let (hw, _) = region.half();
        self.half_w = (0.30 * w).clamp(80.0, 260.0);
        let jx = (hw - self.half_w - 2.0 * WALL_INSET).max(0.0);
        let top = Self::top_y(world);
        let side = if rng.f32() < 0.5 { -1.0 } else { 1.0 };
        // Sheltered wall/ceiling junction, not a retreat suspended in open air.
        self.retreat = if world.is_enclosure() {
            Vec2::new(region.center.x + side * (hw - WALL_INSET - 6.0).max(0.0), top - 2.0)
        } else {
            let from = Vec2::new(region.center.x + side * jx * 0.6, top - 6.0);
            world.anchor(from, std::f32::consts::FRAC_PI_2, WALL_INSET)
        };
        self.sheet_y = top - TANGLE_DEPTH * h;
        self.floor_y = region.min().y + 6.0;
        self.stage = Stage::Retreat;
        self.queue.clear();
        self.tangle_done = 0;
        self.sheet_done = 0;
        self.gumfoot_done = 0;
        self.gumfoot_target = GUMFOOT_LINES;
        self.sheet_next = false;
        self.feet.clear();
        self.world = world.clone();
    }

    fn on_damage(&mut self, silk: &Silk) -> bool {
        if self.stage != Stage::Done && self.stage != Stage::Adding {
            return false;
        }
        // The retreat gone is the one thing that starts over.
        if silk.thread_near(self.retreat, Some(ThreadKind::Retreat), 14.0).is_none() {
            return true;
        }
        // Missing gumfoot lines are re-laid; a tangle is added to, never
        // rebuilt.
        let gone: Vec<Vec2> = self
            .feet
            .iter()
            .copied()
            .filter(|f| silk.thread_near(Vec2::new(f.x, f.y + 20.0), Some(ThreadKind::Gumfoot), 8.0).is_none())
            .collect();
        if !gone.is_empty() {
            self.feet.retain(|f| !gone.contains(f));
            self.gumfoot_done = self.feet.len();
            let world = self.world.clone();
            let mut rng = Pcg32::new(0x600d ^ silk.threads.len() as u64);
            for _ in 0..gone.len() {
                self.queue_gumfoot(&world, &mut rng);
            }
            self.queue.push_back(Move::walk(self.retreat));
            self.stage = Stage::Adding;
        }
        false
    }

    fn rebuilds_daily(&self) -> bool {
        false
    }

    fn nightly(&mut self, _silk: &Silk, world: &Anchors, rng: &mut Pcg32) {
        // Another tangle bout and a couple of gumfoot lines, as the animal
        // does: the web grows.
        self.queue_tangle_bout(world, rng);
        self.gumfoot_target += GUMFOOT_NIGHTLY;
        for _ in 0..GUMFOOT_NIGHTLY {
            self.queue_gumfoot(world, rng);
        }
        self.queue.push_back(Move::walk(self.retreat));
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
        center: Vec2 { x: 120.0, y: -60.0 },
        size: (720.0, 520.0),
    };
    const DT: f32 = 1.0 / 60.0;

    fn house_spider(seed: u64) -> Weaver {
        let mut rng = Pcg32::new(seed);
        let program = CobwebProgram::new(&Anchors::enclosure(TANK), &mut rng);
        Weaver::new(Species::Parasteatoda, TANK.center, seed, Box::new(program))
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
    fn tangle_comes_before_sheet_and_the_spider_goes_home_between_bouts() {
        let mut w = house_spider(4);
        let mut s = BrainSignals::new();
        s.walk_drive = 0.6;
        let mut first_sheet_at = None;
        let mut tangle_at_first_sheet = 0;
        let mut returns = 0;
        let mut was_home = true;
        let mut t = 0.0;
        while !w.web_complete() && t < 900.0 {
            w.update(DT, TANK, None, Some(s));
            t += DT;
            if first_sheet_at.is_none() && w.silk.count_kind(ThreadKind::Sheet) > 0 {
                first_sheet_at = Some(t);
                tangle_at_first_sheet = w.silk.count_kind(ThreadKind::Tangle);
            }
            let r = w.sit_point().unwrap();
            let home = hypot(w.pos.x - r.x, w.pos.y - r.y) < 3.0;
            if home && !was_home {
                returns += 1;
            }
            was_home = home;
        }
        assert!(w.web_complete(), "unfinished at {t:.0} s: {}", w.program.stage());
        assert!(t < 600.0, "took {t:.0} s");
        for _ in 0..30 {
            w.update(DT, TANK, None, Some(s));
        }
        assert!(first_sheet_at.is_some(), "a sheet was laid");
        assert!(tangle_at_first_sheet >= 2 * LINES_PER_BOUT, "tangle first: {tangle_at_first_sheet} lines");
        assert!(returns >= TANGLE_BOUTS + SHEET_BOUTS, "returns to the retreat: {returns}");
        assert_eq!(w.silk.count_kind(ThreadKind::Gumfoot), GUMFOOT_LINES);
        assert!(w.silk.count_kind(ThreadKind::Retreat) >= 6);
        assert_eq!(w.state, WeaverState::Sitting);
        // Every gumfoot line reaches the floor.
        let floor = TANK.min().y + 6.0;
        for th in w.silk.threads.iter().filter(|t| t.kind == ThreadKind::Gumfoot) {
            let low = w.silk.nodes[th.a].pos.y.min(w.silk.nodes[th.b].pos.y);
            assert!((low - floor).abs() < 1.0, "foot at {low}, floor {floor}");
        }
    }

    #[test]
    fn a_bug_on_the_floor_is_lifted_and_caught() {
        let mut w = house_spider(6);
        build(&mut w, 900.0);
        let floor = TANK.min().y + 6.0;
        // Find a foot and start a bug walking toward it.
        let foot = w
            .silk
            .threads
            .iter()
            .filter(|t| t.kind == ThreadKind::Gumfoot)
            .map(|t| {
                let (a, b) = (w.silk.nodes[t.a].pos, w.silk.nodes[t.b].pos);
                if a.y < b.y {
                    a
                } else {
                    b
                }
            })
            .next()
            .unwrap();
        w.spawn_bug(Vec2::new(foot.x - 40.0, floor), Vec2::new(30.0, 0.0), true);
        let calm = BrainSignals::new();
        let mut lifted = false;
        for _ in 0..60 * 8 {
            w.update(DT, TANK, None, Some(calm));
            if !w.prey.is_empty() && w.prey[0].stuck && w.prey[0].lift_to.is_none() && w.prey[0].pos.y > floor + 30.0 {
                lifted = true;
                break;
            }
        }
        assert!(lifted, "bug {:?}", w.prey.first());
        // The struggle is loud; a strike sends the spider down for it.
        for _ in 0..30 {
            w.update(DT, TANK, None, Some(calm));
        }
        assert!(w.silk.loudest(0.01).is_some());
        let mut s = BrainSignals::new();
        s.strike = true;
        w.update(DT, TANK, None, Some(s));
        assert_eq!(w.state, WeaverState::Hunting);
        for _ in 0..60 * 15 {
            w.update(DT, TANK, None, Some(calm));
            if w.captured > 0 {
                break;
            }
        }
        assert_eq!(w.captured, 1, "state {:?} pos {:?} bug {:?} loudest {:?}", w.state, w.pos, w.prey.first(), w.silk.loudest(0.0));
    }

    #[test]
    fn the_web_stands_and_a_night_adds_to_it() {
        let mut w = house_spider(8);
        build(&mut w, 900.0);
        let before = (w.silk.count_kind(ThreadKind::Tangle), w.silk.count_kind(ThreadKind::Gumfoot));
        // Dusk, with a web old enough.
        w.hour = 20.0;
        w.web_age = 7.0 * 3600.0;
        let mut s = BrainSignals::new();
        s.walk_drive = 0.6;
        let mut t = 0.0;
        w.update(DT, TANK, None, Some(s));
        assert_ne!(w.state, WeaverState::Eating, "a theridiid does not take its web down");
        while !w.web_complete() && t < 300.0 {
            w.update(DT, TANK, None, Some(s));
            t += DT;
        }
        let after = (w.silk.count_kind(ThreadKind::Tangle), w.silk.count_kind(ThreadKind::Gumfoot));
        assert!(after.0 > before.0, "tangle grew: {before:?} -> {after:?}");
        assert_eq!(after.1, before.1 + GUMFOOT_NIGHTLY, "gumfoot lines: {before:?} -> {after:?}");
    }

    #[test]
    fn a_cut_gumfoot_line_is_re_laid_and_the_retreat_gone_is_a_rebuild() {
        let mut w = house_spider(9);
        build(&mut w, 900.0);
        let floor = TANK.min().y + 6.0;
        let foot = w
            .silk
            .threads
            .iter()
            .filter(|t| t.kind == ThreadKind::Gumfoot)
            .map(|t| {
                let (a, b) = (w.silk.nodes[t.a].pos, w.silk.nodes[t.b].pos);
                if a.y < b.y {
                    a
                } else {
                    b
                }
            })
            .next()
            .unwrap();
        let before = w.silk.count_kind(ThreadKind::Gumfoot);
        let cut = w.damage(Vec2::new(foot.x, floor + 30.0), 6.0);
        assert!(cut >= 1);
        let after_cut = w.silk.count_kind(ThreadKind::Gumfoot);
        assert_eq!(w.program.stage(), "adding");
        let t = build(&mut w, 300.0);
        assert!(w.web_complete(), "re-laid in {t:.0} s");
        assert_eq!(
            w.silk.count_kind(ThreadKind::Gumfoot),
            before,
            "before {before}, cut {cut} -> {after_cut}, stage {}",
            w.program.stage()
        );
        let r = w.sit_point().unwrap();
        w.damage(r, 30.0);
        assert_eq!(w.state, WeaverState::Eating, "the retreat gone: start over");
    }
}
