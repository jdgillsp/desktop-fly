//! Creature #8 on the desktop: a hognose snake, driven by rules.
//!
//! The koi's arrangement exactly: no simulation, `sim()` is `None`, the brain
//! window stays shut and the tray says `PROCEDURAL, no connectome`. A small,
//! explicit drive model stands where a readout would, and produces the same
//! [`BrainSignals`] every other creature's readout does.
//!
//! The senses are a snake's:
//!
//! | stimulus | koi | hognose |
//! |---|---|---|
//! | cursor approaching | a shadow overhead | something large coming close |
//! | click | a knock on the glass | ground vibration, felt through the jaw |
//! | new window | ignored | ignored |
//! | window ledges | ignored | ignored: it burrows, it does not climb |
//!
//! Habituation matters here more than anywhere: a hognose that plays dead
//! every time you reach for the mouse is a snake you stop keeping. The gain
//! is applied to the threat, so the drama fades as the animal learns you.

use dfcore::creature::{Body, World};
use dfcore::data::BrainPointsFile;
use dfcore::env::thermal_tempo;
use dfcore::util::{clamp, hypot};
use dfcore::Region;
use dfcore::{
    circadian_activity, BrainSignals, Creature, EnvSnapshot, Habituation, HognoseBody, Sim, Vec2,
};

use crate::hognoseasset::HognoseAsset;
use crate::hognosebody;
use crate::mesh::Mesh;
use crate::runtime::{Geometry, Runtime};

const NOTICE_RANGE: f32 = 150.0;
const KNOCK_RANGE: f32 = 360.0;

pub struct HognoseRuntime {
    creature: dfcore::creature::Hognose,
    snake: HognoseBody,
    habituation: Habituation,
    frame_mesh: Mesh,
    authored_skin: Option<&'static HognoseAsset>,

    prev_cursor: Option<Vec2>,
    threat: f32,
    threat_ema: f32,
    threat_at: Option<Vec2>,
    /// Hysteresis on the escape pulse: it fires on the way up, once.
    armed: bool,
    forced_startle: bool,

    pending_tempo: f32,
    pending_sleepy: bool,
    activity: f32,
    body_warmth: f32,
    seek_warm: bool,
}

impl HognoseRuntime {
    pub fn new(seed: u64) -> Self {
        let creature = dfcore::creature::Hognose;
        println!(
            "{} - {}",
            creature.display_name(),
            creature.provenance().describe()
        );
        if let dfcore::Provenance::Procedural { why, .. } = creature.provenance() {
            println!("note: {why}");
        }
        HognoseRuntime {
            creature,
            snake: HognoseBody::new(Vec2::ZERO, seed),
            habituation: Habituation::new(),
            frame_mesh: Mesh::default(),
            authored_skin: if std::env::var_os("DESKTOPFLY_PROCEDURAL_HOGNOSE").is_some() {
                None
            } else {
                match HognoseAsset::embedded() {
                    Ok(asset) => Some(asset),
                    Err(error) => {
                        eprintln!("Authored hognose unavailable; using procedural skin: {error}");
                        None
                    }
                }
            },
            prev_cursor: None,
            threat: 0.0,
            threat_ema: 0.0,
            threat_at: None,
            armed: true,
            forced_startle: false,
            pending_tempo: 1.0,
            pending_sleepy: false,
            activity: 1.0,
            body_warmth: 0.5,
            seek_warm: true,
        }
    }

    fn drives(&mut self) -> BrainSignals {
        let mut s = BrainSignals::new();
        s.tempo = self.pending_tempo;
        s.sleep = self.pending_sleepy;
        s.walk_drive = clamp(self.activity, 0.15, 1.0);
        let gain = self.habituation.loom_gain_f32();
        let level = self.threat_ema * gain;
        // The pulse fires once per approach: the body decides what a second
        // one means (a failed bluff), so it must be a real second approach.
        let fire = level > 0.5 && self.armed;
        if fire {
            self.armed = false;
        } else if level < 0.25 {
            self.armed = true;
        }
        s.escape = fire || self.forced_startle;
        self.forced_startle = false;
        s.nervous = clamp(level, 0.0, 1.0);
        s.arousal = s.nervous;
        s
    }
}

