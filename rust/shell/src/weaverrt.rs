//! Creatures #5–#7 on the desktop: the web-building chimeras (WEB_PLAN.md).
//!
//! One runtime for three species, because their senses, circuit and readouts
//! are identical; only the body's construction program and look differ. What
//! it shares with the fly it takes from the fly: looming, taps, wind,
//! proprioception and circadian transduction run unchanged. What is new is
//! one channel:
//!
//! - **Web vibration → the authored strike node.** The vibration under the
//!   spider's legs (a struggling bug, the cursor stirring the silk) drives
//!   the one authored neuron, a vibration sense with no synapses, on a slow
//!   membrane. Knocks — a cursor lunge, a click — go where they always went:
//!   through the fly's measured wind/tap partners onto the giant fiber, and
//!   the spider drops. The gain here is modelled and labelled.
//!
//! The cursor also *cuts* silk when it sweeps through fast — the web is on
//! the desktop, and a mouse dragged across it is a hand through a web.

use dfcore::data::BrainPointsFile;
use dfcore::env::BuildEvent;
use dfcore::util::hypot;
use dfcore::Region;
use dfcore::{
    Creature, EnvSnapshot, Habituation, LifParams, LifSim, Origin, SignalBuilder, Sim, Vec2,
    Weaver as Species, WeaverBody, WeaverState,
};

use crate::mesh::Mesh;
use crate::runtime::{Geometry, Runtime};
use crate::transduction::Transduction;
use crate::weaverbody::{self, WeaverMeshes};

/// Vibration under the legs → the strike node's input, 0..1. A struggling
/// bug reads ~0.2 at the hub; this puts it near full drive.
const VIBRATION_GAIN: f32 = 4.0;
/// A cursor moving faster than this through silk cuts it.
const CUT_SPEED: f32 = 700.0;
const CUT_RADIUS: f32 = 5.0;
/// Bugs per reported failure, and the ambient spawn interval band.
const BUGS_PER_FAIL: usize = 3;
const AMBIENT_BUG_SECS: (f32, f32) = (35.0, 90.0);

use dfcore::weaver::program_for;

pub struct WeaverRuntime {
    creature: Species,
    sim: Option<LifSim>,
    brain_points: Option<BrainPointsFile>,
    signals: SignalBuilder,
    body: WeaverBody,
    trans: Transduction,

    meshes: WeaverMeshes,
    frame_mesh: Mesh,
    neuron_mesh: Mesh,
    body_flash: Vec<f32>,
    has_neurons: bool,

    ms_accumulator: f64,
    pending_tempo: f32,
    pending_sleepy: bool,
    prev_cursor: Option<Vec2>,
    cursor_vel: Vec2,
    fails_seen: u32,
    rng_phase: f32,
    next_bug: f32,
    region: Region,
    seed: u64,
}

