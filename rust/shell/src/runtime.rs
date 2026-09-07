//! One creature, running.
//!
//! The shell used to construct `LifSim` and `Fly` by name and call the fly's
//! transduction and mesh builder directly, which is why `dfcore`'s `Creature`,
//! `Sim` and `Body` traits existed for a whole phase without a second animal
//! ever reaching the desktop (SPIDER_PLAN.md §6). This is the missing seam:
//! everything the app loop needs from *whichever* creature is loaded, behind
//! one object, so `main.rs` no longer knows what species it is running.
//!
//! What varies per creature and therefore lives behind [`Runtime`]:
//!
//! | piece | fly | worm | koi |
//! |---|---|---|---|
//! | integrator | `LifSim` (spiking) | `GradedSim` (graded + gap junctions) | **none** |
//! | senses → stimulus | looming, taps, wind, proprioception | touch only — it is blind | shadow and knock |
//! | rates → commands | `SignalBuilder` | `GradedSignalBuilder` | a rule model |
//! | body | `Fly` | `Worm` | `Koi` |
//! | geometry | `flybody` | `wormbody` | `koibody` |
//!
//! The koi is the case that proves the seam is real rather than decorative: it
//! has no connectome at all, so `sim()` is `None` for its whole life, and the
//! app loop — brain window, tray, persistence, snapshot — carries on without
//! knowing. Anything that assumed a creature must have a simulation would have
//! broken here.
//!
//! What does *not* vary stays in `main.rs`: the overlay, the frame clock, the
//! tray, the brain window, the sense poll, persistence.

use std::time::Instant;

use dfcore::body::Fly;
use dfcore::data::BrainPointsFile;
use dfcore::{
    Creature, Drosophila, EnvSnapshot, Habituation, LifSim, Region, SignalBuilder, Sim, Vec2,
};

use crate::flybody;
use crate::mesh::Mesh;
use crate::transduction::Transduction;

/// One frame's worth of geometry: the body, and the connectome inside it when
/// the glass register is on.
pub struct Geometry<'a> {
    pub body: &'a Mesh,
    pub neurons: Option<&'a Mesh>,
}

pub trait Runtime {
    fn creature(&self) -> &dyn Creature;
    /// How this animal meets the world. Read off the *body*, which is where
    /// `Substrate` is defined, so there is no second answer to drift from it.
    /// Habitat mode is the first thing to branch on it: a swimmer gets water.
    fn substrate(&self) -> dfcore::Substrate;
    /// Where in an enclosure's vertical space this creature currently is:
    /// 0 on the floor, 1 just under the surface. Only a swimmer has an opinion;
    /// a walker is on the ground and a flier's altitude is already in its own
    /// geometry. Ignored entirely in free roam.
    fn vertical_hint(&self) -> f32 {
        0.0
    }

    fn sim(&self) -> Option<&dyn Sim>;
    fn sim_mut(&mut self) -> Option<&mut dyn Sim>;
    /// Soma cloud for the brain window; `None` when the creature has no data.
    fn brain_points(&self) -> Option<&BrainPointsFile>;
    /// One line for the tray: what data this creature is running on.
    fn brain_info(&self) -> String;

    fn habituation(&self) -> &Habituation;
    fn habituation_mut(&mut self) -> &mut Habituation;

    /// The tray's "Escape Test": a real stimulus into the real circuit.
    fn scare(&mut self);

    /// Drive the circuit from one snapshot of the desktop. Called at the sense
    /// rate (~30 Hz), and must be given the *sense* interval, not the frame
    /// interval — see the note in `main.rs`.
    fn sense(&mut self, env: &EnvSnapshot, dt: f32);

    /// Step the brain at 1 kHz and the body once. Called per frame.
    /// Step the brain at 1 kHz and the body once. `region` is the world's
    /// edge — the display in free roam, the enclosure in habitat mode — and
    /// `attractor` is a prop worth going to look at, if there is one.
    fn tick(
        &mut self,
        dt: f32,
        region: Region,
        cursor: Option<Vec2>,
        attractor: Option<Vec2>,
    );

