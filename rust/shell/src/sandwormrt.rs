//! Creature #9 on the desktop: a sandworm, driven by rules — and by rhythm.
//!
//! No simulation, no brain window, `PROCEDURAL` in the tray, exactly as for
//! the koi and the hognose. What is different is the sense. A sandworm is not
//! startled by anything; it is *called*. In the fiction the call is a
//! thumper: a stake that pounds the sand at a fixed cadence. Here it is the
//! user's clicks — when they come at a steady interval, the worm treats the
//! place they land as a thumper and goes there — and, more weakly, typing,
//! which is a rhythm too, located at the cursor.
//!
//! Two more things from the books. The worm **eats** the thumper: when it
//! breaches over the lure the beats are forgotten, and it takes a fresh
//! rhythm to call it again. And Fremen cross open sand with the *sandwalk*,
//! an arrhythmic step, because a regular tread calls a worm — so a cursor
//! moving at a steady pace for a couple of seconds is a weak lure, and one
//! moving erratically is nothing at all.
//!
//! The detector is content-blind by construction, like every other sense:
//! it sees click *times* and *positions*, cursor speed, and the typing
//! level, and nothing about what was clicked, typed or pointed at.

use std::collections::VecDeque;

use dfcore::creature::{Body, World};
use dfcore::data::BrainPointsFile;
use dfcore::env::thermal_tempo;
use dfcore::util::clamp;
use dfcore::Region;
use dfcore::{
    circadian_activity, BrainSignals, Creature, EnvSnapshot, Habituation, SandwormBody, Sim, Vec2,
};

use crate::mesh::Mesh;
use crate::runtime::{Geometry, Runtime};
use crate::sandwormbody;

/// Clicks older than this are not part of a rhythm.
const RHYTHM_WINDOW: f32 = 5.0;
/// How many clicks it takes before an interval can be called regular.
const MIN_BEATS: usize = 3;
/// How long a steady cursor pace has to hold before it reads as a tread.
const TREAD_WINDOW: f32 = 2.0;

pub struct SandwormRuntime {
    creature: dfcore::creature::Sandworm,
    worm: SandwormBody,
    habituation: Habituation,
    frame_mesh: Mesh,

    /// Recent clicks: (age in seconds, position). Ages advance each sense.
    beats: VecDeque<(f32, Vec2)>,
    /// The rhythm's strength this poll, 0..1, and where it is.
    thumper: f32,
    thumper_at: Option<Vec2>,
    /// Smoothed, so a single missed beat does not lose the worm.
    thumper_ema: f32,
    /// Recent cursor speeds, oldest first, with their ages.
    tread: VecDeque<(f32, f32)>,
    prev_cursor: Option<Vec2>,
    forced_dive: bool,

    pending_tempo: f32,
    pending_sleepy: bool,
    activity: f32,
}

impl SandwormRuntime {
    pub fn new(seed: u64) -> Self {
        let creature = dfcore::creature::Sandworm;
        println!(
            "{} - {}",
            creature.display_name(),
            creature.provenance().describe()
        );
        if let dfcore::Provenance::Procedural { why, .. } = creature.provenance() {
            println!("note: {why}");
        }
        SandwormRuntime {
            creature,
            worm: SandwormBody::new(Vec2::ZERO, seed),
            habituation: Habituation::new(),
            frame_mesh: Mesh::default(),
            beats: VecDeque::new(),
            thumper: 0.0,
            thumper_at: None,
            thumper_ema: 0.0,
            tread: VecDeque::new(),
            prev_cursor: None,
            forced_dive: false,
            pending_tempo: 1.0,
            pending_sleepy: false,
            activity: 1.0,
        }
    }

