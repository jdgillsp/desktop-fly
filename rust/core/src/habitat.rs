//! An optional enclosure: a bounded patch of desktop the creature stays inside,
//! with a few props it notices and pushes around.
//!
//! Two ideas, kept separate on purpose:
//!
//! - [`Region`] is *where the world ends*. Every body already turned away from
//!   the edge of the display and clamped itself inside it — but each did so
//!   against `±bounds/2`, which silently assumed the region was centred on the
//!   scene origin. It always was, so the assumption was invisible and free. A
//!   habitat parked in a corner breaks it, so `Region` carries a centre and the
//!   bodies ask it questions instead of doing the arithmetic themselves. With
//!   [`Region::centered`] the answers are bit-identical to the old ones, which
//!   is why free-roam is untouched by this whole feature.
//! - [`Habitat`] is *the enclosure itself*: which kind of tank, and the props
//!   in it. It is pure state and a `step`; the geometry lives in the shell,
//!   like every other body's does.
//!
//! ## Why the props cannot be clicked
//!
//! The overlay is click-through by contract — `WS_EX_TRANSPARENT`, plus
//! `set_cursor_hittest(false)`, and the README promises it never intercepts the
//! user's mouse. A prop you could click would be a hole in the desktop, and the
//! moment there is one hole there is a reason to add another. So interaction
//! runs the other way round: props react to the **creature**, and to the
//! cursor's *proximity* — the same shadow-of-a-hand channel the creatures
//! already sense through. Nothing in here consumes a click.

use crate::creature::Substrate;
use crate::rng::Pcg32;
use crate::util::{clamp, hypot, Vec2};

/// An axis-aligned patch of scene the creature lives inside.
///
/// Scene space is centred on the display with y up (see `ScreenSpace`), so the
/// whole-display case is a `Region` at the origin and everything downstream is
/// unchanged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    pub center: Vec2,
    pub size: (f32, f32),
}

impl Region {
    /// The free-roam region: the whole display, centred on the origin. This is
    /// exactly what the bodies assumed before habitats existed.
    pub fn centered(size: (f32, f32)) -> Self {
        Region {
            center: Vec2::ZERO,
            size,
        }
    }

    pub fn new(center: Vec2, size: (f32, f32)) -> Self {
        Region { center, size }
    }

    #[inline]
    pub fn half(&self) -> (f32, f32) {
        (self.size.0 / 2.0, self.size.1 / 2.0)
    }

    #[inline]
    pub fn min(&self) -> Vec2 {
        Vec2::new(
            self.center.x - self.size.0 / 2.0,
            self.center.y - self.size.1 / 2.0,
        )
    }

    #[inline]
    pub fn max(&self) -> Vec2 {
        Vec2::new(
            self.center.x + self.size.0 / 2.0,
            self.center.y + self.size.1 / 2.0,
        )
    }

    /// Is `p` within `margin` of the wall, or past it?
    #[inline]
    pub fn outside(&self, p: Vec2, margin: f32) -> bool {
        let (hw, hh) = self.half();
        (p.x - self.center.x).abs() > hw - margin || (p.y - self.center.y).abs() > hh - margin
    }

    /// Pull `p` back inside, leaving `margin` of clearance. A margin wider than
    /// the region collapses to its centre rather than inverting the interval.
    #[inline]
    pub fn clamp_inside(&self, p: Vec2, margin: f32) -> Vec2 {
        let (hw, hh) = self.half();
        let (ex, ey) = ((hw - margin).max(0.0), (hh - margin).max(0.0));
        Vec2::new(
            clamp(p.x, self.center.x - ex, self.center.x + ex),
            clamp(p.y, self.center.y - ey, self.center.y + ey),
        )
    }

    /// A point drawn uniformly from the region, `margin` in from the wall.
    pub fn sample(&self, rng: &mut Pcg32, margin: f32) -> Vec2 {
        let (hw, hh) = self.half();
        let (ex, ey) = ((hw - margin).max(0.0), (hh - margin).max(0.0));
        Vec2::new(
            self.center.x + rng.range(-ex, ex),
            self.center.y + rng.range(-ey, ey),
        )
    }