    /// Build this frame's geometry from the current body state.
    fn build(&mut self, glass: bool) -> Geometry<'_>;

    fn position(&self) -> Vec2;
    /// Put the creature somewhere — at startup, or after a creature switch so
    /// the new animal appears where the old one was.
    fn place(&mut self, at: Vec2);
    /// The display changed under the creature: terrain is stale and it must
    /// land inside the new bounds.
    fn moved_display(&mut self, region: Region);

    /// For the 10 s console report.
    fn status(&self) -> String;

    /// Put the creature into a representative pose for `--snapshot`, with the
    /// circuit visibly active. `alt` lifts a flier; a crawler ignores it.
    fn snapshot_pose(&mut self, alt: f32, walking_frames: u32, region: Region);
}

/// Construct the runtime for a creature id. Data is loaded here; a creature
/// whose data is missing still runs, brainless, and says so.
pub fn make(id: &str, seed: u64) -> Box<dyn Runtime> {
    match id {
        "c_elegans" => Box::new(crate::wormrt::WormRuntime::new(seed)),
        "salticid" => Box::new(crate::spiderrt::SpiderRuntime::new(seed)),
        "koi" => Box::new(crate::koirt::KoiRuntime::new(seed)),
        "araneus" => Box::new(crate::weaverrt::WeaverRuntime::new(dfcore::Weaver::Araneus, seed)),
        "parasteatoda" => {
            Box::new(crate::weaverrt::WeaverRuntime::new(dfcore::Weaver::Parasteatoda, seed))
        }
        "agelenopsis" => {
            Box::new(crate::weaverrt::WeaverRuntime::new(dfcore::Weaver::Agelenopsis, seed))
        }
        _ => Box::new(FlyRuntime::new(seed)),
    }
}

/// Creature #1, exactly as the shell ran it before this seam existed. The
/// code in `sense`, `tick` and `build` is moved, not rewritten: the
/// `--snapshot` diff against the pre-seam build is the check on that.
pub struct FlyRuntime {
    creature: Drosophila,
    sim: Option<LifSim>,
    brain_points: Option<BrainPointsFile>,
    signals: SignalBuilder,
    fly: Fly,
    trans: Transduction,

    meshes: flybody::FlyMeshes,
    frame_mesh: Mesh,
    neuron_mesh: Mesh,
    /// Per-neuron flash brightness for the glass body, decayed each frame.
    body_flash: Vec<f32>,

    ms_accumulator: f64,
    // Carried between the 30 Hz sense poll and the per-frame update.
    pending_tempo: f32,
    pending_sleepy: bool,
    /// Whether the last `build` had neurons to draw.
    has_neurons: bool,
    _born: Instant,
}

impl FlyRuntime {
    pub fn new(seed: u64) -> Self {
        let mut rt = FlyRuntime {
            creature: Drosophila,
            sim: None,
            brain_points: None,
            signals: SignalBuilder::new(),
            fly: Fly::new(Vec2::ZERO, seed),
            trans: Transduction::new(),
            meshes: flybody::FlyMeshes::build(),
            frame_mesh: Mesh::default(),
            neuron_mesh: Mesh::default(),
            body_flash: Vec::new(),
            ms_accumulator: 0.0,
            pending_tempo: 1.0,
            pending_sleepy: false,
            has_neurons: false,
            _born: Instant::now(),
        };
        match dfcore::data::load_for(rt.creature.data_dir()) {
            Ok(brain) => {
                let mut sim = LifSim::new(&brain.circuit, seed);
                // The brain window flashes spikes where they actually happen.
                sim.collect_spikes = true;
                println!(
                    "FlyWire v783 - {} somas - circuit {}n/{}e",
                    brain.points.points.len(),
                    brain.circuit.neurons.len(),
                    brain.circuit.edges.len()
                );
                rt.body_flash = vec![0.0; sim.n];
                rt.sim = Some(sim);
                rt.brain_points = Some(brain.points);
            }
            Err(e) => eprintln!("no brain data ({e}) - falling back to brainless behaviour"),
        }
        rt
    }

