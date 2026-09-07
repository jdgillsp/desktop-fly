//! Where a web can be fixed: the structures on the screen.
//!
//! A web hangs in the air between things. In an enclosure the things are the
//! four walls, and they never move. On the desktop they are the edges of the
//! screen and the edges of every window — and windows move and close. So a
//! thread's fixed end remembers *which* structure it is on
//! (`silk::Node::on`), and when that structure moves or goes, the threads on
//! it — and only those — are cut, and the animal repairs or starts again
//! (WEB_PLAN.md §6, §13.2).
//!
//! Nothing here knows what a window is. A [`Frame`] is a rectangle with an
//! id, produced by the platform layer from the same window rectangles the
//! fly's `Ledge`s come from. The screen is a frame too, with an id that never
//! changes, so a thread to the edge of the display is a thread to something.
//!
//! Windows are treated as **solid**: on the desktop the web is planned in the
//! air between them, never over one. A web inside a window's rectangle would
//! be anchored only to that window, and every move of it would take the whole
//! web; a web in a gap loses one side and keeps the rest, which is the
//! behaviour the feature is for.

use crate::habitat::Region;
use crate::rng::Pcg32;
use crate::util::{hypot, Vec2};

/// The display's own edges: a structure that never moves.
pub const SCREEN: i64 = 0;
/// The walls of an enclosure: likewise.
pub const WALLS: i64 = -1;

/// How far a thread can go looking for something to fix to on the desktop.
/// Beyond this the program falls back to the edge of its own box and the
/// thread ends on nothing. An enclosure has no limit: a wall is always there.
pub const MAX_REACH: f32 = 1100.0;

/// The smallest clearance a site on the desktop needs in every direction
/// before a web is planned there — the smallest orb the species builds.
pub const MIN_CLEARANCE: f32 = 110.0;

/// A rectangle on the screen a thread can be fixed to, in scene coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub lo: Vec2,
    pub hi: Vec2,
    pub id: i64,
}

impl Frame {
    pub fn of(region: Region, id: i64) -> Frame {
        Frame {
            lo: region.min(),
            hi: region.max(),
            id,
        }
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.lo.x && p.x <= self.hi.x && p.y >= self.lo.y && p.y <= self.hi.y
    }

    /// Has it moved or resized by more than a hair? Two units, because window
    /// managers jitter a frame by a pixel without anyone having touched it.
    pub fn moved_from(&self, o: &Frame) -> bool {
        (self.lo.x - o.lo.x).abs() > 2.0
            || (self.lo.y - o.lo.y).abs() > 2.0
            || (self.hi.x - o.hi.x).abs() > 2.0
            || (self.hi.y - o.hi.y).abs() > 2.0
    }

    /// Distance from `p` to the nearest point of the rectangle's outline.
    pub fn edge_distance(&self, p: Vec2) -> f32 {
        let inside = self.contains(p);
        if inside {
            (p.x - self.lo.x)
                .min(self.hi.x - p.x)
                .min(p.y - self.lo.y)
                .min(self.hi.y - p.y)
        } else {
            let dx = (self.lo.x - p.x).max(0.0).max(p.x - self.hi.x);
            let dy = (self.lo.y - p.y).max(0.0).max(p.y - self.hi.y);
            hypot(dx, dy)
        }
    }