impl Runtime for HognoseRuntime {
    fn creature(&self) -> &dyn Creature {
        &self.creature
    }
    fn substrate(&self) -> dfcore::Substrate {
        dfcore::creature::Body::substrate(&self.snake)
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
        "PROCEDURAL - no connectome, hand-written behaviour".to_string()
    }

    fn habituation(&self) -> &Habituation {
        &self.habituation
    }
    fn habituation_mut(&mut self) -> &mut Habituation {
        &mut self.habituation
    }

    fn scare(&mut self) {
        self.forced_startle = true;
    }

    fn sense(&mut self, env: &EnvSnapshot, dt: f32) {
        self.threat = 0.0;
        self.threat_at = None;

        if let Some(m) = env.cursor {
            let speed = match self.prev_cursor {
                Some(p) if dt > 0.0 => hypot(m.x - p.x, m.y - p.y) / dt,
                _ => 0.0,
            };
            self.prev_cursor = Some(m);
            let d = hypot(m.x - self.snake.pos.x, m.y - self.snake.pos.y);
            if d < NOTICE_RANGE {
                let near = 1.0 - d / NOTICE_RANGE;
                let fast = clamp(speed / 600.0, 0.0, 1.0);
                // A snake is less tolerant of a *close* still thing than a
                // fish is: something looming over it is a threat even at
                // rest. But it still has to be near.
                let t = near * (0.35 + 0.65 * fast);
                if t > self.threat {
                    self.threat = t;
                    self.threat_at = Some(m);
                }
            }
        }

        // A click is a footfall: vibration through the ground.
        for c in &env.clicks {
            let d = hypot(c.x - self.snake.pos.x, c.y - self.snake.pos.y);
            let t = clamp(1.0 - d / KNOCK_RANGE, 0.0, 1.0) * 0.85;
            if t > self.threat {
                self.threat = t;
                self.threat_at = Some(*c);
            }
        }

        let rate = if self.threat > self.threat_ema {
            16.0
        } else {
            2.0
        };
        self.threat_ema += (self.threat - self.threat_ema) * (rate * dt).min(1.0);

        self.habituation.step(dt, self.threat, 0.0);

        self.activity = circadian_activity(env.local_hour);
        self.pending_tempo = thermal_tempo(env.machine_heat);
        self.pending_sleepy = (env.idle_secs > 600.0
            && (env.local_hour >= 22.0 || env.local_hour < 6.0))
            || env.idle_secs > 1800.0;
    }

    fn tick(&mut self, dt: f32, region: Region, cursor: Option<Vec2>, attractor: Option<Vec2>) {
        let drives = self.drives();
        let world = World {
            region,
            ledges: Vec::new(),
            cursor: self.threat_at.or(cursor),
            attractor,
        };
        self.snake.step(dt, &drives, &world);
    }

    fn habitat_tick(&mut self, dt: f32, h: &mut dfcore::Habitat, cursor: Option<Vec2>) {
        // Normalised, authored thermal inertia; never infer tank heat from CPU load.
        let heated = h.warm_end.unwrap_or(0);
        let heating = h.warm_end.is_some() || (7.0..21.0).contains(&h.local_hour());
        let hides: Vec<_> = h
            .props
            .iter()
            .enumerate()
            .filter(|(_, p)| p.kind == dfcore::PropKind::Hide)
            .collect();
        let warmth = hides
            .iter()
            .find(|(_, p)| p.variant == heated)
            .map(|(_, p)| {
                (1.0 - self.snake.pos.dist(p.pos) / (h.region.size.0 * 0.7)).clamp(0.15, 0.9)
            })
            .unwrap_or(0.3)
            * if heating { 1.0 } else { 0.45 };
        self.body_warmth += (warmth - self.body_warmth) * (dt / 45.0).min(1.0);
        if self.body_warmth < 0.4 {
            self.seek_warm = true;
        }
        if self.body_warmth > 0.65 {
            self.seek_warm = false;
        }
        let variant = if self.seek_warm { heated } else { 1 - heated };
        let target = hides
            .iter()
            .find(|(_, p)| p.variant == variant)
            .or_else(|| hides.first())
            .map(|(_, p)| p.pos);
        if self.snake.state == dfcore::HognoseState::Slither {
            self.snake.heading +=
                h.obstacle_turn(self.snake.pos, self.snake.heading, 12.0) * dt * 2.0;
        }
        self.pending_tempo = 0.75 + self.body_warmth * 0.5;
        self.tick(dt, h.region, cursor, target);
        if self.snake.buried < 0.5 && self.snake.speed > 1.0 {
            h.track(self.snake.pos);
        }
    }