impl WeaverRuntime {
    pub fn new(species: Species, seed: u64) -> Self {
        let region = Region::centered((1920.0, 1080.0));
        let mut rng = dfcore::rng::Pcg32::new(seed ^ 0x5e1f);
        let program = program_for(species, region, &mut rng);
        let mut rt = WeaverRuntime {
            sim: None,
            brain_points: None,
            signals: SignalBuilder::new(),
            body: WeaverBody::new(species, Vec2::ZERO, seed, program),
            trans: Transduction::new(),
            meshes: WeaverMeshes::build(species),
            frame_mesh: Mesh::default(),
            neuron_mesh: Mesh::default(),
            body_flash: Vec::new(),
            has_neurons: false,
            ms_accumulator: 0.0,
            pending_tempo: 1.0,
            pending_sleepy: false,
            prev_cursor: None,
            cursor_vel: Vec2::ZERO,
            fails_seen: 0,
            rng_phase: 0.41,
            next_bug: 20.0,
            region,
            seed,
            creature: species,
        };
        match dfcore::data::load_for(rt.creature.data_dir()) {
            Ok(brain) => {
                let mut sim =
                    LifSim::with_params(&brain.circuit, seed, LifParams::default(), rt.creature.manifest());
                sim.collect_spikes = true;
                let authored = (0..sim.n).filter(|&i| sim.origin(i) == Origin::Authored).count();
                println!(
                    "{} ({}) - circuit {}n/{}e, {authored} authored neuron(s); web program: PROCEDURAL",
                    rt.creature.provenance().describe(),
                    rt.creature.species(),
                    sim.n,
                    brain.circuit.edges.len()
                );
                rt.body_flash = vec![0.0; sim.n];
                rt.sim = Some(sim);
                rt.brain_points = Some(brain.points);
            }
            Err(e) => eprintln!("no weaver data ({e}) - run etl_weaver.py; the spider runs brainless"),
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
        for &i in &sim.strike {
            if sim.last_spikes.iter().any(|e| e.neuron == i) && i < self.body_flash.len() {
                self.body_flash[i] = 2.5;
            }
        }
    }

    fn scatter(&mut self) -> f32 {
        self.rng_phase = (self.rng_phase * 9.7 + 0.31).fract();
        self.rng_phase
    }

    /// A bug from an edge of the region, heading across it — so it has a
    /// web to blunder into. Grounded for the gumfoot weaver.
    fn spawn_bug(&mut self) {
        let region = self.region;
        let (hw, hh) = region.half();
        let grounded = self.creature == Species::Parasteatoda && self.scatter() < 0.7;
        let s = self.scatter();
        let (at, vel) = if grounded {
            let x = region.center.x + (s * 2.0 - 1.0) * (hw - 30.0);
            (Vec2::new(x, region.center.y - hh + 6.0), Vec2::new(if s < 0.5 { 30.0 } else { -30.0 }, 0.0))
        } else {
            let side = (self.scatter() * 4.0) as u32;
            let along = self.scatter() * 2.0 - 1.0;
            let at = match side {
                0 => Vec2::new(region.center.x + along * hw, region.center.y + hh - 25.0),
                1 => Vec2::new(region.center.x + along * hw, region.center.y - hh + 25.0),
                2 => Vec2::new(region.center.x - hw + 25.0, region.center.y + along * hh),
                _ => Vec2::new(region.center.x + hw - 25.0, region.center.y + along * hh),
            };
            let to = Vec2::new(
                region.center.x + (self.scatter() - 0.5) * hw,
                region.center.y + (self.scatter() - 0.5) * hh,
            );
            let d = hypot(to.x - at.x, to.y - at.y).max(1.0);
            (at, Vec2::new((to.x - at.x) / d * 55.0, (to.y - at.y) / d * 55.0))
        };
        self.body.spawn_bug(at, vel, grounded);
    }

    fn spawn_bugs(&mut self) {
        for _ in 0..BUGS_PER_FAIL {
            self.spawn_bug();
        }
    }
}

impl Runtime for WeaverRuntime {
    fn creature(&self) -> &dyn Creature {
        &self.creature
    }
    fn substrate(&self) -> dfcore::Substrate {
        dfcore::creature::Body::substrate(&self.body)
    }
    fn prefers_habitat(&self) -> bool {
        true
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
            None => "no data - run etl_weaver.py (brainless); web program: PROCEDURAL".to_string(),
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
        self.body.terrain = env.ledges.clone();
        self.body.settled = env.foreground.is_work();
        self.body.hour = env.local_hour;

        // Cursor kinematics: a fast sweep through the silk cuts it; a slow
        // pass stirs it a little, the way it stirs the plants in a tank.
        if let (Some(m), Some(p)) = (env.cursor, self.prev_cursor) {
            if dt > 0.0 {
                let v = Vec2::new((m.x - p.x) / dt, (m.y - p.y) / dt);
                self.cursor_vel.x += (v.x - self.cursor_vel.x) * 0.4;
                self.cursor_vel.y += (v.y - self.cursor_vel.y) * 0.4;
                let speed = hypot(v.x, v.y);
                if speed > CUT_SPEED {
                    let steps = ((hypot(m.x - p.x, m.y - p.y) / CUT_RADIUS) as usize).clamp(1, 24);
                    for k in 0..=steps {
                        let t = k as f32 / steps as f32;
                        let at = Vec2::new(p.x + (m.x - p.x) * t, p.y + (m.y - p.y) * t);
                        self.body.damage(at, CUT_RADIUS);
                    }
                } else if speed > 40.0 {
                    self.body.silk.excite(m, 0.08 * dt);
                }
            }
        }
        self.prev_cursor = env.cursor;

        for ev in &env.build_events {
            match ev {
                BuildEvent::Fail => {
                    self.fails_seen += 1;
                    println!("build failed - bugs are loose");
                    self.spawn_bugs();
                }
                BuildEvent::Pass => {
                    if !self.body.prey.is_empty() {
                        println!("build passed - the bugs are gone");
                    }
                    self.body.prey.clear();
                }
            }
        }
        // An occasional bug regardless, so the web gets used.
        self.next_bug -= dt;
        if self.next_bug <= 0.0 {
            self.spawn_bug();
            let s = self.scatter();
            self.next_bug = AMBIENT_BUG_SECS.0 + s * (AMBIENT_BUG_SECS.1 - AMBIENT_BUG_SECS.0);
        }

        if let Some(sim) = self.sim.as_mut() {
            let (tempo, sleepy) = self.trans.apply(sim, &self.body, env, dt);
            self.pending_tempo = tempo;
            self.pending_sleepy = sleepy;
            // Web vibration onto the authored strike node, and nowhere else:
            // the measured wind pathway is for knocks (cursor lunges, clicks).
            sim.vibration = (self.body.felt * VIBRATION_GAIN).min(1.0);
        }
    }