    /// Where a ray from `from` in direction (`dx`, `dy`) meets this frame,
    /// stopping `inset` short of its outline, as (distance, point).
    ///
    /// From inside, that is the exit through the outline shrunk by `inset` —
    /// exactly the arithmetic the orb program used against its box, so an
    /// enclosure build is unchanged. From outside, it is the entry into the
    /// outline grown by `inset`, so the animal stops just short of the thing.
    fn hit(&self, from: Vec2, dx: f32, dy: f32, inset: f32) -> Option<(f32, Vec2)> {
        if self.contains(from) {
            let lo = Vec2::new(self.lo.x + inset, self.lo.y + inset);
            let hi = Vec2::new(self.hi.x - inset, self.hi.y - inset);
            let mut t = f32::MAX;
            if dx.abs() > 1e-6 {
                let tx = if dx > 0.0 { (hi.x - from.x) / dx } else { (lo.x - from.x) / dx };
                if tx > 0.0 {
                    t = t.min(tx);
                }
            }
            if dy.abs() > 1e-6 {
                let ty = if dy > 0.0 { (hi.y - from.y) / dy } else { (lo.y - from.y) / dy };
                if ty > 0.0 {
                    t = t.min(ty);
                }
            }
            if t == f32::MAX {
                return None;
            }
            Some((
                t,
                Vec2::new(
                    (from.x + dx * t).clamp(lo.x, hi.x),
                    (from.y + dy * t).clamp(lo.y, hi.y),
                ),
            ))
        } else {
            let lo = Vec2::new(self.lo.x - inset, self.lo.y - inset);
            let hi = Vec2::new(self.hi.x + inset, self.hi.y + inset);
            let (mut tmin, mut tmax) = (0.0f32, f32::MAX);
            for (f, d, l, h) in [(from.x, dx, lo.x, hi.x), (from.y, dy, lo.y, hi.y)] {
                if d.abs() < 1e-6 {
                    if f < l || f > h {
                        return None;
                    }
                } else {
                    let (t1, t2) = ((l - f) / d, (h - f) / d);
                    tmin = tmin.max(t1.min(t2));
                    tmax = tmax.min(t1.max(t2));
                }
            }
            if tmax < tmin || tmin <= 1e-6 {
                return None;
            }
            Some((
                tmin,
                Vec2::new(
                    (from.x + dx * tmin).clamp(lo.x, hi.x),
                    (from.y + dy * tmin).clamp(lo.y, hi.y),
                ),
            ))
        }
    }
}

/// The things a web can be fixed to, and the box it is planned in.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchors {
    /// The enclosure, or the patch of desktop air the web occupies. The
    /// programs size their webs from this and fall back to its edges when
    /// nothing real is in reach.
    pub bounds: Region,
    pub structures: Vec<Frame>,
    /// How far a thread may reach for a structure.
    pub reach: f32,
}

impl Anchors {
    /// Four walls that never move.
    pub fn enclosure(region: Region) -> Self {
        Anchors {
            bounds: region,
            structures: vec![Frame::of(region, WALLS)],
            reach: f32::INFINITY,
        }
    }

    /// The desktop: the screen's edges and every window's, with the web
    /// planned in `bounds`.
    pub fn desktop(bounds: Region, display: Region, frames: &[Frame]) -> Self {
        let mut structures = Vec::with_capacity(frames.len() + 1);
        structures.push(Frame::of(display, SCREEN));
        structures.extend(frames.iter().copied());
        Anchors {
            bounds,
            structures,
            reach: MAX_REACH,
        }
    }

    pub fn is_enclosure(&self) -> bool {
        self.structures.len() == 1 && self.structures[0].id == WALLS
    }

    /// The first structure a ray from `from` at `angle` reaches within
    /// `reach`, as the point `inset` short of it and the structure's id.
    pub fn ray(&self, from: Vec2, angle: f32, inset: f32) -> Option<(Vec2, i64)> {
        let (dy, dx) = angle.sin_cos();
        let mut best: Option<(f32, Vec2, i64)> = None;
        for s in &self.structures {
            if let Some((t, p)) = s.hit(from, dx, dy, inset) {
                if t <= self.reach && best.map(|b| t < b.0).unwrap_or(true) {
                    best = Some((t, p, s.id));
                }
            }
        }
        best.map(|(_, p, id)| (p, id))
    }

    /// Where the bounds box's own edge is in that direction: the fallback
    /// when nothing real is in reach. Fixed to nothing, and `on` will say so.
    fn bounds_point(&self, from: Vec2, angle: f32, inset: f32) -> Vec2 {
        let (dy, dx) = angle.sin_cos();
        match Frame::of(self.bounds, WALLS).hit(from, dx, dy, inset) {
            Some((_, p)) => p,
            None => self.bounds.clamp_inside(from, inset),
        }
    }

    /// A point to fix a thread at, in direction `angle` from `from`: on the
    /// nearest structure, or on the edge of the box if none is in reach.
    pub fn anchor(&self, from: Vec2, angle: f32, inset: f32) -> Vec2 {
        match self.ray(from, angle, inset) {
            Some((p, _)) => p,
            None => self.bounds_point(from, angle, inset),
        }
    }

    /// [`anchor`](Self::anchor), aimed at a point rather than an angle. In an
    /// enclosure, aimed at a point on the inset wall, this returns that point.
    pub fn anchor_toward(&self, from: Vec2, target: Vec2, inset: f32) -> Vec2 {
        let (dx, dy) = (target.x - from.x, target.y - from.y);
        if hypot(dx, dy) < 1e-3 {
            return target;
        }
        self.anchor(from, dy.atan2(dx), inset)
    }