    /// How regular the recent clicks are, 0..1, and their centre.
    ///
    /// Regularity is one minus the coefficient of variation of the
    /// intervals, so three clicks a second apart score near 1 and three
    /// clicks at random score near 0. A double-click is two beats a few
    /// milliseconds apart: too fast to be pounding, and discarded.
    fn rhythm(&self) -> (f32, Option<Vec2>) {
        let mut ages: Vec<f32> = self.beats.iter().map(|b| b.0).collect();
        ages.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let intervals: Vec<f32> = ages
            .windows(2)
            .map(|w| w[1] - w[0])
            .filter(|&d| d > 0.12)
            .collect();
        if intervals.len() + 1 < MIN_BEATS {
            return (0.0, None);
        }
        let mean = intervals.iter().sum::<f32>() / intervals.len() as f32;
        if !(0.15..=2.5).contains(&mean) {
            return (0.0, None);
        }
        let var = intervals.iter().map(|d| (d - mean).powi(2)).sum::<f32>() / intervals.len() as f32;
        let cv = var.sqrt() / mean;
        let regularity = clamp(1.0 - cv * 2.5, 0.0, 1.0);
        let n = self.beats.len() as f32;
        let centre = self.beats.iter().fold(Vec2::ZERO, |acc, b| {
            Vec2::new(acc.x + b.1.x / n, acc.y + b.1.y / n)
        });
        (regularity, Some(centre))
    }

    /// A regular tread: the cursor moving at a steady pace. One minus the
    /// coefficient of variation of its speed over the window, and only while
    /// it is actually moving at a walking sort of pace.
    fn tread_regularity(&self) -> f32 {
        if self.tread.len() < 20 {
            return 0.0;
        }
        let n = self.tread.len() as f32;
        let mean = self.tread.iter().map(|t| t.1).sum::<f32>() / n;
        if !(80.0..=900.0).contains(&mean) {
            return 0.0;
        }
        let var = self.tread.iter().map(|t| (t.1 - mean).powi(2)).sum::<f32>() / n;
        let cv = var.sqrt() / mean;
        clamp(1.0 - cv * 3.0, 0.0, 1.0)
    }

    fn drives(&mut self) -> BrainSignals {
        let mut s = BrainSignals::new();
        s.tempo = self.pending_tempo;
        s.sleep = self.pending_sleepy;
        s.walk_drive = clamp(self.activity, 0.15, 1.0);
        s.pursuit = clamp(self.thumper_ema, 0.0, 1.0);
        s.escape = self.forced_dive;
        self.forced_dive = false;
        s.arousal = s.pursuit;
        s
    }
}

impl Runtime for SandwormRuntime {
    fn creature(&self) -> &dyn Creature {
        &self.creature
    }
    fn substrate(&self) -> dfcore::Substrate {
        dfcore::creature::Body::substrate(&self.worm)
    }

    fn sim(&self) -> Option<&dyn Sim> {
        None
    }
    fn sim_mut(&mut self) -> Option<&mut dyn Sim> {
        None
    }
    fn brain_points(&self) -> Option<&BrainPointsFile> {
        None
    }
    fn brain_info(&self) -> String {
        "PROCEDURAL - no connectome, fictional animal, hand-written behaviour".to_string()
    }

    fn habituation(&self) -> &Habituation {
        &self.habituation
    }
    fn habituation_mut(&mut self) -> &mut Habituation {
        &mut self.habituation
    }

    /// The escape test sends it under.
    fn scare(&mut self) {
        self.forced_dive = true;
    }

    /// In a terrarium there is a water dish, and water is poison to a
    /// sandworm. The body is told where it is; on the open desktop there is
    /// no water anywhere.
    fn habitat_changed(&mut self, habitat: Option<&dfcore::Habitat>) {
        self.worm.hazard = habitat
            .filter(|h| h.kind == dfcore::HabitatKind::SandTerrarium)
            .map(|h| crate::habitatmesh::terrarium::water_dish(&h.region));
    }