    fn tick(&mut self, dt: f32, region: Region, cursor: Option<Vec2>, attractor: Option<Vec2>) {
        if region != self.region {
            self.moved_display(region);
        }
        self.body.attractor = attractor;
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
        self.body.update(dt, region, cursor, signals);
        self.flash_spikes((-dt * 7.0).exp());
    }

    fn build(&mut self, glass: bool) -> Geometry<'_> {
        let pose = self.body.pose();
        weaverbody::build_frame(&mut self.frame_mesh, &self.meshes, &self.body, &pose, glass);
        self.has_neurons = false;
        if glass {
            if let Some(sim) = self.sim.as_ref() {
                weaverbody::build_neuron_field(&mut self.neuron_mesh, sim, &self.body_flash, &self.body, &pose);
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
        self.body.pos
    }
    fn place(&mut self, at: Vec2) {
        self.body.pos = at;
    }
    fn moved_display(&mut self, region: Region) {
        // The anchors moved out from under the web: start over in the new
        // box. (Phase 6 makes this per-anchor.)
        self.region = region;
        self.body.terrain.clear();
        self.body.silk.clear();
        self.body.prey.clear();
        let mut rng = dfcore::rng::Pcg32::new(self.seed ^ 0x5e1f ^ (region.size.0 as u64));
        self.body.program = program_for(self.creature, region, &mut rng);
        // The body picks where in the new world the web goes on its next
        // frame (an enclosure whole, or a box under a window edge).
        self.body.build_region = None;
        self.body.anchor_ledge = None;
        self.body.pos = region.clamp_inside(self.body.pos, 40.0);
        let _ = WeaverState::Sitting;
    }
    fn status(&self) -> String {
        format!(
            "state {:?}  {} {:.0}%  threads {}  pos ({:.0},{:.0})  bugs {}  caught {}  felt {:.3}",
            self.body.state,
            self.body.program.stage(),
            self.body.program.progress() * 100.0,
            self.body.silk.threads.len(),
            self.body.pos.x,
            self.body.pos.y,
            self.body.prey.len(),
            self.body.captured,
            self.body.felt,
        )
    }

    fn snapshot_pose(&mut self, _alt: f32, walking_frames: u32, region: Region) {
        // Build most of a web brainless, so the snapshot shows the animal
        // mid-capture-spiral rather than on a bare floor.
        self.moved_display(region);
        let mut s = dfcore::BrainSignals::new();
        s.walk_drive = 0.6;
        let mut frames = 0;
        while self.body.program.progress() < 0.8 && frames < 40_000 {
            self.body.update(1.0 / 60.0, region, None, Some(s));
            frames += 1;
        }
        for _ in 0..walking_frames {
            self.body.update(1.0 / 60.0, region, None, Some(s));
        }
        let at = self.body.pos;
        if let Some((t, _)) = self.body.silk.thread_near(
            Vec2::new(at.x + 60.0, at.y + 30.0),
            Some(dfcore::ThreadKind::Capture),
            40.0,
        ) {
            let p = self.body.silk.point_on_thread(t, Vec2::new(at.x + 60.0, at.y + 30.0));
            self.body.spawn_bug(p, Vec2::ZERO, false);
            let n = self.body.prey.len() - 1;
            self.body.prey[n].stuck = true;
            self.body.prey[n].struggle = 0.8;
        }
        if let Some(sim) = self.sim.as_mut() {
            sim.step(1500);
            sim.air_puff = 0.12;
            sim.step(60);
            let spiked = sim.last_spikes.len();
            self.flash_spikes(0.0);
            for f in self.body_flash.iter_mut() {
                *f *= 0.9;
            }
            println!(
                "snapshot: {} neurons lit ({} spiked this step); web {} {:.0}%",
                self.body_flash.iter().filter(|f| **f >= 0.012).count(),
                spiked,
                self.body.program.stage(),
                self.body.program.progress() * 100.0
            );
        }
    }
}
