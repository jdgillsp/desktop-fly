//! Creature #2 on the desktop: *C. elegans*.
//!
//! The runtime the `Runtime` seam was built for. It differs from the fly's in
//! every row of the table in `runtime.rs`: a graded integrator, a body that
//! crawls, and — the part that is easy to get wrong — **senses that are not
//! the fly's**. A worm is blind. The cursor cannot loom at it, a new window is
//! not a predator, and driving its touch neurons from the fly's looming
//! transduction would be inventing a sense the animal does not have. What it
//! *does* have is mechanosensation: the anterior touch cells (ALM/AVM) drive
//! reversal, the posterior ones (PLM/PVM) drive acceleration, and habituation
//! of exactly that pathway is the canonical behavioural assay in the species.
//!
//! **Data is not shipped** (`CElegans::provenance()` says why). Without it the
//! worm runs brainless: a constant forward drive, and no reactions. The tray
//! says so, and the brain window stays closed.

use dfcore::creature::{Body, World};
use dfcore::data::BrainPointsFile;
use dfcore::env::thermal_tempo;
use dfcore::util::hypot;
use dfcore::{
    circadian_activity, BrainSignals, CElegans, Connectome, Creature, DynamicsSpec,
    EnvSnapshot, GradedSignalBuilder, GradedSim, Habituation, Sim, Vec2, Worm,
};

use crate::mesh::Mesh;
use crate::runtime::{Geometry, Runtime};
use crate::wormbody;

/// How close, in scene units, a cursor or click must come to the body to be
/// felt. The animal is touched, not seen.
const TOUCH_RANGE: f32 = 14.0;
const CLICK_RANGE: f32 = 90.0;

pub struct WormRuntime {
    creature: CElegans,
    sim: Option<GradedSim>,
    brain_points: Option<BrainPointsFile>,
    signals: GradedSignalBuilder,
    worm: Worm,
    habituation: Habituation,

    frame_mesh: Mesh,
    neuron_mesh: Mesh,
    activity: Vec<f32>,
    has_neurons: bool,

    ms_accumulator: f64,
    prev_cursor: Option<Vec2>,
    pending_tempo: f32,
    pending_sleepy: bool,
    /// "Escape Test" for a blind animal: a strong anterior touch.
    touch_override: f32,
}

impl WormRuntime {
    pub fn new(seed: u64) -> Self {
        let creature = CElegans;
        let mut rt = WormRuntime {
            sim: None,
            brain_points: None,
            signals: GradedSignalBuilder::new(),
            worm: Worm::new(Vec2::ZERO, seed),
            habituation: Habituation::new(),
            frame_mesh: Mesh::default(),
            neuron_mesh: Mesh::default(),
            activity: Vec::new(),
            has_neurons: false,
            ms_accumulator: 0.0,
            prev_cursor: None,
            pending_tempo: 1.0,
            pending_sleepy: false,
            touch_override: 0.0,
            creature,
        };
        match dfcore::data::load_for(rt.creature.data_dir()) {
            Ok(brain) => {
                let connectome = Connectome::from_circuit(&brain.circuit, rt.creature.provenance());
                let DynamicsSpec::Graded(params) = rt.creature.dynamics() else {
                    unreachable!("the worm is a graded creature");
                };
                let sim = GradedSim::new(&connectome, rt.creature.manifest(), params, seed);
                println!(
                    "{} - {} neurons, {} chemical / {} electrical edges",
                    rt.creature.provenance().describe(),
                    sim.n,
                    connectome.chemical.len(),
                    connectome.electrical.len()
                );
                rt.activity = vec![0.0; sim.n];
                rt.sim = Some(sim);
                rt.brain_points = Some(brain.points);
            }
            Err(e) => eprintln!("no worm data ({e}) - the worm runs brainless"),
        }
        rt
    }

    /// Where along the body a point is: 0 = head, 1 = tail, or `None` if it is
    /// not within `range` of any segment.
    fn body_contact(&self, p: Vec2, range: f32) -> Option<f32> {
        let n = self.worm.body.len();
        let mut best: Option<(f32, f32)> = None;
        for (i, b) in self.worm.body.iter().enumerate() {
            let d = hypot(p.x - b.x, p.y - b.y);
            if d <= range && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                best = Some((d, i as f32 / (n.max(2) - 1) as f32));
            }
        }
        best.map(|(_, t)| t)
    }

    /// A touch at body fraction `t` reaches the anterior or posterior
    /// mechanosensory cells, gated by habituation of that same pathway.
    fn touch(&mut self, t: f32, strength: f32) {
        let gain = self.habituation.tap_gain_f32();
        let Some(sim) = self.sim.as_mut() else { return };
        let slug = if t < 0.5 { "touch" } else { "touch_post" };
        let cells = sim.group(slug).to_vec();
        if !cells.is_empty() {
            sim.stimulate(&cells, strength * gain * 3.0, 120);
        }
    }
}