    /// Light up whatever just spiked, on top of last frame's decayed glow.
    fn flash_spikes(&mut self, decay: f32) {
        let Some(sim) = self.sim.as_ref() else { return };
        for f in self.body_flash.iter_mut() {
            *f *= decay;
        }
        for ev in &sim.last_spikes {
            if ev.neuron < self.body_flash.len() {
                self.body_flash[ev.neuron] = if ev.is_gf { 2.5 } else { 1.0 };
            }
        }
    }
}

impl Runtime for FlyRuntime {
    fn creature(&self) -> &dyn Creature {
        &self.creature
    }
    fn substrate(&self) -> dfcore::Substrate {
        dfcore::creature::Body::substrate(&self.fly)
    }
    fn sim(&self) -> Option<&dyn Sim> {
        self.sim.as_ref().map(|s| s as &dyn Sim)
    }
    fn sim_mut(&mut self) -> Option<&mut dyn Sim> {
        self.sim.as_mut().map(|s| s as &mut dyn Sim)
    }
    fn brain_points(&self) -> Option<&BrainPointsFile> {
        self.brain_points.as_ref()
    }
    fn brain_info(&self) -> String {
        match &self.sim {
            Some(sim) => format!("FlyWire v783 - circuit {}n", sim.n),
            None => "no data - run etl.py".to_string(),
        }
    }
    fn habituation(&self) -> &Habituation {
        &self.trans.habituation
    }
    fn habituation_mut(&mut self) -> &mut Habituation {
        &mut self.trans.habituation
    }
    fn scare(&mut self) {
        self.trans.trigger_scare();
    }

    fn sense(&mut self, env: &EnvSnapshot, dt: f32) {
        self.fly.terrain = env.ledges.clone();
        if let Some(sim) = self.sim.as_mut() {
            let (tempo, sleepy) = self.trans.apply(sim, &self.fly, env, dt);
            self.pending_tempo = tempo;
            self.pending_sleepy = sleepy;
        }
    }

    fn tick(&mut self, dt: f32, region: Region, cursor: Option<Vec2>, attractor: Option<Vec2>) {
        // Step the brain at a true 1 kHz, decoupled from the frame rate.
        let mut signals = None;
        if let Some(sim) = self.sim.as_mut() {
            self.ms_accumulator += dt as f64 * 1000.0;
            let steps = (self.ms_accumulator as i64).min(50);
            self.ms_accumulator -= steps as f64;
            sim.step(steps);
            let mut s = self.signals.make(sim, dt);
            s.tempo = self.pending_tempo;
            s.sleep = self.pending_sleepy;
            signals = Some(s);
        }
        self.fly.attractor = attractor;
        self.fly.update(dt, region, cursor, signals);
        // The connectome inside the glass shell: decay, then light up whatever
        // just spiked. Same data the brain window draws, on the creature itself.
        self.flash_spikes((-dt * 7.0).exp());
    }

