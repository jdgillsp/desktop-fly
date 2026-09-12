//! A small autonomous thumper expedition, present only in the worm's enclosure.
use super::prims::{blob, dome, slab};
use crate::mesh::Mesh;
use dfcore::{Region, SandwormBody, SandwormState, Vec2};

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn blend(a: Vec2, b: Vec2, t: f32) -> Vec2 {
    Vec2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn clear_segment(a: Vec2, b: Vec2, obstacles: &[(Vec2, f32)]) -> bool {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    obstacles.iter().all(|(c, r)| {
        let t = (((c.x - a.x) * dx + (c.y - a.y) * dy) / (dx * dx + dy * dy).max(0.001))
            .clamp(0.0, 1.0);
        c.dist(blend(a, b, t)) >= r + 3.0
    })
}

/// A small visibility graph gives the walker a complete route around props.
/// Ring nodes sit outside the clearance circle so their connecting chords
/// also leave room for the robe. The route is cached until its goal changes.
fn route(start: Vec2, goal: Vec2, obstacles: &[(Vec2, f32)], region: Region) -> Vec<Vec2> {
    if clear_segment(start, goal, obstacles) {
        return vec![goal];
    }
    let mut nodes = vec![start, goal];
    for (c, r) in obstacles {
        for i in 0..16 {
            let angle = std::f32::consts::TAU * i as f32 / 16.0;
            let p = Vec2::new(
                c.x + angle.cos() * (r + 4.0) / 0.98,
                c.y + angle.sin() * (r + 4.0) / 0.98,
            );
            if !region.outside(p, 3.0) {
                nodes.push(p);
            }
        }
    }
    let mut distance = vec![f32::INFINITY; nodes.len()];
    let mut previous = vec![usize::MAX; nodes.len()];
    let mut visited = vec![false; nodes.len()];
    distance[0] = 0.0;
    for _ in 0..nodes.len() {
        let Some(i) = (0..nodes.len())
            .filter(|&i| !visited[i] && distance[i].is_finite())
            .min_by(|&a, &b| distance[a].total_cmp(&distance[b]))
        else {
            break;
        };
        if i == 1 {
            break;
        }
        visited[i] = true;
        for j in 0..nodes.len() {
            let d = distance[i] + nodes[i].dist(nodes[j]);
            if !visited[j] && d < distance[j] && clear_segment(nodes[i], nodes[j], obstacles) {
                distance[j] = d;
                previous[j] = i;
            }
        }
    }
    if !distance[1].is_finite() {
        return Vec::new();
    }
    let mut path = vec![goal];
    let mut i = 1;
    while previous[i] != 0 {
        i = previous[i];
        path.push(nodes[i]);
    }
    path.reverse();
    path
}

pub(crate) fn thumper(out: &mut Mesh, at: Vec2, time: f32) {
    use super::prims::{ribbon, tube};
    let beat = (time * 1.5).fract();
    let piston = if beat < 0.15 {
        1.0 - beat / 0.15
    } else {
        (beat - 0.15) / 0.85
    };
    let metal = [0.32, 0.29, 0.23, 1.0];
    tube(
        out,
        [at.x, at.y, 0.0],
        [at.x, at.y, 9.0],
        1.4,
        12,
        true,
        metal,
    );
    tube(
        out,
        [at.x, at.y, 8.0],
        [at.x, at.y, 11.0 + piston * 3.0],
        0.65,
        10,
        true,
        [0.62, 0.60, 0.52, 1.0],
    );
    tube(
        out,
        [at.x, at.y, 10.0 + piston * 3.0],
        [at.x, at.y, 11.5 + piston * 3.0],
        2.1,
        12,
        true,
        metal,
    );
    for offset in [0.0, 0.5] {
        let age = (time * 0.75 - 0.075 + offset).rem_euclid(1.0);
        let radius = 3.0 + age * 32.0;
        let points: Vec<_> = (0..=48)
            .map(|i| {
                let a = std::f32::consts::TAU * i as f32 / 48.0;
                Vec2::new(at.x + radius * a.cos(), at.y + radius * a.sin())
            })
            .collect();
        ribbon(
            out,
            &points,
            0.18,
            0.6,
            [0.65, 0.45, 0.23, (1.0 - age) * 0.35],
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Rest,
    Walk,
    Plant,
    Call,
    Approach,
    Mount,
    Ride,
    Dismount,
    Return,
}

pub struct Expedition {
    phase: Phase,
    timer: f32,
    region: Region,
    at: Vec2,
    height: f32,
    from: Vec2,
    from_height: f32,
    landing: Vec2,
    obstacles: Vec<(Vec2, f32)>,
    route: Vec<Vec2>,
    destination: Option<Vec2>,
    facing: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scout_routes_around_structures_and_refuses_blocked_goals() {
        let region = Region::centered((400.0, 300.0));
        let obstacles = vec![(Vec2::ZERO, 24.0), (Vec2::new(40.0, 35.0), 15.0)];
        let start = Vec2::new(-70.0, 0.0);
        let goal = Vec2::new(80.0, 0.0);
        let path = route(start, goal, &obstacles, region);
        assert_eq!(path.last(), Some(&goal));
        let mut p = start;
        for q in path {
            assert!(clear_segment(p, q, &obstacles));
            p = q;
        }
        assert!(route(start, Vec2::ZERO, &obstacles, region).is_empty());
    }
    #[test]
    fn boarding_and_cancelled_ride_reach_ground_without_teleporting() {
        let region = Region::centered((400.0, 300.0));
        let mut worm = SandwormBody::new(Vec2::ZERO, 42);
        worm.surface = 1.0;
        let mut e = Expedition::new(region);
        e.phase = Phase::Approach;
        e.timer = 10.0;
        e.at = worm.spine[8];
        let mut thumper = None;
        let mut mounted = false;
        for _ in 0..300 {
            let (p, z) = (e.at, e.height);
            e.update(0.02, region, &mut worm, &mut thumper);
            assert!(p.dist(e.at) < 1.0 && (z - e.height).abs() < 0.5);
            if e.phase == Phase::Ride {
                mounted = true;
                break;
            }
        }
        assert!(mounted);
        e.cancel();
        assert_eq!(e.phase, Phase::Dismount);
        for _ in 0..100 {
            let (p, z) = (e.at, e.height);
            e.update(0.02, region, &mut worm, &mut thumper);
            assert!(p.dist(e.at) < 1.0 && (z - e.height).abs() < 0.5);
            if e.phase == Phase::Return {
                break;
            }
        }
        assert_eq!(e.phase, Phase::Return);
        assert_eq!(e.height, 0.0);
    }
    #[test]
    fn expedition_summons_rides_and_returns() {
        use dfcore::creature::{Body, World};
        let region = Region::centered((400.0, 300.0));
        let mut worm = SandwormBody::new(region.center, 42);
        let mut e = Expedition::new(region);
        let mut thumper = None;
        for _ in 0..2000 {
            e.update(0.02, region, &mut worm, &mut thumper);
            if thumper.is_some() {
                break;
            }
        }
        assert_eq!(e.phase, Phase::Call);
        let target = thumper.unwrap();
        let mut drives = dfcore::BrainSignals::new();
        drives.pursuit = 1.0;
        let world = World {
            region,
            ledges: Vec::new(),
            cursor: None,
            attractor: Some(target),
        };
        for _ in 0..4500 {
            worm.step(0.02, &drives, &world);
            if worm.swallowed {
                thumper = None;
            }
            e.update(0.02, region, &mut worm, &mut thumper);
            if e.phase == Phase::Ride {
                break;
            }
        }
        assert_eq!(
            e.phase,
            Phase::Ride,
            "worm {:?} heading {} state {:?} target {:?}",
            worm.pos,
            worm.heading,
            worm.state,
            target
        );
        worm.swallowed = false;
        for _ in 0..4000 {
            e.update(0.02, region, &mut worm, &mut thumper);
            if e.phase == Phase::Rest {
                break;
            }
        }
        assert_eq!(e.phase, Phase::Rest);
        assert_eq!(e.at, e.cave());
        assert!(thumper.is_none());
    }
    #[test]
    fn manual_thumper_is_preserved_when_scout_is_cancelled() {
        let region = Region::centered((400.0, 300.0));
        let mut e = Expedition::new(region);
        let mut worm = SandwormBody::new(region.center, 1);
        e.phase = Phase::Call;
        e.cancel();
        let mut thumper = Some(region.center);
        e.update(0.1, region, &mut worm, &mut thumper);
        assert_eq!(thumper, Some(region.center));
    }
}

impl Expedition {
    pub fn activity(&self) -> &'static str {
        match self.phase {
            Phase::Rest => "in cave",
            Phase::Walk => "walking to thumper site",
            Phase::Plant => "planting thumper",
            Phase::Call => "calling worm",
            Phase::Approach => "approaching worm",
            Phase::Mount => "climbing aboard",
            Phase::Ride => "riding",
            Phase::Dismount => "dismounting",
            Phase::Return => "returning to cave",
        }
    }
    pub fn new(region: Region) -> Self {
        let mut s = Self {
            phase: Phase::Rest,
            timer: 5.0,
            region,
            at: region.center,
            height: 0.0,
            from: region.center,
            from_height: 0.0,
            landing: region.center,
            obstacles: Vec::new(),
            route: Vec::new(),
            destination: None,
            facing: -std::f32::consts::FRAC_PI_2,
        };
        s.at = s.cave();
        s
    }
    fn cave(&self) -> Vec2 {
        let lo = self.region.min();
        Vec2::new(
            lo.x + self.region.size.0 * 0.78,
            lo.y + self.region.size.1 * 0.78,
        )
    }
    pub fn footprint(&self) -> (Vec2, f32) {
        (self.cave(), 24.0)
    }
    fn site(&self) -> Vec2 {
        let lo = self.region.min();
        Vec2::new(
            lo.x + self.region.size.0 * 0.60,
            lo.y + self.region.size.1 * 0.52,
        )
    }
    fn walk(&mut self, target: Vec2, dt: f32) -> bool {
        if self.destination.map_or(true, |p| p.dist(target) > 2.0) {
            self.route = route(self.at, target, &self.obstacles, self.region);
            self.destination = Some(target);
        }
        let Some(&waypoint) = self.route.first() else {
            return self.at.dist(target) < 0.5;
        };
        let dx = waypoint.x - self.at.x;
        let dy = waypoint.y - self.at.y;
        let distance = (dx * dx + dy * dy).sqrt();
        self.facing = dy.atan2(dx);
        let speed = if self.phase == Phase::Approach {
            42.0
        } else {
            16.0
        };
        if distance <= dt * speed + 0.1 {
            self.at = waypoint;
            self.route.remove(0);
            return self.at.dist(target) < 0.5;
        }
        self.at.x += dx / distance * dt * speed;
        self.at.y += dy / distance * dt * speed;
        false
    }
    pub fn cancel(&mut self) {
        self.phase = if self.height > 0.1 {
            Phase::Dismount
        } else {
            Phase::Return
        };
        self.timer = -1.0;
        self.destination = None;
    }
    pub fn update(
        &mut self,
        dt: f32,
        region: Region,
        worm: &mut SandwormBody,
        thumper: &mut Option<Vec2>,
    ) {
        // Carry the walker along with enclosure moves and resizes.
        let old = self.region.min();
        let new = region.min();
        let carry = |p: Vec2| {
            Vec2::new(
                new.x + (p.x - old.x) * region.size.0 / self.region.size.0.max(1.0),
                new.y + (p.y - old.y) * region.size.1 / self.region.size.1.max(1.0),
            )
        };
        self.from = carry(self.from);
        self.landing = carry(self.landing);
        if old != new || self.region.size != region.size {
            self.destination = None;
        }
        self.at = Vec2::new(
            new.x + (self.at.x - old.x) * region.size.0 / self.region.size.0.max(1.0),
            new.y + (self.at.y - old.y) * region.size.1 / self.region.size.1.max(1.0),
        );
        self.region = region;
        let obstacles: Vec<_> = worm
            .obstacles
            .iter()
            .copied()
            .filter(|(c, _)| c.dist(self.cave()) > 1.0)
            .collect();
        if obstacles != self.obstacles {
            self.obstacles = obstacles;
            self.destination = None;
        }
        self.timer -= dt;
        match self.phase {
            Phase::Rest => {
                self.at = self.cave();
                if self.timer <= 0.0 && thumper.is_none() {
                    self.phase = Phase::Walk;
                }
            }
            Phase::Walk => {
                if thumper.is_some() {
                    self.phase = Phase::Return;
                } else if self.walk(Vec2::new(self.site().x + 5.0, self.site().y), dt) {
                    self.phase = Phase::Plant;
                    self.timer = 1.6;
                }
            }
            Phase::Plant => {
                if thumper.is_some() {
                    self.phase = Phase::Return;
                } else if self.timer <= 0.0 {
                    *thumper = Some(self.site());
                    self.phase = Phase::Call;
                    self.timer = 90.0;
                }
            }
            Phase::Call => {
                let p = worm.pos;
                let d = ((p.x - self.site().x).powi(2) + (p.y - self.site().y).powi(2)).sqrt();
                if worm.swallowed && d < 40.0 {
                    self.phase = Phase::Approach;
                    self.timer = 12.0;
                } else if self.timer <= 0.0 || thumper.is_none() {
                    *thumper = None;
                    self.phase = Phase::Return;
                } else {
                    self.walk(Vec2::new(self.site().x + 18.0, self.site().y), dt);
                }
            }
            Phase::Approach => {
                worm.state = SandwormState::Cruise;
                worm.state_timer = 2.0;
                self.walk(worm.spine[8], dt);
                if self.at.dist(worm.spine[8]) < 3.0 {
                    self.phase = Phase::Mount;
                    self.timer = 1.5;
                    self.from = self.at;
                } else if self.timer <= 0.0 {
                    self.phase = Phase::Return;
                }
            }
            Phase::Mount => {
                worm.state = SandwormState::Cruise;
                worm.state_timer = 2.0;
                let t = smooth(1.0 - self.timer / 1.5);
                self.at = blend(self.from, worm.spine[8], t);
                self.height = crate::sandwormbody::riding_height(worm, 8) * t;
                if self.timer <= 0.0 {
                    self.phase = Phase::Ride;
                    self.timer = 14.0;
                }
            }
            Phase::Ride => {
                self.facing = worm.heading;
                self.at = worm.spine[8];
                self.height = crate::sandwormbody::riding_height(worm, 8);
                if self.timer <= 0.0 || worm.state == SandwormState::Dive {
                    self.phase = Phase::Dismount;
                    self.timer = -1.0;
                } else {
                    worm.state = SandwormState::Cruise;
                    worm.state_timer = 2.0;
                }
            }
            Phase::Dismount => {
                if self.timer < -0.5 {
                    self.from = self.at;
                    self.from_height = self.height;
                    self.landing = (0..16)
                        .map(|i| {
                            let angle = worm.heading + std::f32::consts::TAU * i as f32 / 16.0;
                            Vec2::new(
                                self.at.x + angle.cos() * 18.0,
                                self.at.y + angle.sin() * 18.0,
                            )
                        })
                        .find(|p| {
                            !region.outside(*p, 4.0)
                                && self.obstacles.iter().all(|(c, r)| p.dist(*c) > r + 4.0)
                        })
                        .unwrap_or(self.at);
                    self.timer = 1.2;
                }
                let t = smooth(1.0 - self.timer / 1.2);
                self.at = blend(self.from, self.landing, t);
                self.height = self.from_height * (1.0 - t) + (std::f32::consts::PI * t).sin() * 2.0;
                if self.timer <= 0.02 {
                    self.at = self.landing;
                    self.height = 0.0;
                    self.phase = Phase::Return;
                    self.destination = None;
                }
            }
            Phase::Return => {
                if self.walk(self.cave(), dt) {
                    self.phase = Phase::Rest;
                    self.timer = 35.0;
                }
            }
        }
    }
    pub fn build(&self, out: &mut Mesh, worm: &SandwormBody) {
        let c = self.cave();
        let rock = [0.55, 0.39, 0.23, 1.0];
        // Two shoulders and a lintel leave a real open cave mouth.
        for x in [-12.0, 12.0] {
            blob(out, Vec2::new(c.x + x, c.y), 0.0, 8.5, 15.0, 0.18, x, rock);
        }
        blob(
            out,
            Vec2::new(c.x, c.y + 2.0),
            12.0,
            15.0,
            7.0,
            0.12,
            4.0,
            rock,
        );
        slab(
            out,
            [c.x - 6.0, c.y + 6.0, 0.0],
            [c.x + 6.0, c.y + 7.0, 12.0],
            [0.09, 0.065, 0.04, 1.0],
        );
        if self.phase == Phase::Rest {
            return;
        }
        if self.phase == Phase::Plant {
            let at = self.site();
            let z = (self.timer / 1.6).clamp(0.0, 1.0) * 4.0;
            super::prims::tube(
                out,
                [at.x, at.y, z],
                [at.x, at.y, z + 11.0],
                1.4,
                12,
                true,
                [0.32, 0.29, 0.23, 1.0],
            );
        }
        let riding = matches!(self.phase, Phase::Mount | Phase::Ride | Phase::Dismount);
        let p = self.at;
        let scout_start = out.verts.len();
        let z = self.height;
        let crouch = if self.phase == Phase::Plant {
            (std::f32::consts::PI * (1.0 - self.timer / 1.6))
                .sin()
                .max(0.0)
                * 2.5
        } else {
            0.0
        };
        let robe = [0.25, 0.22, 0.17, 1.0];
        dome(out, p, z + 1.0, 2.6, 6.2 - crouch, robe);
        dome(out, p, z + 6.0 - crouch, 1.8, 2.5, [0.42, 0.35, 0.25, 1.0]);
        slab(
            out,
            [p.x - 1.2, p.y - 1.55, z + 7.0 - crouch],
            [p.x + 1.2, p.y - 1.3, z + 7.5 - crouch],
            [0.12, 0.42, 0.65, 1.0],
        );
        for side in [-1.0, 1.0] {
            let sway = if !matches!(self.phase, Phase::Walk | Phase::Return | Phase::Approach) {
                0.0
            } else {
                (worm.time * 7.0).sin() * side
            };
            slab(
                out,
                [p.x + side * 1.3 - 0.4, p.y + sway - 0.5, z],
                [p.x + side * 1.3 + 0.4, p.y + sway + 0.5, z + 2.0],
                robe,
            );
            if riding {
                slab(
                    out,
                    [p.x + side * 3.0 - 0.2, p.y - 0.2, z - 2.0],
                    [p.x + side * 3.0 + 0.2, p.y + 0.2, z + 5.0],
                    [0.55, 0.55, 0.49, 1.0],
                );
                super::prims::tube(
                    out,
                    [p.x + side * 1.6, p.y, z + 4.5],
                    [p.x + side * 3.0, p.y, z + 5.0],
                    0.45,
                    8,
                    true,
                    robe,
                );
                super::prims::tube(
                    out,
                    [p.x + side * 3.0, p.y, z - 2.0],
                    [p.x + side * 2.2, p.y, z - 2.6],
                    0.2,
                    8,
                    true,
                    [0.55, 0.55, 0.49, 1.0],
                );
            }
        }
        let angle = self.facing + std::f32::consts::FRAC_PI_2;
        let (s, c) = angle.sin_cos();
        for v in &mut out.verts[scout_start..] {
            let (x, y) = (v.pos[0] - p.x, v.pos[1] - p.y);
            v.pos[0] = p.x + c * x - s * y;
            v.pos[1] = p.y + s * x + c * y;
            let (x, y) = (v.normal[0], v.normal[1]);
            v.normal[0] = c * x - s * y;
            v.normal[1] = s * x + c * y;
        }
    }
}
