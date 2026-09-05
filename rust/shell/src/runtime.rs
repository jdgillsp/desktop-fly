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
//! | piece | fly | worm |
//! |---|---|---|
//! | integrator | `LifSim` (spiking) | `GradedSim` (graded + gap junctions) |
//! | senses → stimulus | looming, taps, wind, proprioception | touch only — it is blind |
//! | rates → commands | `SignalBuilder` | `GradedSignalBuilder` |
//! | body | `Fly` | `Worm` |
//! | geometry | `flybody` | `wormbody` |
//!
//! What does *not* vary stays in `main.rs`: the overlay, the frame clock, the
//! tray, the brain window, the sense poll, persistence.

use std::time::Instant;

use dfcore::body::Fly;
use dfcore::data::BrainPointsFile;
use dfcore::{
    Creature, Drosophila, EnvSnapshot, Habituation, LifSim, SignalBuilder, Sim, Vec2,
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
    fn tick(&mut self, dt: f32, bounds: (f32, f32), cursor: Option<Vec2>);

    /// Build this frame's geometry from the current body state.
    fn build(&mut self, glass: bool) -> Geometry<'_>;

    fn position(&self) -> Vec2;
    /// Put the creature somewhere — at startup, or after a creature switch so
    /// the new animal appears where the old one was.
    fn place(&mut self, at: Vec2);
    /// The display changed under the creature: terrain is stale and it must
    /// land inside the new bounds.
    fn moved_display(&mut self, bounds: (f32, f32));

    /// For the 10 s console report.
    fn status(&self) -> String;

    /// Put the creature into a representative pose for `--snapshot`, with the
    /// circuit visibly active. `alt` lifts a flier; a crawler ignores it.
    fn snapshot_pose(&mut self, alt: f32, walking_frames: u32, bounds: (f32, f32));
}

/// Construct the runtime for a creature id. Data is loaded here; a creature
/// whose data is missing still runs, brainless, and says so.
pub fn make(id: &str, seed: u64) -> Box<dyn Runtime> {
    match id {
        "c_elegans" => Box::new(crate::wormrt::WormRuntime::new(seed)),
        "salticid" => Box::new(crate::spiderrt::SpiderRuntime::new(seed)),
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

    fn tick(&mut self, dt: f32, bounds: (f32, f32), cursor: Option<Vec2>) {
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
        self.fly.update(dt, bounds, cursor, signals);
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
    fn moved_display(&mut self, bounds: (f32, f32)) {
        // Terrain is stale until the next poll, and the fly must land inside
        // the new display.
        self.fly.terrain.clear();
        self.fly.ledge = None;
        let (w, h) = bounds;
        self.fly.pos.x = self.fly.pos.x.clamp(-w / 2.0 + 40.0, w / 2.0 - 40.0);
        self.fly.pos.y = self.fly.pos.y.clamp(-h / 2.0 + 40.0, h / 2.0 - 40.0);
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

    fn snapshot_pose(&mut self, alt: f32, walking_frames: u32, bounds: (f32, f32)) {
        self.fly.state = dfcore::State::Walking;
        self.fly.speed = 60.0;
        self.fly.heading = 0.4;
        for _ in 0..walking_frames {
            self.fly.update(1.0 / 60.0, bounds, None, None);
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