    fn build(&mut self, glass: bool) -> Geometry<'_> {
        if let Some(asset) = self.authored_skin.filter(|_| !glass) {
            asset.build_frame(&mut self.frame_mesh, &self.snake);
        } else {
            hognosebody::build_frame(&mut self.frame_mesh, &self.snake, glass);
        }
        Geometry {
            inspection_vertices: None,
            body: &self.frame_mesh,
            neurons: None,
        }
    }

    fn head_inspection_radius(&self) -> Option<f32> {
        Some(6.0)
    }

    fn position(&self) -> Vec2 {
        self.snake.pos
    }

    fn place(&mut self, at: Vec2) {
        let seed = (at.x.abs() as u64) ^ ((at.y.abs() as u64) << 8) ^ 0x4E05;
        self.snake = HognoseBody::new(at, seed);
    }

    fn moved_display(&mut self, region: Region) {
        let p = self.snake.pos;
        let clamped = region.clamp_inside(p, 60.0);
        if clamped != p {
            self.place(clamped);
        }
    }

    fn status(&self) -> String {
        format!(
            "{:?} pos ({:.0},{:.0}) speed {:.1} hood {:.2} over {:.2} buried {:.2}",
            self.snake.state,
            self.snake.pos.x,
            self.snake.pos.y,
            self.snake.speed,
            self.snake.hood,
            self.snake.belly_up,
            self.snake.buried
        )
    }

    fn snapshot_pose(&mut self, alt: f32, frames: u32, region: Region) {
        // Slither for a moment, then bluff: the hood is the pose that says
        // "hognose" rather than "rope".
        let drives = BrainSignals::new();
        let world = World {
            region,
            ledges: Vec::new(),
            cursor: None,
            attractor: None,
        };
        self.snake.state = dfcore::HognoseState::Slither;
        self.snake.state_timer = 60.0;
        for _ in 0..frames.max(30) {
            self.snake.step(1.0 / 60.0, &drives, &world);
        }
        if alt < 0.0 {
            return;
        } // Diagnostic calm pose, before the bluff.
        let ahead = Vec2::new(
            self.snake.pos.x + self.snake.heading.cos() * 80.0,
            self.snake.pos.y + self.snake.heading.sin() * 80.0,
        );
        self.snake.threaten(Some(ahead));
        for _ in 0..40 {
            self.snake.step(1.0 / 60.0, &drives, &world);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dfcore::HognoseState;

    fn env_with_cursor(at: Vec2, hour: f32) -> EnvSnapshot {
        EnvSnapshot {
            cursor: Some(at),
            local_hour: hour,
            ..Default::default()
        }
    }

    #[test]
    fn the_hognose_never_claims_to_have_a_brain() {
        let rt = HognoseRuntime::new(1);
        assert!(rt.sim().is_none());
        assert!(rt.brain_points().is_none());
        let info = rt.brain_info();
        assert!(info.to_uppercase().contains("PROCEDURAL"), "{info}");
        assert!(!info.to_lowercase().contains("flywire"), "{info}");
        let d = rt.creature().provenance().describe();
        assert!(d.contains("PROCEDURAL"), "{d}");
        assert!(!d.contains("measured"), "{d}");
    }

    #[test]
    fn the_glass_register_shows_no_neurons() {
        let mut rt = HognoseRuntime::new(2);
        let g = rt.build(true);
        assert!(g.neurons.is_none());
        assert!(!g.body.verts.is_empty());
    }

    /// A cursor lunging at it produces the bluff; a still one far enough away
    /// does not.
    #[test]
    fn a_lunging_cursor_makes_it_bluff_and_a_distant_still_one_does_not() {
        let bounds = Region::centered((1512.0, 982.0));
        let dt = 1.0 / 30.0;

        let mut calm = HognoseRuntime::new(3);
        calm.place(Vec2::ZERO);
        for _ in 0..40 {
            calm.sense(&env_with_cursor(Vec2::new(120.0, 0.0), 12.0), dt);
            calm.tick(dt, bounds, None, None);
        }
        assert!(
            !matches!(
                calm.snake.state,
                HognoseState::Bluff | HognoseState::PlayDead
            ),
            "a cursor resting well away should not alarm it: {:?}",
            calm.snake.state
        );

        let mut spooked = HognoseRuntime::new(3);
        spooked.place(Vec2::ZERO);
        let mut x = 150.0f32;
        let mut bluffed = false;
        for _ in 0..30 {
            spooked.sense(&env_with_cursor(Vec2::new(x, 0.0), 12.0), dt);
            spooked.tick(dt, bounds, None, None);
            bluffed |= spooked.snake.state == HognoseState::Bluff;
            x -= 20.0;
        }
        assert!(bluffed, "a cursor lunging at it should make it bluff");
    }

    /// Keep at it and the bluff fails: it plays dead.
    #[test]
    fn persisting_makes_it_play_dead() {
        let bounds = Region::centered((1512.0, 982.0));
        let dt = 1.0 / 30.0;
        let mut rt = HognoseRuntime::new(5);
        rt.place(Vec2::ZERO);
        let mut dead = false;
        // Two lunges, a pause between so the first can register as a bluff.
        for pass in 0..2 {
            let mut x = 150.0f32;
            for _ in 0..30 {
                rt.sense(&env_with_cursor(Vec2::new(x, 0.0), 12.0), dt);
                rt.tick(dt, bounds, None, None);
                x -= 20.0;
            }
            for _ in 0..30 {
                rt.sense(&env_with_cursor(Vec2::new(600.0, 600.0), 12.0), dt);
                rt.tick(dt, bounds, None, None);
            }
            dead |= rt.snake.state == HognoseState::PlayDead;
            let _ = pass;
        }
        assert!(
            dead,
            "two lunges should exhaust the bluff: {:?}",
            rt.snake.state
        );
    }

    #[test]
    fn a_habituated_hognose_is_harder_to_alarm() {
        let mut rt = HognoseRuntime::new(7);
        rt.place(Vec2::ZERO);
        rt.habituation.loom_gain = 0.35;
        let dt = 1.0 / 30.0;
        let mut x = 150.0f32;
        let mut bluffed = false;
        for _ in 0..30 {
            rt.sense(&env_with_cursor(Vec2::new(x, 0.0), 12.0), dt);
            rt.tick(dt, Region::centered((1512.0, 982.0)), None, None);
            bluffed |= rt.snake.state == HognoseState::Bluff;
            x -= 20.0;
        }
        assert!(
            !bluffed,
            "a habituated snake should tolerate the same approach"
        );
    }

    #[test]
    fn the_tray_escape_test_makes_it_bluff() {
        let mut rt = HognoseRuntime::new(9);
        rt.place(Vec2::ZERO);
        rt.scare();
        rt.tick(1.0 / 60.0, Region::centered((1512.0, 982.0)), None, None);
        assert_eq!(rt.snake.state, HognoseState::Bluff);
    }

    #[test]
    fn it_burrows_at_night_when_the_machine_is_idle() {
        let mut rt = HognoseRuntime::new(11);
        let mut env = EnvSnapshot::default();
        env.local_hour = 2.0;
        env.idle_secs = 1200.0;
        for _ in 0..60 {
            rt.sense(&env, 1.0 / 30.0);
            rt.tick(1.0 / 30.0, Region::centered((1512.0, 982.0)), None, None);
        }
        assert_eq!(rt.snake.state, HognoseState::Burrow);
    }
}