impl Runtime for WormRuntime {
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
            Some(sim) => format!("{} - {}n", self.creature.provenance().describe(), sim.n),
            None => "no data - see etl_celegans.py (brainless)".to_string(),
        }
    }
    fn habituation(&self) -> &Habituation {
        &self.habituation
    }
    fn habituation_mut(&mut self) -> &mut Habituation {
        &mut self.habituation
    }
    fn scare(&mut self) {
        self.touch_override = 1.0;
    }

    fn sense(&mut self, env: &EnvSnapshot, dt: f32) {
        // Clicks near the body are substrate taps, felt strongest at the
        // nearer end. The tap-withdrawal assay, on a desktop.
        let mut tap_drive: f32 = 0.0;
        for c in &env.clicks {
            let head = self.worm.pos;
            let d = hypot(c.x - head.x, c.y - head.y);
            if d < CLICK_RANGE * 3.0 {
                let t = self.body_contact(*c, CLICK_RANGE).unwrap_or(0.0);
                let strength = (1.0 - d / (CLICK_RANGE * 3.0)).clamp(0.0, 1.0);
                tap_drive = tap_drive.max(strength);
                self.touch(t, 0.3 + strength * 0.5);
            }
        }
        // A cursor moving across the body is a touch; a still one resting on
        // it is not (the animal adapts to sustained contact within seconds).
        if let Some(m) = env.cursor {
            let speed = match self.prev_cursor {
                Some(p) if dt > 0.0 => hypot(m.x - p.x, m.y - p.y) / dt,
                _ => 0.0,
            };
            self.prev_cursor = Some(m);
            if speed > 40.0 {
                if let Some(t) = self.body_contact(m, TOUCH_RANGE) {
                    let strength = (speed / 900.0).clamp(0.15, 1.0);
                    tap_drive = tap_drive.max(strength);
                    self.touch(t, strength * 0.6);
                }
            }
        } else {
            self.prev_cursor = None;
        }
        if self.touch_override > 0.0 {
            let s = self.touch_override;
            self.touch(0.0, s);
            self.touch_override = 0.0;
        }
        // Habituation is presynaptic gain on the stimulus, advanced on what
        // the world actually presented. There is no looming pathway here, so
        // that channel stays at zero and only the tap channel learns.
        self.habituation.step(dt, 0.0, tap_drive);

        // Circadian and quiescence neuromodulation: the same compression toward
        // 1 as the fly (CLAUDE.md's "siesta coma" lesson applies to any
        // integrator with a thin operating point).
        let activity = circadian_activity(env.local_hour);
        let sleepy = (env.idle_secs > 600.0 && (env.local_hour >= 22.0 || env.local_hour < 6.0))
            || env.idle_secs > 1800.0;
        if let Some(sim) = self.sim.as_mut() {
            sim.activity_scale = (1.0 - (1.0 - activity) * 0.35) * if sleepy { 0.75 } else { 1.0 };
            sim.sensory_gate = if sleepy { 0.55 } else { 1.0 };
        }
        self.pending_tempo = thermal_tempo(env.machine_heat);
        self.pending_sleepy = sleepy;
    }

    fn tick(&mut self, dt: f32, bounds: (f32, f32), cursor: Option<Vec2>) {
        let mut drives = match self.sim.as_mut() {
            Some(sim) => {
                self.ms_accumulator += dt as f64 * 1000.0;
                let steps = (self.ms_accumulator as i64).min(50);
                self.ms_accumulator -= steps as f64;
                sim.step(steps);
                self.signals.make(sim)
            }
            None => {
                // Brainless: a steady crawl, so the seam can be exercised
                // without the data. Labelled as such in the tray.
                let mut s = BrainSignals::new();
                s.walk_drive = 0.5;
                s
            }
        };
        drives.tempo = self.pending_tempo;
        drives.sleep = self.pending_sleepy;
        let world = World {
            bounds,
            ledges: Vec::new(),
            cursor,
        };
        self.worm.step(dt, &drives, &world);
        if let Some(sim) = self.sim.as_ref() {
            self.activity = sim.activity();
        }
    }

    fn build(&mut self, glass: bool) -> Geometry<'_> {
        wormbody::build_frame(&mut self.frame_mesh, &self.worm, glass);
        self.has_neurons = false;
        if glass {
            if let Some(sim) = self.sim.as_ref() {
                wormbody::build_neuron_field(&mut self.neuron_mesh, sim, &self.activity, &self.worm);
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
        self.worm.pos
    }
    fn place(&mut self, at: Vec2) {
        // The body is a recorded path, so the worm is re-created at the new
        // spot rather than dragged there with its trail behind it.
        let heading = self.worm.heading;
        self.worm = Worm::new(at, dfcore::DEFAULT_SEED);
        self.worm.heading = heading;
    }
    fn moved_display(&mut self, bounds: (f32, f32)) {
        let (w, h) = bounds;
        let p = self.worm.pos;
        let inside = Vec2::new(
            p.x.clamp(-w / 2.0 + 60.0, w / 2.0 - 60.0),
            p.y.clamp(-h / 2.0 + 60.0, h / 2.0 - 60.0),
        );
        if inside.x != p.x || inside.y != p.y {
            self.place(inside);
        }
    }
    fn status(&self) -> String {
        format!(
            "state {:?}  pos ({:.0},{:.0})  drive {:.2}",
            self.worm.state, self.worm.pos.x, self.worm.pos.y, self.worm.drive
        )
    }

    fn snapshot_pose(&mut self, _alt: f32, walking_frames: u32, bounds: (f32, f32)) {
        let mut d = BrainSignals::new();
        d.walk_drive = 0.8;
        let world = World {
            bounds,
            ledges: Vec::new(),
            cursor: None,
        };
        for _ in 0..walking_frames.max(90) {
            self.worm.step(1.0 / 60.0, &d, &world);
        }
        self.place(Vec2::ZERO);
        for _ in 0..90 {
            self.worm.step(1.0 / 60.0, &d, &world);
        }
        // Re-centre on the body's midpoint so the whole animal is in frame.
        let n = self.worm.body.len();
        if n > 0 {
            let mid = self.worm.body[n / 2];
            for p in self.worm.body.iter_mut() {
                p.x -= mid.x;
                p.y -= mid.y;
            }
        }
        if let Some(sim) = self.sim.as_mut() {
            sim.step(1500);
            let cells = sim.group("touch").to_vec();
            sim.stimulate(&cells, 3.0, 200);
            sim.step(120);
            self.activity = sim.activity();
        }
    }
}