    fn sense(&mut self, env: &EnvSnapshot, dt: f32) {
        for b in self.beats.iter_mut() {
            b.0 += dt;
        }
        while self.beats.front().map(|b| b.0 > RHYTHM_WINDOW).unwrap_or(false) {
            self.beats.pop_front();
        }
        for c in &env.clicks {
            self.beats.push_back((0.0, *c));
        }
        let (reg, at) = self.rhythm();
        self.thumper = reg;
        self.thumper_at = at;
        // A steady tread across the sand.
        for t in self.tread.iter_mut() {
            t.0 += dt;
        }
        while self.tread.front().map(|t| t.0 > TREAD_WINDOW).unwrap_or(false) {
            self.tread.pop_front();
        }
        if let Some(m) = env.cursor {
            if let Some(p) = self.prev_cursor {
                if dt > 0.0 {
                    self.tread.push_back((0.0, m.dist(p) / dt));
                }
            }
            self.prev_cursor = Some(m);
            let tread = self.tread_regularity() * 0.45;
            if tread > self.thumper {
                self.thumper = tread;
                self.thumper_at = Some(m);
            }
            // Typing is a weaker rhythm, at the cursor.
            if env.typing_level > 0.5 && self.thumper < 0.4 {
                self.thumper = 0.4 * env.typing_level;
                self.thumper_at = Some(m);
            }
        } else {
            self.prev_cursor = None;
        }
        let rate = if self.thumper > self.thumper_ema { 4.0 } else { 0.6 };
        self.thumper_ema += (self.thumper - self.thumper_ema) * (rate * dt).min(1.0);

        // A worm does not learn to fear you; there is nothing to habituate.
        // The record is kept, unchanged, so the tray's mood line is honest
        // about it rather than absent.
        self.habituation.step(dt, 0.0, 0.0);

        self.activity = circadian_activity(env.local_hour);
        self.pending_tempo = thermal_tempo(env.machine_heat);
        self.pending_sleepy = env.idle_secs > 1800.0;
    }

    fn tick(&mut self, dt: f32, region: Region, cursor: Option<Vec2>, attractor: Option<Vec2>) {
        let drives = self.drives();
        // The thumper is the attractor while it is going; otherwise whatever
        // the enclosure offers.
        let lure = if drives.pursuit > 0.2 { self.thumper_at } else { None };
        let world = World {
            region,
            ledges: Vec::new(),
            cursor,
            attractor: lure.or(attractor),
        };
        self.worm.step(dt, &drives, &world);
        if self.worm.swallowed {
            // The thumper is gone. So is the rhythm that was it.
            self.beats.clear();
            self.tread.clear();
            self.thumper = 0.0;
            self.thumper_ema = 0.0;
            self.thumper_at = None;
        }
    }

