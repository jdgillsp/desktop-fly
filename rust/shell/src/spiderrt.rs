//! Creature #3 on the desktop: the jumping-spider chimera (SPIDER_PLAN.md).
//!
//! What it shares with the fly it takes from the fly: the looming, tap, wind,
//! proprioception and circadian transduction in `transduction.rs` runs
//! unchanged, because the modules it drives are the fly's measured modules.
//! What is new is one channel and a few senses:
//!
//! - **Small objects → LC11.** Bugs, and the cursor when it is moving but not
//!   looming, become per-eye small-object drive. The size selectivity is
//!   modelled here (the lobula is outside the extract); what the population
//!   and the authored pounce node do with it is the circuit's business.
//! - **The coding senses** (§5): the foreground class settles the spider while
//!   you work, and a reported build failure spawns bugs. Nothing else. Bugs
//!   do not habituate — prey is not a threat — but a startle still does.

use dfcore::Region;
use dfcore::data::BrainPointsFile;
use dfcore::env::{BuildEvent, Foreground};
use dfcore::util::hypot;
use dfcore::{
    Creature, EnvSnapshot, Habituation, LifParams, LifSim, Origin, Salticid, SignalBuilder, Sim,
    Spider, Vec2,
};

use crate::mesh::Mesh;
use crate::runtime::{Geometry, Runtime};
use crate::spiderbody::{self, SpiderMeshes};
use crate::transduction::Transduction;

/// Closer than this, a moving cursor is a looming object (the fly's
/// transduction owns it); farther, it is a small one.
const CURSOR_SMALL_MIN_DIST: f32 = 140.0;
/// A cursor slower than this is a still speck; faster, it is a fleeing gnat,
/// which is exactly what a salticid tracks.
const CURSOR_SMALL_MIN_SPEED: f32 = 25.0;
/// Bugs per reported failure.
const BUGS_PER_FAIL: usize = 3;

pub struct SpiderRuntime {
    creature: Salticid,
    sim: Option<LifSim>,
    brain_points: Option<BrainPointsFile>,
    signals: SignalBuilder,
    spider: Spider,
    trans: Transduction,

    meshes: SpiderMeshes,
    frame_mesh: Mesh,
    neuron_mesh: Mesh,
    body_flash: Vec<f32>,
    has_neurons: bool,

    ms_accumulator: f64,
    pending_tempo: f32,
    pending_sleepy: bool,
    prev_cursor: Option<Vec2>,
    cursor_vel: Vec2,
    foreground: Foreground,
    fails_seen: u32,
    rng_phase: f32,
    /// The display the body last stepped in, for placing bugs on it.
    region: Region,
}

impl SpiderRuntime {
    pub fn new(seed: u64) -> Self {
        let creature = Salticid;
        let mut rt = SpiderRuntime {
            sim: None,
            brain_points: None,
            signals: SignalBuilder::new(),
            spider: Spider::new(Vec2::ZERO, seed),
            trans: Transduction::new(),
            meshes: SpiderMeshes::build(),
            frame_mesh: Mesh::default(),
            neuron_mesh: Mesh::default(),
            body_flash: Vec::new(),
            has_neurons: false,
            ms_accumulator: 0.0,
            pending_tempo: 1.0,
            pending_sleepy: false,
            prev_cursor: None,
            cursor_vel: Vec2::ZERO,
            foreground: Foreground::Unknown,
            fails_seen: 0,
            rng_phase: 0.37,
            region: Region::centered((1920.0, 1080.0)),
            creature,
        };
        match dfcore::data::load_for(rt.creature.data_dir()) {
            Ok(brain) => {
                let mut sim =
                    LifSim::with_params(&brain.circuit, seed, LifParams::default(), rt.creature.manifest());
                sim.collect_spikes = true;
                let authored = (0..sim.n).filter(|&i| sim.origin(i) == Origin::Authored).count();
                println!(
                    "{} - circuit {}n/{}e, {authored} authored neuron(s)",
                    rt.creature.provenance().describe(),
                    sim.n,
                    brain.circuit.edges.len()
                );
                rt.body_flash = vec![0.0; sim.n];
                rt.sim = Some(sim);
                rt.brain_points = Some(brain.points);
            }
            Err(e) => eprintln!("no chimera data ({e}) - run etl_chimera.py; the spider runs brainless"),
        }
        rt
    }

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
        // The pounce node's spike is the other event worth a full flash.
        for &i in &sim.pounce {
            if sim.last_spikes.iter().any(|e| e.neuron == i) && i < self.body_flash.len() {
                self.body_flash[i] = 2.5;
            }
        }
    }

    /// Cheap deterministic scatter for bug placement, so a failure lands its
    /// bugs somewhere new each time without another RNG in the shell.
    fn scatter(&mut self) -> f32 {
        self.rng_phase = (self.rng_phase * 9.7 + 0.31).fract();
        self.rng_phase
    }

    fn spawn_bugs(&mut self, region: Region) {
        for _ in 0..BUGS_PER_FAIL {
            let ang = self.scatter() * std::f32::consts::TAU;
            let dist = 160.0 + self.scatter() * 220.0;
            let at = region.clamp_inside(
                Vec2::new(
                    self.spider.pos.x + ang.cos() * dist,
                    self.spider.pos.y + ang.sin() * dist,
                ),
                40.0,
            );
            let va = self.scatter() * std::f32::consts::TAU;
            self.spider
                .spawn_bug(at, Vec2::new(va.cos() * 30.0, va.sin() * 30.0));
        }
    }
}