    fn build(&mut self, glass: bool) -> Geometry<'_> {
        let pose = self.fly.pose();
        flybody::build_frame(&mut self.frame_mesh, &self.meshes, &self.fly, &pose, glass);
        self.has_neurons = false;
        if glass {
            if let Some(sim) = self.sim.as_ref() {
                flybody::build_neuron_field(
                    &mut self.neuron_mesh,
                    sim,
                    &self.body_flash,
                    &self.fly,
                    &pose,
                );
                self.has_neurons = true;
            }
        }
        Geometry {
            body: &self.frame_mesh,
            neurons: if self.has_neurons {
                Some(&self.neuron_mesh)
            } else {
                None
            },
        }
    }

    fn position(&self) -> Vec2 {
        self.fly.pos
    }
    fn place(&mut self, at: Vec2) {
        self.fly.pos = at;
    }
    fn moved_display(&mut self, region: Region) {
        // Terrain is stale until the next poll, and the fly must land inside
        // the new world — which is also how it gets into a habitat that has
        // just been switched on around it.
        self.fly.terrain.clear();
        self.fly.ledge = None;
        self.fly.pos = region.clamp_inside(self.fly.pos, 40.0);
    }
    fn status(&self) -> String {
        format!(
            "state {:?}  pos ({:.0},{:.0})  ledges {}",
            self.fly.state,
            self.fly.pos.x,
            self.fly.pos.y,
            self.fly.terrain.len()
        )
    }

    fn snapshot_pose(&mut self, alt: f32, walking_frames: u32, region: Region) {
        self.fly.state = dfcore::State::Walking;
        self.fly.speed = 60.0;
        self.fly.heading = 0.4;
        for _ in 0..walking_frames {
            self.fly.update(1.0 / 60.0, region, None, None);
        }
        self.fly.pos = Vec2::ZERO;
        self.fly.alt = alt;
        // Run the real circuit so the glass body shows real activity rather
        // than a decorative sparkle. Without this the diagnostic would be a lie
        // about the one thing the rendering is claiming.
        if let Some(sim) = self.sim.as_mut() {
            sim.step(1500);
            // A cursor lunge, so the looming population is visibly hot.
            sim.loom_l = 1.0;
            sim.loom_r = 0.6;
            sim.step(60);
            let spiked = sim.last_spikes.len();
            self.flash_spikes(0.0);
            // A few frames of decay so it looks like a moment, not a freeze.
            for f in self.body_flash.iter_mut() {
                *f *= 0.9;
            }
            println!(
                "snapshot: {} neurons lit ({} spiked this step)",
                self.body_flash.iter().filter(|f| **f >= 0.012).count(),
                spiked
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dfcore::CREATURE_IDS;

    /// The tray picker's contract, end to end: every listed creature builds a
    /// runtime, that runtime reports the id it was asked for, and it produces
    /// geometry. This is the code path `SelectCreature` takes, so a creature
    /// that is listed but cannot be switched to would fail here rather than in
    /// front of the user.
    #[test]
    fn every_listed_creature_builds_a_runtime_that_renders() {
        for id in CREATURE_IDS {
            let mut rt = make(id, 7);
            assert_eq!(
                rt.creature().id(),
                id,
                "make({id}) built a different creature"
            );

            // Place it, run a second, and build a frame — the same sequence
            // `switch_creature` performs.
            rt.place(dfcore::Vec2::new(40.0, -20.0));
            for _ in 0..60 {
                rt.tick(1.0 / 60.0, Region::centered((1512.0, 982.0)), None, None);
            }
            let g = rt.build(false);
            assert!(
                !g.body.verts.is_empty() && !g.body.indices.is_empty(),
                "{id} rendered nothing"
            );
            assert!(
                g.body.indices.iter().all(|&i| (i as usize) < g.body.verts.len()),
                "{id} produced out-of-range indices"
            );

            // And the tray line must never be empty, whatever the data status.
            assert!(!rt.brain_info().is_empty(), "{id} has no data line");
            assert!(!rt.status().is_empty(), "{id} has no status line");
        }
    }

    /// Switching must not hand one creature another's data line — that is how
    /// a procedural creature would end up claiming a connectome.
    #[test]
    fn each_creature_reports_its_own_provenance() {
        let fly = make("drosophila", 1);
        let koi = make("koi", 1);
        assert_ne!(fly.brain_info(), koi.brain_info());
        assert!(
            koi.brain_info().to_uppercase().contains("PROCEDURAL"),
            "koi data line: {}",
            koi.brain_info()
        );
        assert!(
            !koi.brain_info().to_lowercase().contains("flywire"),
            "the koi must not borrow the fly's dataset"
        );
        assert!(koi.sim().is_none(), "the koi has no simulation");
    }

    /// An unknown id degrades to the fly rather than panicking, so a stale
    /// settings file cannot brick the app.
    #[test]
    fn an_unknown_creature_id_falls_back_to_the_fly() {
        assert_eq!(make("nonsense", 1).creature().id(), "drosophila");
    }

    /// The point of the whole feature: an enclosure has to actually hold the
    /// animal. Run each creature for a simulated minute inside a small tank
    /// parked *off* the scene origin — which is the case the old `±bounds/2`
    /// arithmetic could not express — and it must never get out.
    ///
    /// The tank is deliberately off-centre and deliberately small: a creature
    /// that merely drifts toward (0, 0) would pass a centred test by accident.
    #[test]
    fn every_creature_stays_inside_its_enclosure() {
        let region = dfcore::Region::new(dfcore::Vec2::new(420.0, -260.0), (520.0, 360.0));
        for id in CREATURE_IDS {
            let mut rt = make(id, 4);
            rt.place(region.center);
            let mut worst = 0.0f32;
            for i in 0..3600 {
                // A cursor sweeping across the tank, so escapes and darts fire
                // rather than leaving the creature idling in the middle.
                let cursor = if i % 400 < 60 {
                    Some(region.center)
                } else {
                    None
                };
                rt.tick(1.0 / 60.0, region, cursor, None);
                let p = rt.position();
                worst = worst
                    .max((p.x - region.center.x).abs() - region.size.0 / 2.0)
                    .max((p.y - region.center.y).abs() - region.size.1 / 2.0);
            }
            assert!(
                worst <= 0.0,
                "{id} escaped its enclosure by {worst:.1} units"
            );
        }
    }

    /// Free roam has to keep working: the same creature, given the whole
    /// display, must still use most of it rather than being penned in by the
    /// refactor.
    #[test]
    fn free_roam_still_roams() {
        let region = dfcore::Region::centered((1512.0, 982.0));
        let mut rt = make("drosophila", 4);
        rt.place(dfcore::Vec2::ZERO);
        let mut reach = 0.0f32;
        for _ in 0..7200 {
            rt.tick(1.0 / 60.0, region, None, None);
            let p = rt.position();
            reach = reach.max(p.x.abs().max(p.y.abs()));
        }
        assert!(
            reach > 250.0,
            "the fly stayed within {reach:.0} units of the origin - free roam is penned in"
        );
    }

    /// A creature standing in its own gravel, or clipping out through the rim,
    /// is the obvious way for this to look broken — and the clearance is not
    /// obvious by inspection, because a body's *nominal* plane is z = 0 but its
    /// geometry is not: the fly's legs and wings reach several units below it.
    ///
    /// So this measures each creature's true extent, applies the same lift the
    /// frame composition applies, and asserts the result is inside the tank.
    #[test]
    fn every_creature_fits_between_the_floor_and_the_rim_of_its_tank() {
        use dfcore::HabitatKind;
        let floor = crate::habitatmesh::FLOOR_Z;
        for id in CREATURE_IDS {
            let mut rt = make(id, 4);
            for _ in 0..60 {
                rt.tick(
                    1.0 / 60.0,
                    dfcore::Region::centered((1512.0, 982.0)),
                    None,
                    None,
                );
            }
            let kind = HabitatKind::for_substrate(rt.substrate());
            let rim = crate::habitatmesh::top_z(kind);
            // The worm stands on the agar, not the bottom of the dish, and must
            // clear *that*.
            let floor = if kind == HabitatKind::AgarPlate {
                floor + crate::habitatmesh::AGAR
            } else {
                floor
            };
            // Both extremes of the water column for a swimmer; a walker's hint
            // is a constant.
            for hint in [0.0f32, 1.0] {
                let lift = crate::habitatmesh::creature_lift(kind, hint);
                let g = rt.build(false);
                let (lo, hi) = g.body.verts.iter().map(|v| v.pos[2] + lift).fold(
                    (f32::MAX, f32::MIN),
                    |(a, b), z| (a.min(z), b.max(z)),
                );
                assert!(
                    lo >= floor,
                    "{id} at hint {hint} sinks to {lo:.1}, below the tank floor {floor:.1}"
                );
                assert!(
                    hi <= rim,
                    "{id} at hint {hint} reaches {hi:.1}, above the tank rim {rim:.1}"
                );
            }
        }
    }

    /// A swimmer's depth has to actually change its height, or the tilt bought
    /// nothing: looking straight down, the koi could only express depth as
    /// apparent size, and that is the fake this replaces.
    #[test]
    fn a_swimmers_depth_becomes_real_height_in_the_tank() {
        use dfcore::HabitatKind;
        let deep = crate::habitatmesh::creature_lift(HabitatKind::Pond, 0.0);
        let shallow = crate::habitatmesh::creature_lift(HabitatKind::Pond, 1.0);
        assert!(
            shallow > deep + 20.0,
            "surfacing only lifts the fish {:.1} units",
            shallow - deep
        );
        // A walker has no such freedom; it is on the ground either way.
        for kind in [HabitatKind::FlyCage, HabitatKind::Vivarium, HabitatKind::AgarPlate] {
            assert_eq!(
                crate::habitatmesh::creature_lift(kind, 0.0),
                crate::habitatmesh::creature_lift(kind, 1.0)
            );
        }
        // And only the koi ever asks to be lifted.
        for id in CREATURE_IDS {
            let hint = make(id, 1).vertical_hint();
            assert!((0.0..=1.0).contains(&hint), "{id} hint {hint} out of range");
            if id != "koi" {
                assert_eq!(hint, 0.0, "{id} claims to float");
            }
        }
    }

    /// Each animal gets the container it is actually kept in — through the
    /// runtime, not through a second table that could disagree with the
    /// bodies.
    #[test]
    fn each_creature_asks_for_the_right_enclosure() {
        use dfcore::HabitatKind;
        for id in CREATURE_IDS {
            let want = match id {
                "koi" => HabitatKind::Pond,
                "c_elegans" => HabitatKind::AgarPlate,
                "salticid" | "araneus" | "parasteatoda" | "agelenopsis" => HabitatKind::Vivarium,
                _ => HabitatKind::FlyCage,
            };
            let got = HabitatKind::for_substrate(make(id, 1).substrate());
            assert_eq!(got, want, "{id} was given a {}", got.label());
        }
    }

    /// The creature has to notice a prop, not merely be allowed to bump into
    /// one. Given a fixed attractor off to one side, it should end up closer to
    /// it than an otherwise identical creature that was never told about it.
    #[test]
    fn an_attractor_pulls_the_creature_toward_it() {
        let region = dfcore::Region::centered((900.0, 700.0));
        let target = dfcore::Vec2::new(300.0, 220.0);
        let mut nearest = (f32::MAX, f32::MAX);
        for (k, curious) in [(0, false), (1, true)] {
            let _ = k;
            let mut rt = make("drosophila", 12);
            rt.place(dfcore::Vec2::new(-300.0, -220.0));
            let mut best = f32::MAX;
            for _ in 0..5400 {
                rt.tick(
                    1.0 / 60.0,
                    region,
                    None,
                    if curious { Some(target) } else { None },
                );
                let p = rt.position();
                best = best.min(((p.x - target.x).powi(2) + (p.y - target.y).powi(2)).sqrt());
            }
            if curious {
                nearest.1 = best;
            } else {
                nearest.0 = best;
            }
        }
        assert!(
            nearest.1 < nearest.0,
            "curiosity made no difference: closest approach {:.0} with an attractor vs {:.0} without",
            nearest.1,
            nearest.0
        );
    }
}