    fn build(&mut self, glass: bool) -> Geometry<'_> {
        sandwormbody::build_frame(&mut self.frame_mesh, &self.worm, glass);
        Geometry {
            body: &self.frame_mesh,
            neurons: None,
        }
    }

    fn position(&self) -> Vec2 {
        self.worm.pos
    }

    fn place(&mut self, at: Vec2) {
        let seed = (at.x.abs() as u64) ^ ((at.y.abs() as u64) << 8) ^ 0x5A4D;
        let hazard = self.worm.hazard;
        self.worm = SandwormBody::new(at, seed);
        self.worm.hazard = hazard;
    }

    fn moved_display(&mut self, region: Region) {
        let p = self.worm.pos;
        let clamped = region.clamp_inside(p, 80.0);
        if clamped != p {
            self.place(clamped);
        }
    }

    fn status(&self) -> String {
        format!(
            "{:?} pos ({:.0},{:.0}) speed {:.0} surface {:.2} rear {:.2} thumper {:.2}",
            self.worm.state,
            self.worm.pos.x,
            self.worm.pos.y,
            self.worm.speed,
            self.worm.surface,
            self.worm.rear,
            self.thumper_ema
        )
    }

    fn snapshot_pose(&mut self, _alt: f32, frames: u32, region: Region) {
        // Crawl, then breach: the reared, gaping head is the picture.
        let drives = BrainSignals::new();
        let world = World {
            region,
            ledges: Vec::new(),
            cursor: None,
            attractor: None,
        };
        for _ in 0..frames.max(30) {
            self.worm.step(1.0 / 60.0, &drives, &world);
        }
        self.worm.breach();
        for _ in 0..75 {
            self.worm.step(1.0 / 60.0, &drives, &world);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dfcore::SandwormState;

    #[test]
    fn the_sandworm_never_claims_to_have_a_brain() {
        let rt = SandwormRuntime::new(1);
        assert!(rt.sim().is_none());
        assert!(rt.brain_points().is_none());
        let info = rt.brain_info();
        assert!(info.to_uppercase().contains("PROCEDURAL"), "{info}");
        assert!(info.to_lowercase().contains("fictional"), "{info}");
        assert!(!info.to_lowercase().contains("flywire"), "{info}");
    }

    #[test]
    fn the_glass_register_shows_no_neurons() {
        let mut rt = SandwormRuntime::new(2);
        let g = rt.build(true);
        assert!(g.neurons.is_none());
        assert!(!g.body.verts.is_empty());
    }

    fn click_env(at: Option<Vec2>) -> EnvSnapshot {
        EnvSnapshot {
            clicks: at.into_iter().collect(),
            local_hour: 12.0,
            ..Default::default()
        }
    }

    /// Steady clicks at one spot are a thumper: the worm goes there and
    /// breaches. Random clicks are not.
    #[test]
    fn a_steady_beat_calls_it_and_a_random_one_does_not() {
        let bounds = Region::centered((1512.0, 982.0));
        let dt = 1.0 / 30.0;
        let spot = Vec2::new(320.0, 60.0);

        let mut called = SandwormRuntime::new(3);
        called.place(Vec2::new(-300.0, -100.0));
        called.worm.heading = std::f32::consts::PI;
        let mut breached = false;
        let mut nearest = f32::MAX;
        // One click every 0.5 s (every 15th poll) for 40 s.
        for i in 0..1200 {
            let env = click_env(if i % 15 == 0 { Some(spot) } else { None });
            called.sense(&env, dt);
            called.tick(dt, bounds, None, None);
            nearest = nearest.min(called.worm.pos.dist(spot));
            breached |= called.worm.state == SandwormState::Breach;
        }
        assert!(called.thumper_ema > 0.5, "no rhythm detected: {}", called.thumper_ema);
        assert!(nearest < 60.0, "never came: nearest {nearest:.0}");
        assert!(breached, "came but did not breach");

        let mut ignored = SandwormRuntime::new(3);
        ignored.place(Vec2::new(-300.0, -100.0));
        // Clicks at irregular intervals: 0.1, 1.4, 0.3, 2.0 s ...
        let gaps = [3usize, 42, 9, 60, 5, 30, 12, 50];
        let mut next = 0usize;
        let mut g = 0usize;
        for i in 0..1200 {
            let env = if i == next {
                next += gaps[g % gaps.len()];
                g += 1;
                click_env(Some(spot))
            } else {
                click_env(None)
            };
            ignored.sense(&env, dt);
            ignored.tick(dt, bounds, None, None);
        }
        assert!(
            ignored.thumper_ema < 0.4,
            "random clicks read as a thumper: {}",
            ignored.thumper_ema
        );
    }

    /// Once it has the thumper, the rhythm that called it is gone, and it
    /// does not sit on the spot waiting for more.
    #[test]
    fn it_eats_the_thumper_and_forgets_it() {
        let bounds = Region::centered((1512.0, 982.0));
        let dt = 1.0 / 30.0;
        let spot = Vec2::new(60.0, 0.0);
        let mut rt = SandwormRuntime::new(11);
        rt.place(Vec2::ZERO);
        rt.worm.heading = 0.0;
        let mut ate = false;
        for i in 0..900 {
            let env = click_env(if i % 15 == 0 { Some(spot) } else { None });
            rt.sense(&env, dt);
            rt.tick(dt, bounds, None, None);
            if rt.worm.swallowed {
                ate = true;
                assert!(rt.beats.is_empty(), "the beats survived being eaten");
                assert_eq!(rt.thumper_ema, 0.0);
            }
        }
        assert!(ate, "never reached the thumper");
    }

    /// A cursor moving at a steady pace is a regular tread; an erratic one
    /// is the sandwalk, and calls nothing.
    #[test]
    fn a_steady_tread_calls_it_and_the_sandwalk_does_not() {
        let dt = 1.0 / 30.0;
        let mut steady = SandwormRuntime::new(13);
        let mut x = -300.0f32;
        for _ in 0..120 {
            x += 10.0; // 300 units/s, constant
            let env = EnvSnapshot {
                cursor: Some(Vec2::new(x, 0.0)),
                local_hour: 12.0,
                ..Default::default()
            };
            steady.sense(&env, dt);
        }
        assert!(steady.thumper > 0.3, "a steady tread was not heard: {}", steady.thumper);

        let mut erratic = SandwormRuntime::new(13);
        let mut x = -300.0f32;
        let steps = [2.0f32, 25.0, 0.0, 9.0, 40.0, 1.0, 0.0, 18.0, 3.0, 30.0];
        for i in 0..120 {
            x += steps[i % steps.len()];
            let env = EnvSnapshot {
                cursor: Some(Vec2::new(x, 0.0)),
                local_hour: 12.0,
                ..Default::default()
            };
            erratic.sense(&env, dt);
        }
        assert!(erratic.thumper < 0.15, "the sandwalk called a worm: {}", erratic.thumper);
    }

    /// The terrarium's dish reaches the body as a hazard; the open desktop
    /// has none, and a tank of another kind has none either.
    #[test]
    fn the_terrarium_dish_is_a_hazard_and_free_roam_has_none() {
        let mut rt = SandwormRuntime::new(15);
        assert!(rt.worm.hazard.is_none());
        let h = dfcore::Habitat::new(
            dfcore::HabitatKind::SandTerrarium,
            Region::new(Vec2::new(100.0, -50.0), (600.0, 400.0)),
            1,
        );
        rt.habitat_changed(Some(&h));
        let (at, r) = rt.worm.hazard.expect("no hazard from the terrarium");
        assert!(r > 10.0);
        assert!(at.x < h.region.center.x, "the dish should be at the cool end");
        rt.place(Vec2::ZERO);
        assert!(rt.worm.hazard.is_some(), "placing the worm lost the water");
        rt.habitat_changed(None);
        assert!(rt.worm.hazard.is_none());
    }

    /// A double-click is not two beats of a rhythm.
    #[test]
    fn a_double_click_is_not_a_beat() {
        let mut rt = SandwormRuntime::new(5);
        let dt = 1.0 / 60.0;
        let spot = Vec2::new(100.0, 100.0);
        rt.sense(&click_env(Some(spot)), dt);
        rt.sense(&click_env(Some(spot)), dt);
        rt.sense(&click_env(Some(spot)), dt);
        assert_eq!(rt.thumper, 0.0, "three clicks in 50 ms are one click");
    }

    #[test]
    fn the_tray_escape_test_sends_it_under() {
        let mut rt = SandwormRuntime::new(7);
        rt.place(Vec2::ZERO);
        rt.worm.breach();
        for _ in 0..120 {
            rt.tick(1.0 / 60.0, Region::centered((1512.0, 982.0)), None, None);
        }
        assert!(rt.worm.surface > 0.5);
        rt.scare();
        rt.tick(1.0 / 60.0, Region::centered((1512.0, 982.0)), None, None);
        assert_eq!(rt.worm.state, SandwormState::Dive);
    }

    /// Nothing about the cursor frightens it.
    #[test]
    fn a_lunging_cursor_does_nothing() {
        let mut rt = SandwormRuntime::new(9);
        rt.place(Vec2::ZERO);
        rt.worm.breach();
        let dt = 1.0 / 30.0;
        let mut x = 200.0f32;
        for _ in 0..90 {
            let env = EnvSnapshot {
                cursor: Some(Vec2::new(x, 0.0)),
                local_hour: 12.0,
                ..Default::default()
            };
            rt.sense(&env, dt);
            rt.tick(dt, Region::centered((1512.0, 982.0)), None, None);
            x -= 20.0;
        }
        assert!(
            matches!(rt.worm.state, SandwormState::Breach | SandwormState::Cruise),
            "the worm reacted to a cursor: {:?}",
            rt.worm.state
        );
    }
}