    /// The structure whose outline `p` lies within `tol` of, nearest first.
    pub fn on(&self, p: Vec2, tol: f32) -> Option<i64> {
        self.structures
            .iter()
            .map(|s| (s.edge_distance(p), s.id))
            .filter(|(d, _)| *d <= tol)
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .map(|(_, id)| id)
    }

    /// Choose where on the desktop to build: a spot in the air between
    /// structures, with room for the smallest web in every direction and the
    /// structures as close around it as that allows — a gap between two
    /// windows, a corner of the screen, the space under a window and above
    /// the taskbar. Windows are solid; nothing is planned over one. `None` if
    /// the screen is too crowded for any web at all.
    ///
    /// Deterministic given the frames and the seed: the best few sites by
    /// score are kept and one is drawn, so the same desktop does not always
    /// get the same corner.
    pub fn choose_site(display: Region, frames: &[Frame], rng: &mut Pcg32) -> Option<Region> {
        let world = Anchors::desktop(display, display, frames);
        let (lo, hi) = (display.min(), display.max());
        const STEP: f32 = 80.0;
        const DIRS: usize = 8;
        let mut sites: Vec<(f32, Vec2, f32)> = Vec::new();
        let mut y = lo.y + 60.0;
        while y < hi.y - 60.0 {
            let mut x = lo.x + 60.0;
            while x < hi.x - 60.0 {
                let p = Vec2::new(x, y);
                x += STEP;
                if frames.iter().any(|f| f.contains(p)) {
                    continue;
                }
                let mut clearance = f32::MAX;
                let mut score = 0.0;
                for k in 0..DIRS {
                    let a = std::f32::consts::TAU * k as f32 / DIRS as f32;
                    let d = match world.ray(p, a, 0.0) {
                        Some((q, _)) => hypot(q.x - p.x, q.y - p.y),
                        None => MAX_REACH,
                    };
                    clearance = clearance.min(d);
                    score += d.min(MAX_REACH);
                }
                if clearance >= MIN_CLEARANCE {
                    sites.push((score, p, clearance));
                }
            }
            y += STEP;
        }
        if sites.is_empty() {
            return None;
        }
        sites.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let n = sites.len().min(3);
        let (_, at, clearance) = sites[rng.int_range(0, n as i64 - 1) as usize];
        // The box is as big as the clearance allows, capped at the free-roam
        // web size, and kept on the screen.
        let side = (clearance * 2.2).min(crate::weaver::FREE_ROAM_WEB.0.max(crate::weaver::FREE_ROAM_WEB.1));
        let size = (
            side.min(crate::weaver::FREE_ROAM_WEB.0),
            side.min(crate::weaver::FREE_ROAM_WEB.1),
        );
        let center = display.clamp_inside(at, size.0.max(size.1) * 0.5);
        Some(Region::new(center, size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TANK: Region = Region {
        center: Vec2 { x: 160.0, y: -80.0 },
        size: (720.0, 520.0),
    };

    /// The orb program's old `wall_point`, kept here as the oracle: an
    /// enclosure ray has to reproduce it exactly, or every enclosure web
    /// changes.
    fn wall_point(region: Region, from: Vec2, angle: f32, inset: f32) -> Vec2 {
        let lo = region.min();
        let hi = region.max();
        let (lo, hi) = (
            Vec2::new(lo.x + inset, lo.y + inset),
            Vec2::new(hi.x - inset, hi.y - inset),
        );
        let (dx, dy) = (angle.cos(), angle.sin());
        let mut t = f32::MAX;
        if dx.abs() > 1e-6 {
            let tx = if dx > 0.0 { (hi.x - from.x) / dx } else { (lo.x - from.x) / dx };
            if tx > 0.0 {
                t = t.min(tx);
            }
        }
        if dy.abs() > 1e-6 {
            let ty = if dy > 0.0 { (hi.y - from.y) / dy } else { (lo.y - from.y) / dy };
            if ty > 0.0 {
                t = t.min(ty);
            }
        }
        if t == f32::MAX {
            return from;
        }
        Vec2::new(
            (from.x + dx * t).clamp(lo.x, hi.x),
            (from.y + dy * t).clamp(lo.y, hi.y),
        )
    }

    #[test]
    fn an_enclosure_ray_is_the_old_wall_point_to_the_bit() {
        let w = Anchors::enclosure(TANK);
        assert!(w.is_enclosure());
        let mut rng = Pcg32::new(1);
        for _ in 0..500 {
            let from = TANK.sample(&mut rng, 20.0);
            let a = rng.range(0.0, std::f32::consts::TAU);
            let want = wall_point(TANK, from, a, 14.0);
            let (got, id) = w.ray(from, a, 14.0).expect("a wall is always there");
            assert_eq!(got, want, "from {from:?} at {a}");
            assert_eq!(id, WALLS);
            assert_eq!(w.anchor(from, a, 14.0), want);
            assert_eq!(w.anchor_toward(from, want, 14.0).x.round(), want.x.round());
        }
    }

    #[test]
    fn on_the_desktop_the_nearer_window_wins_and_reach_is_finite() {
        let display = Region::centered((1920.0, 1440.0));
        let win = Frame {
            lo: Vec2::new(300.0, -400.0),
            hi: Vec2::new(900.0, 300.0),
            id: 7,
        };
        let w = Anchors::desktop(Region::centered((500.0, 400.0)), display, &[win]);
        assert!(!w.is_enclosure());
        // Rightward from the origin: the window's left edge, inset, not the
        // screen edge behind it.
        let (p, id) = w.ray(Vec2::ZERO, 0.0, 14.0).unwrap();
        assert_eq!(id, 7);
        assert!((p.x - 286.0).abs() < 1e-3, "{p:?}");
        // Leftward: the screen.
        let (p, id) = w.ray(Vec2::ZERO, std::f32::consts::PI, 14.0).unwrap();
        assert_eq!(id, SCREEN);
        assert!((p.x + 946.0).abs() < 1e-3, "{p:?}");
        // On a wider display that edge is out of reach: a thread does not
        // cross half a 4K screen to find something.
        let wide = Anchors::desktop(Region::centered((500.0, 400.0)), Region::centered((2560.0, 1440.0)), &[]);
        assert!(wide.ray(Vec2::ZERO, std::f32::consts::PI, 14.0).is_none());
        // Downward on a tall display the bottom is out of reach, and the
        // program is handed the edge of its own box instead.
        let tall = Anchors::desktop(Region::centered((500.0, 400.0)), Region::centered((2560.0, 3000.0)), &[]);
        assert!(tall.ray(Vec2::ZERO, -std::f32::consts::FRAC_PI_2, 14.0).is_none());
        let fallback = tall.anchor(Vec2::ZERO, -std::f32::consts::FRAC_PI_2, 14.0);
        assert!((fallback.y + 186.0).abs() < 1e-3, "{fallback:?}");
        // And a point is known to be on a structure, or on nothing.
        assert_eq!(w.on(Vec2::new(300.0, 0.0), 3.0), Some(7));
        assert_eq!(w.on(Vec2::new(286.0, 0.0), 16.0), Some(7));
        assert_eq!(w.on(Vec2::new(-960.0, 10.0), 3.0), Some(SCREEN));
        assert_eq!(w.on(Vec2::ZERO, 16.0), None);
    }

    #[test]
    fn a_site_is_chosen_in_the_air_between_windows_never_over_one() {
        let display = Region::centered((2560.0, 1440.0));
        // Two windows filling the height, with a gap between.
        let a = Frame {
            lo: Vec2::new(-1200.0, -720.0),
            hi: Vec2::new(-260.0, 720.0),
            id: 1,
        };
        let b = Frame {
            lo: Vec2::new(260.0, -720.0),
            hi: Vec2::new(1200.0, 720.0),
            id: 2,
        };
        let mut rng = Pcg32::new(4);
        let site = Anchors::choose_site(display, &[a, b], &mut rng).expect("there is a gap");
        assert!(!a.contains(site.center) && !b.contains(site.center), "{site:?}");
        assert!(site.center.x.abs() < 260.0, "in the gap: {site:?}");
        assert!(site.size.0 <= crate::weaver::FREE_ROAM_WEB.0 && site.size.1 <= crate::weaver::FREE_ROAM_WEB.1);
        // A screen with no room anywhere gives nothing.
        let full = Frame::of(display, 3);
        assert!(Anchors::choose_site(display, &[full], &mut rng).is_none());
        // An empty screen gives a corner-ish site: something is always
        // close, because a web wants structure around it.
        let site = Anchors::choose_site(display, &[], &mut rng).unwrap();
        let (lo, hi) = (display.min(), display.max());
        let edge = (site.center.x - lo.x)
            .min(hi.x - site.center.x)
            .min(site.center.y - lo.y)
            .min(hi.y - site.center.y);
        assert!(edge < 200.0, "an empty desktop's site is out in the open: {site:?}");
    }
}