    /// Heading, in radians, from `p` back toward the middle. Every body steers
    /// by this when it finds itself near a wall.
    #[inline]
    pub fn bearing_home(&self, p: Vec2) -> f32 {
        (self.center.y - p.y).atan2(self.center.x - p.x)
    }
}

/// Which enclosure a creature gets. Chosen from its [`Substrate`], which is
/// what that enum was declared for — until now nothing branched on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HabitatKind {
    /// Water, gravel, plants. For swimmers.
    Aquarium,
    /// Substrate, pebbles, a plant. For everything that walks or crawls.
    Terrarium,
}

impl HabitatKind {
    pub fn for_substrate(s: Substrate) -> Self {
        match s {
            Substrate::Swimmer => HabitatKind::Aquarium,
            Substrate::WalkerFlier | Substrate::WalkerJumper | Substrate::Crawler => {
                HabitatKind::Terrarium
            }
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            HabitatKind::Aquarium => "aquarium",
            HabitatKind::Terrarium => "terrarium",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropKind {
    /// Rolls when the creature or the cursor bumps it, then coasts to a stop.
    Ball,
    /// Does not move. Scenery, and a landmark.
    Pebble,
    /// Anchored; sways when something passes close.
    Plant,
    /// Drifts, is approached, is eaten, comes back somewhere else.
    Food,
}

#[derive(Debug, Clone, Copy)]
pub struct Prop {
    pub kind: PropKind,
    pub pos: Vec2,
    pub vel: Vec2,
    pub radius: f32,
    /// 0..1 — how disturbed this prop is right now. Drives the plant's sway and
    /// the ball's highlight; decays on its own.
    pub stir: f32,
    /// Phase for idle motion, so two plants do not sway in lockstep.
    pub phase: f32,
    /// Food only: seconds until it reappears. 0 means it is present.
    pub respawn: f32,
}

impl Prop {
    pub fn present(&self) -> bool {
        self.respawn <= 0.0
    }
}

/// How close the creature has to get to a prop to affect it. Generous: the
/// bodies are a few dozen units long and the point is that contact reads as
/// contact, not as a near miss.
const TOUCH: f32 = 26.0;
/// The cursor is a shadow passing over the tank, not a finger in it — it stirs
/// props from further away, and pushes them more gently.
const CURSOR_REACH: f32 = 70.0;

/// The enclosure: a region, a kind, and the props inside it.
#[derive(Debug, Clone)]
pub struct Habitat {
    pub kind: HabitatKind,
    pub region: Region,
    pub props: Vec<Prop>,
    /// Which prop the creature is currently interested in, as an index. The
    /// bodies only ever see its position, via `World::attractor`.
    pub focus: Option<usize>,
    focus_timer: f32,
    rng: Pcg32,
}

impl Habitat {
    pub fn new(kind: HabitatKind, region: Region, seed: u64) -> Self {
        let mut h = Habitat {
            kind,
            region,
            props: Vec::new(),
            focus: None,
            focus_timer: 0.0,
            rng: Pcg32::new(seed),
        };
        h.stock();
        h
    }

    /// Fill the enclosure. Deliberately sparse: four props in a tank the size of
    /// a browser window is already a busy picture at this scale, and the
    /// creature is the thing you are meant to be watching.
    fn stock(&mut self) {
        self.props.clear();
        let kinds: [(PropKind, f32); 4] = match self.kind {
            HabitatKind::Aquarium => [
                (PropKind::Plant, 16.0),
                (PropKind::Plant, 13.0),
                (PropKind::Pebble, 11.0),
                (PropKind::Food, 5.0),
            ],
            HabitatKind::Terrarium => [
                (PropKind::Pebble, 14.0),
                (PropKind::Pebble, 10.0),
                (PropKind::Plant, 15.0),
                (PropKind::Ball, 9.0),
            ],
        };
        // Plants and pebbles belong against the back wall (up-screen), where
        // they frame the creature instead of sitting on top of it.
        for (i, (kind, radius)) in kinds.into_iter().enumerate() {
            let (hw, hh) = self.region.half();
            let pos = match kind {
                // Anywhere but the near strip, which is gravel, and never
                // hard against the glass. A wide band rather than a narrow one
                // so scenery does not line up on an invisible shelf.
                PropKind::Plant | PropKind::Pebble => Vec2::new(
                    self.region.center.x + self.rng.range(-hw * 0.78, hw * 0.78),
                    self.region.center.y + self.rng.range(-hh * 0.28, hh * 0.76),
                ),
                _ => self.region.sample(&mut self.rng, 60.0),
            };
            self.props.push(Prop {
                kind,
                pos,
                vel: Vec2::ZERO,
                radius,
                stir: 0.0,
                phase: i as f32 * 1.7,
                respawn: 0.0,
            });
        }
    }

    /// Move the enclosure — on a display change, or when the screen geometry
    /// shifts under it. Props travel with their tank.
    pub fn reshape(&mut self, region: Region) {
        let dx = region.center.x - self.region.center.x;
        let dy = region.center.y - self.region.center.y;
        let resized = region.size != self.region.size;
        self.region = region;
        for p in self.props.iter_mut() {
            p.pos = Vec2::new(p.pos.x + dx, p.pos.y + dy);
        }
        if resized {
            // A differently-shaped tank would leave props hanging outside it.
            self.stock();
        }
    }

    /// One frame of enclosure life.
    pub fn step(&mut self, dt: f32, creature: Vec2, cursor: Option<Vec2>) {
        let region = self.region;
        for p in self.props.iter_mut() {
            p.phase += dt * (0.7 + p.radius * 0.02);
            p.stir = (p.stir - dt * 0.8).max(0.0);

            if p.respawn > 0.0 {
                p.respawn -= dt;
                continue;
            }

            let d = hypot(creature.x - p.pos.x, creature.y - p.pos.y);
            let near_creature = d < p.radius + TOUCH;
            let near_cursor = cursor
                .map(|c| hypot(c.x - p.pos.x, c.y - p.pos.y) < p.radius + CURSOR_REACH)
                .unwrap_or(false);
            if near_creature || near_cursor {
                p.stir = (p.stir + dt * 3.0).min(1.0);
            }

            match p.kind {
                PropKind::Ball => {
                    // Pushed away from whatever touched it, then friction. The
                    // ball is the one prop that keeps a position of its own, so
                    // a session leaves it wherever the creature put it.
                    if near_creature && d > 1e-3 {
                        let k = 190.0 * (1.0 - d / (p.radius + TOUCH)).max(0.0);
                        p.vel.x += (p.pos.x - creature.x) / d * k * dt;
                        p.vel.y += (p.pos.y - creature.y) / d * k * dt;
                    }
                    if let Some(c) = cursor {
                        let cd = hypot(c.x - p.pos.x, c.y - p.pos.y);
                        if cd < p.radius + CURSOR_REACH && cd > 1e-3 {
                            let k = 70.0 * (1.0 - cd / (p.radius + CURSOR_REACH)).max(0.0);
                            p.vel.x += (p.pos.x - c.x) / cd * k * dt;
                            p.vel.y += (p.pos.y - c.y) / cd * k * dt;
                        }
                    }
                    let drag = (1.0 - 2.4 * dt).max(0.0);
                    p.vel.x *= drag;
                    p.vel.y *= drag;
                    p.pos.x += p.vel.x * dt;
                    p.pos.y += p.vel.y * dt;
                    // Bounce off the glass rather than stopping dead against it.
                    let (hw, hh) = region.half();
                    let ex = (hw - p.radius - 6.0).max(0.0);
                    let ey = (hh - p.radius - 6.0).max(0.0);
                    if (p.pos.x - region.center.x).abs() > ex {
                        p.vel.x = -p.vel.x * 0.55;
                    }
                    if (p.pos.y - region.center.y).abs() > ey {
                        p.vel.y = -p.vel.y * 0.55;
                    }
                    p.pos = region.clamp_inside(p.pos, p.radius + 6.0);
                }
                PropKind::Food => {
                    // Sinks, wanders a little on the way down, and is gone the
                    // moment the fish reaches it.
                    p.pos.y -= 11.0 * dt;
                    p.pos.x += (p.phase * 0.9).sin() * 6.0 * dt;
                    if near_creature {
                        p.respawn = 6.0;
                    } else if p.pos.y <= region.center.y - region.half().1 + 26.0 {
                        // Settled into the gravel uneaten. Rather than leaving a
                        // flake parked in a corner forever, it goes soggy and a
                        // fresh one drifts down later.
                        p.respawn = 5.0;
                    }
                    p.pos = region.clamp_inside(p.pos, 24.0);
                }
                PropKind::Plant | PropKind::Pebble => {}
            }
        }

        // A flake that has just come back starts near the surface, where food
        // would fall in from.
        for i in 0..self.props.len() {
            if self.props[i].kind == PropKind::Food && self.props[i].respawn > 0.0 {
                let (hw, hh) = self.region.half();
                let x = self.region.center.x + self.rng.range(-hw * 0.6, hw * 0.6);
                let p = &mut self.props[i];
                if p.pos.y < self.region.center.y + hh - 41.0 {
                    p.pos = Vec2::new(x, self.region.center.y + hh - 40.0);
                }
            }
        }

        self.retarget(dt, creature);
    }

    /// Pick something for the creature to head toward, and stick with it for a
    /// while. Re-picking every frame would make the creature jitter between
    /// props; holding one for several seconds reads as having decided.
    fn retarget(&mut self, dt: f32, creature: Vec2) {
        self.focus_timer -= dt;
        let stale = match self.focus {
            None => true,
            Some(i) => {
                self.focus_timer <= 0.0
                    || !self.props[i].present()
                    || hypot(creature.x - self.props[i].pos.x, creature.y - self.props[i].pos.y)
                        < self.props[i].radius + TOUCH
            }
        };
        if !stale {
            return;
        }
        // Food outranks the ball — a fish that ignores food is not a fish — and
        // scenery is not a target at all, so the creature spends most of its
        // time being a creature rather than orbiting the furniture.
        let rank = |k: PropKind| match k {
            PropKind::Food => 2,
            PropKind::Ball => 1,
            _ => 0,
        };
        let mut best: Option<(usize, i32)> = None;
        for (i, p) in self.props.iter().enumerate() {
            let r = rank(p.kind);
            if !p.present() || r == 0 {
                continue;
            }
            if best.map(|(_, br)| r > br).unwrap_or(true) {
                best = Some((i, r));
            }
        }
        self.focus = best.map(|(i, _)| i);
        self.focus_timer = self.rng.range(4.0, 9.0);
    }

    /// Where the creature is currently drawn to, if anywhere. This is the only
    /// thing the bodies ever learn about the props.
    pub fn attractor(&self) -> Option<Vec2> {
        self.focus
            .filter(|&i| self.props[i].present())
            .map(|i| self.props[i].pos)
    }
}

/// Default placement: a tank in the lower-right of the display, big enough to
/// be a home and small enough to leave the screen usable. Free-floating rather
/// than anchored to a window — see HABITAT_PLAN.md for why that decision is
/// still open.
pub fn default_region(display: (f32, f32)) -> Region {
    let w = clamp(display.0 * 0.42, 320.0, 760.0);
    let h = clamp(display.1 * 0.46, 240.0, 520.0);
    let margin = 24.0;
    Region::new(
        Vec2::new(
            display.0 / 2.0 - w / 2.0 - margin,
            -(display.1 / 2.0) + h / 2.0 + margin,
        ),
        (w, h),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole feature rests on this: a centred region must give the bodies
    /// the same numbers the old `±bounds/2` arithmetic did, or turning habitat
    /// mode *off* would no longer be free-roam.
    #[test]
    fn a_centred_region_reproduces_the_old_bounds_arithmetic() {
        let size = (1512.0, 982.0);
        let r = Region::centered(size);
        for &(x, y) in &[
            (700.0, 400.0),
            (-755.0, 491.0),
            (900.0, -600.0),
            (-100.0, 480.0),
        ] {
            let p = Vec2::new(x, y);
            let margin = 24.0;
            let hw = size.0 / 2.0 - margin;
            let hh = size.1 / 2.0 - margin;
            assert_eq!(
                r.outside(p, margin),
                p.x.abs() > hw || p.y.abs() > hh,
                "edge test disagrees at ({x}, {y})"
            );
            let c = r.clamp_inside(p, 20.0);
            assert_eq!(c.x, clamp(p.x, -size.0 / 2.0 + 20.0, size.0 / 2.0 - 20.0));
            assert_eq!(c.y, clamp(p.y, -size.1 / 2.0 + 20.0, size.1 / 2.0 - 20.0));
            assert_eq!(r.bearing_home(p), (-p.y).atan2(-p.x));
        }

        // The exact centre is the one point where the two formulations
        // disagree, and only over signed zero: `(-0.0).atan2(-0.0)` is -pi,
        // while `(0.0 - 0.0).atan2(0.0 - 0.0)` is 0.0. "Which way is home" is
        // undefined when you are already home, and no body can reach this case
        // anyway — it would have to be at the centre *and* within `margin` of a
        // wall, which needs a region narrower than two margins.
        assert_eq!(r.bearing_home(Vec2::ZERO), 0.0);
    }

    #[test]
    fn an_offset_region_confines_to_itself_not_to_the_origin() {
        let r = Region::new(Vec2::new(400.0, -250.0), (500.0, 300.0));
        let c = r.clamp_inside(Vec2::ZERO, 20.0);
        // Centre (400, -250), half (250, 150), margin 20 -> the inside runs
        // x 170..630 and y -380..-120. The origin is off to the upper left of
        // that, so it lands on the corner.
        assert!(
            (c.x - 170.0).abs() < 1e-3 && (c.y + 120.0).abs() < 1e-3,
            "clamped to ({}, {})",
            c.x,
            c.y
        );
        assert!(!r.outside(r.center, 20.0));
        assert!(r.outside(Vec2::ZERO, 20.0));
    }

    /// Substrate finally does something.
    #[test]
    fn the_swimmer_gets_water_and_everything_else_gets_soil() {
        assert_eq!(
            HabitatKind::for_substrate(Substrate::Swimmer),
            HabitatKind::Aquarium
        );
        for s in [
            Substrate::WalkerFlier,
            Substrate::WalkerJumper,
            Substrate::Crawler,
        ] {
            assert_eq!(HabitatKind::for_substrate(s), HabitatKind::Terrarium);
        }
    }

    #[test]
    fn props_are_stocked_inside_the_tank() {
        for kind in [HabitatKind::Aquarium, HabitatKind::Terrarium] {
            let r = Region::new(Vec2::new(300.0, -200.0), (600.0, 400.0));
            let h = Habitat::new(kind, r, 9);
            assert_eq!(h.props.len(), 4);
            for p in &h.props {
                assert!(
                    !r.outside(p.pos, 0.0),
                    "{:?} prop {:?} spawned outside the tank at ({}, {})",
                    kind,
                    p.kind,
                    p.pos.x,
                    p.pos.y
                );
            }
        }
        // The stock differs by kind, or "matched to the creature type" means
        // nothing.
        let r = Region::centered((600.0, 400.0));
        let aq = Habitat::new(HabitatKind::Aquarium, r, 1);
        let te = Habitat::new(HabitatKind::Terrarium, r, 1);
        assert!(aq.props.iter().any(|p| p.kind == PropKind::Food));
        assert!(!aq.props.iter().any(|p| p.kind == PropKind::Ball));
        assert!(te.props.iter().any(|p| p.kind == PropKind::Ball));
        assert!(!te.props.iter().any(|p| p.kind == PropKind::Food));
    }

    /// The ball is the "play with it" prop: walking into it must move it, and it
    /// must then come to rest rather than drifting forever.
    #[test]
    fn the_creature_pushes_the_ball_and_the_ball_settles() {
        let r = Region::centered((600.0, 400.0));
        let mut h = Habitat::new(HabitatKind::Terrarium, r, 3);
        let i = h
            .props
            .iter()
            .position(|p| p.kind == PropKind::Ball)
            .unwrap();
        h.props[i].pos = Vec2::ZERO;
        h.props[i].vel = Vec2::ZERO;

        // Creature arrives from the left and leans on it.
        for _ in 0..30 {
            h.step(1.0 / 60.0, Vec2::new(-20.0, 0.0), None);
        }
        let pushed = h.props[i].pos;
        assert!(
            pushed.x > 3.0,
            "the ball did not move away from the creature: ({}, {})",
            pushed.x,
            pushed.y
        );

        // Left alone it coasts to a stop, and stays inside the glass.
        for _ in 0..600 {
            h.step(1.0 / 60.0, Vec2::new(-2000.0, 0.0), None);
        }
        let v = hypot(h.props[i].vel.x, h.props[i].vel.y);
        assert!(v < 1.0, "the ball never settled: {v}");
        assert!(!r.outside(h.props[i].pos, 0.0), "the ball escaped the tank");
    }

    /// The other half of "notices, approaches": the fish is pointed at the food,
    /// and once it reaches it the flake is gone and stops being a target.
    #[test]
    fn food_is_a_target_until_it_is_eaten() {
        let r = Region::centered((600.0, 400.0));
        let mut h = Habitat::new(HabitatKind::Aquarium, r, 5);
        let i = h
            .props
            .iter()
            .position(|p| p.kind == PropKind::Food)
            .unwrap();
        h.props[i].pos = Vec2::new(100.0, 40.0);
        h.step(1.0 / 60.0, Vec2::new(-200.0, -100.0), None);
        let a = h.attractor().expect("the fish should be drawn to the flake");
        assert!(
            (a.x - h.props[i].pos.x).abs() < 1e-3,
            "attracted to the wrong prop"
        );

        // Swim into it.
        let at = h.props[i].pos;
        for _ in 0..5 {
            h.step(1.0 / 60.0, at, None);
        }
        assert!(!h.props[i].present(), "the flake was not eaten");
        assert!(
            h.attractor().is_none(),
            "an eaten flake is still being chased"
        );
    }

    /// A cursor passing over the glass stirs the plants — the only way the user
    /// touches the tank, since the overlay never takes a click.
    #[test]
    fn the_cursor_stirs_props_without_being_clicked() {
        let r = Region::centered((600.0, 400.0));
        let mut h = Habitat::new(HabitatKind::Terrarium, r, 2);
        let i = h
            .props
            .iter()
            .position(|p| p.kind == PropKind::Plant)
            .unwrap();
        let at = h.props[i].pos;
        for _ in 0..20 {
            h.step(1.0 / 60.0, Vec2::new(-2000.0, -2000.0), Some(at));
        }
        assert!(h.props[i].stir > 0.2, "the plant ignored the cursor");
        // And it is still anchored — a plant does not roll away.
        assert_eq!(h.props[i].pos.x, at.x);
        assert_eq!(h.props[i].pos.y, at.y);
    }

    #[test]
    fn the_default_tank_fits_on_the_display() {
        for display in [(1512.0, 982.0), (3840.0, 2160.0), (1280.0, 720.0)] {
            let r = default_region(display);
            let screen = Region::centered(display);
            assert!(!screen.outside(r.min(), 0.0), "tank hangs off the display");
            assert!(!screen.outside(r.max(), 0.0), "tank hangs off the display");
            assert!(r.size.0 >= 320.0 && r.size.1 >= 240.0);
        }
    }
}