impl Runtime for SpiderRuntime {
    fn creature(&self) -> &dyn Creature {
        &self.creature
    }
    fn substrate(&self) -> dfcore::Substrate {
        dfcore::creature::Body::substrate(&self.spider)
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
            None => "no data - run etl_chimera.py (brainless)".to_string(),
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
        self.spider.terrain = env.ledges.clone();
        self.foreground = env.foreground;
        self.spider.settled = env.foreground.is_work();

        // Cursor kinematics for the small-object channel. The looming channel
        // keeps its own inside `Transduction`; this one needs the velocity
        // vector, not the approach rate.
        if let (Some(m), Some(p)) = (env.cursor, self.prev_cursor) {
            if dt > 0.0 {
                let v = Vec2::new((m.x - p.x) / dt, (m.y - p.y) / dt);
                self.cursor_vel.x += (v.x - self.cursor_vel.x) * 0.4;
                self.cursor_vel.y += (v.y - self.cursor_vel.y) * 0.4;
            }
        }
        self.prev_cursor = env.cursor;

        // Build results: a failure releases bugs; a pass sends the survivors
        // off, which is what returns the spider to watching.
        for ev in &env.build_events {
            match ev {
                BuildEvent::Fail => {
                    self.fails_seen += 1;
                    println!("build failed - bugs are loose");
                    let region = self.region;
                    self.spawn_bugs(region);
                }
                BuildEvent::Pass => {
                    if !self.spider.prey.is_empty() {
                        println!("build passed - the bugs are gone");
                    }
                    self.spider.prey.clear();
                }
            }
        }

        if let Some(sim) = self.sim.as_mut() {
            let (tempo, sleepy) = self.trans.apply(sim, &self.spider, env, dt);
            self.pending_tempo = tempo;
            self.pending_sleepy = sleepy;

            // Small objects onto LC11. The cursor counts when it moves like a
            // gnat and is not close enough to be a looming threat.
            let extra = env.cursor.and_then(|m| {
                let d = hypot(m.x - self.spider.pos.x, m.y - self.spider.pos.y);
                let speed = hypot(self.cursor_vel.x, self.cursor_vel.y);
                (d > CURSOR_SMALL_MIN_DIST && speed > CURSOR_SMALL_MIN_SPEED)
                    .then_some((m, self.cursor_vel))
            });
            let (l, r) = self.spider.prey_drive(extra);
            sim.prey_l = l;
            sim.prey_r = r;
        }
    }

    fn tick(&mut self, dt: f32, region: Region, cursor: Option<Vec2>, attractor: Option<Vec2>) {
        self.region = region;
        self.spider.attractor = attractor;
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
        self.spider.update(dt, region, cursor, signals);
        self.flash_spikes((-dt * 7.0).exp());
    }

    fn build(&mut self, glass: bool) -> Geometry<'_> {
        let pose = self.spider.pose();
        spiderbody::build_frame(&mut self.frame_mesh, &self.meshes, &self.spider, &pose, glass);
        self.has_neurons = false;
        if glass {
            if let Some(sim) = self.sim.as_ref() {
                spiderbody::build_neuron_field(&mut self.neuron_mesh, sim, &self.body_flash, &self.spider, &pose);
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
        self.spider.pos
    }
    fn place(&mut self, at: Vec2) {
        self.spider.pos = at;
    }
    fn moved_display(&mut self, region: Region) {
        self.spider.terrain.clear();
        self.spider.ledge = None;
        self.spider.silk.clear();
        self.spider.pos = region.clamp_inside(self.spider.pos, 40.0);
    }
    fn status(&self) -> String {
        format!(
            "state {:?}  pos ({:.0},{:.0})  head {:+.2}  bugs {}  caught {}  front {:?}",
            self.spider.state,
            self.spider.pos.x,
            self.spider.pos.y,
            self.spider.head_yaw,
            self.spider.prey.len(),
            self.spider.captured,
            self.foreground
        )
    }

    fn snapshot_pose(&mut self, _alt: f32, walking_frames: u32, region: Region) {
        // A few steps of walking so the legs are mid-stride, then a bug just
        // ahead so the head is turned toward something.
        self.spider.state = dfcore::SpiderState::Walking;
        self.spider.speed = 30.0;
        self.spider.heading = 0.4;
        for _ in 0..walking_frames {
            self.spider.update(1.0 / 60.0, region, None, None);
        }
        self.spider.pos = Vec2::ZERO;
        self.spider.state = dfcore::SpiderState::Watching;
        self.spider.head_yaw = 0.55;
        self.spider.crouch = 0.35;
        self.spider.spawn_bug(Vec2::new(46.0, 30.0), Vec2::new(20.0, 5.0));
        if let Some(sim) = self.sim.as_mut() {
            sim.step(1500);
            // A faint small object in the left eye: enough for LC11 to
            // crackle and the pounce node to show, not enough to saturate
            // 127 cells into one white bloom (which is what a *seen* bug does
            // on the desktop, and is right there, but hides the anatomy here).
            sim.prey_l = 0.12;
            sim.prey_r = 0.05;
            sim.step(40);
            let spiked = sim.last_spikes.len();
            self.flash_spikes(0.0);
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
