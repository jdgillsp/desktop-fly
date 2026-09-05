//! The 1 kHz leaky-integrate-and-fire simulation.
//!
//! A faithful transliteration of `Sim.swift:75-342`. Every constant here is
//! **data, not a tunable** — `CLAUDE.md` warns the operating point is razor
//! thin (neurons rest at `baseline × 20.4` against a threshold of 1.0), and the
//! escape behaviour depends on a race between gap-junction-boosted excitation
//! and ~1,200 synapses of 4 ms-delayed feedforward inhibition. Changing any of
//! these silently changes the animal.
//!
//! Step order within each simulated millisecond is load-bearing and matches the
//! Swift original exactly:
//!   1. advance clock, maybe start a noise burst
//!   2. leak + baseline + noise for every non-refractory neuron
//!   3. sensory injection (loom, gait proprioception, air puff, stimulation)
//!   4. deliver inhibition scheduled for *this* millisecond
//!   5. detect spikes (all at once — no within-millisecond cascade)
//!   6. propagate: excitation immediately, inhibition into the delay ring
//!   7. update population rate EMAs

use crate::data::CircuitFile;
use crate::rng::Pcg32;

/// Fixed parameters of the fly's spiking dynamics (Sim.swift:129-140).
#[derive(Debug, Clone, Copy)]
pub struct LifParams {
    /// exp(-1/20): 20 ms membrane tau at a 1 ms step.
    pub decay: f32,
    pub threshold: f32,
    pub refractory_ms: f32,
    pub weight_scale: f32,
    pub p_noise: f32,
    pub noise_kick: f32,
    pub loom_gain: f32,
    pub rate_alpha: f32,
    pub inh_delay_ms: usize,
    /// LC->GF and wind->GF are electrically coupled; chemical synapse counts
    /// under-represent that, so the drive is boosted.
    pub gap_junction_boost: f32,
}

impl Default for LifParams {
    fn default() -> Self {
        Self {
            decay: 0.9512,
            threshold: 1.0,
            refractory_ms: 2.0,
            weight_scale: 0.0008,
            p_noise: 0.0022,
            noise_kick: 0.42,
            loom_gain: 0.30,
            rate_alpha: 1.0 / 120.0,
            inh_delay_ms: 4,
            gap_junction_boost: 6.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SpikeEvent {
    pub neuron: usize,
    pub is_gf: bool,
}

#[derive(Debug, Clone)]
struct Stim {
    idx: Vec<usize>,
    strength: f32,
    duration_ms: i64,
    until_ms: i64,
}

pub struct LifSim {
    pub n: usize,
    pub roles: Vec<String>,
    pub types: Vec<String>,
    pub positions: Vec<[f32; 3]>,
    params: LifParams,

    // membrane state
    v: Vec<f32>,
    refr: Vec<f32>,
    baseline: Vec<f32>,

    // CSR adjacency with pre-scaled weights
    row_start: Vec<usize>,
    col_idx: Vec<u32>,
    w: Vec<f32>,

    // populations
    pub loom_left: Vec<usize>,
    pub loom_right: Vec<usize>,
    pub gf: Vec<usize>,
    pub dna_l: Vec<usize>,
    pub dna_r: Vec<usize>,
    pub mdn: Vec<usize>,
    pub fwd: Vec<usize>,
    pub groom: Vec<usize>,
    pub escw: Vec<usize>,
    pub ascend: Vec<usize>,
    pub sens: Vec<usize>,
    ascend_phase: Vec<f32>,
    /// Per-neuron membership flags, so the hot loop avoids `dnaL.contains(i)`.
    is_dna_left: Vec<bool>,

    // inputs, written each frame by the coordinator
    pub loom_l: f32,
    pub loom_r: f32,
    pub gait_drive: f32,
    pub gait_phase: f32,
    pub air_puff: f32,
    pub activity_scale: f32,
    pub sensory_gate: f32,

    // outputs (Hz per neuron, exponential moving averages)
    pub rate_loom: f32,
    pub rate_dna_l: f32,
    pub rate_dna_r: f32,
    pub rate_mdn: f32,
    pub rate_fwd: f32,
    pub rate_groom: f32,
    pub rate_escw: f32,
    pub rate_pop: f32,

    gf_latch: bool,
    pub sim_ms: i64,
    pub total_spikes: u64,

    // delayed inhibition ring buffer
    inh_queue: Vec<Vec<f32>>,
    q_head: usize,

    burst_until: i64,
    burst_next: i64,
    rng: Pcg32,

    pending_stims: Vec<Stim>,
    active_stims: Vec<Stim>,

    /// Spikes observed in the most recent `step`, for the brain window.
    pub collect_spikes: bool,
    pub last_spikes: Vec<SpikeEvent>,
}

impl LifSim {
    pub fn new(circuit: &CircuitFile, seed: u64) -> Self {
        Self::with_params(circuit, seed, LifParams::default())
    }

    pub fn with_params(circuit: &CircuitFile, seed: u64, params: LifParams) -> Self {
        let mut rng = Pcg32::new(seed);
        let n = circuit.neurons.len();

        let roles: Vec<String> = circuit.neurons.iter().map(|x| x.role.clone()).collect();
        let types: Vec<String> = circuit.neurons.iter().map(|x| x.cell_type.clone()).collect();
        let positions: Vec<[f32; 3]> = circuit
            .neurons
            .iter()
            .map(|x| {
                if x.pos.len() == 3 {
                    [x.pos[0], x.pos[1], x.pos[2]]
                } else {
                    [0.0, 0.0, 0.0]
                }
            })
            .collect();

        let (mut loom_left, mut loom_right) = (Vec::new(), Vec::new());
        let (mut gf, mut dna_l, mut dna_r) = (Vec::new(), Vec::new(), Vec::new());
        let (mut mdn, mut fwd, mut groom, mut escw) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let (mut ascend, mut sens) = (Vec::new(), Vec::new());

        for (i, nr) in circuit.neurons.iter().enumerate() {
            match nr.role.as_str() {
                "lc4" | "lplc2" => {
                    if nr.side == "left" {
                        loom_left.push(i)
                    } else {
                        loom_right.push(i)
                    }
                }
                "gf" => gf.push(i),
                "dna01" | "dna02" => {
                    if nr.side == "left" {
                        dna_l.push(i)
                    } else {
                        dna_r.push(i)
                    }
                }
                "mdn" => mdn.push(i),
                "dnp09" => fwd.push(i),
                "dng11" => groom.push(i),
                "escw" => escw.push(i),
                // Partners keep their FlyWire super_class in `type`.
                "other" => match nr.cell_type.as_str() {
                    "ascending" => ascend.push(i),
                    "sensory" => sens.push(i),
                    _ => {}
                },
                _ => {}
            }
        }

        let mut is_dna_left = vec![false; n];
        for &i in &dna_l {
            is_dna_left[i] = true;
        }

        let ascend_phase: Vec<f32> = ascend
            .iter()
            .map(|_| rng.range(0.0, 2.0 * std::f32::consts::PI))
            .collect();

        // Heterogeneous excitability. Interneurons crackle at a few Hz;
        // sensory and command neurons stay quiet unless driven. Bilateral
        // command DNs get a *deterministic* baseline on purpose — their
        // left/right asymmetry must come from the wiring, never from luck
        // (Sim.swift:196, and the CLAUDE.md recipe step 4).
        let mut baseline = vec![0.0f32; n];
        for (i, nr) in circuit.neurons.iter().enumerate() {
            baseline[i] = match nr.role.as_str() {
                "other" => rng.range(0.010, 0.070),
                "lc4" | "lplc2" => 0.004,
                "dna01" | "dna02" | "mdn" | "dng11" | "escw" => 0.036,
                "dnp09" => 0.038,
                _ => 0.002, // gf: silent unless synaptically driven
            };
        }

        // CSR build.
        let mut counts = vec![0usize; n];
        for e in &circuit.edges {
            counts[e[0] as usize] += 1;
        }
        let mut row_start = vec![0usize; n + 1];
        for i in 0..n {
            row_start[i + 1] = row_start[i] + counts[i];
        }
        let mut col_idx = vec![0u32; circuit.edges.len()];
        let mut w = vec![0.0f32; circuit.edges.len()];
        let mut fill = row_start.clone();
        for e in &circuit.edges {
            let pre = e[0] as usize;
            let post = e[1] as usize;
            let mut weight = e[2] * params.weight_scale;
            let electrical = roles[pre] == "lc4"
                || roles[pre] == "lplc2"
                || (roles[pre] == "other" && types[pre] == "sensory");
            if electrical && roles[post] == "gf" {
                weight *= params.gap_junction_boost;
            }
            col_idx[fill[pre]] = post as u32;
            w[fill[pre]] = weight;
            fill[pre] += 1;
        }

        LifSim {
            n,
            roles,
            types,
            positions,
            params,
            v: vec![0.0; n],
            refr: vec![0.0; n],
            baseline,
            row_start,
            col_idx,
            w,
            loom_left,
            loom_right,
            gf,
            dna_l,
            dna_r,
            mdn,
            fwd,
            groom,
            escw,
            ascend,
            sens,
            ascend_phase,
            is_dna_left,
            loom_l: 0.0,
            loom_r: 0.0,
            gait_drive: 0.0,
            gait_phase: 0.0,
            air_puff: 0.0,
            activity_scale: 1.0,
            sensory_gate: 1.0,
            rate_loom: 0.0,
            rate_dna_l: 0.0,
            rate_dna_r: 0.0,
            rate_mdn: 0.0,
            rate_fwd: 0.0,
            rate_groom: 0.0,
            rate_escw: 0.0,
            rate_pop: 0.0,
            gf_latch: false,
            sim_ms: 0,
            total_spikes: 0,
            inh_queue: vec![vec![0.0; n]; 5],
            q_head: 0,
            burst_until: 0,
            burst_next: 12_000,
            rng,
            pending_stims: Vec::new(),
            active_stims: Vec::new(),
            collect_spikes: false,
            last_spikes: Vec::new(),
        }
    }

    /// "Optogenetic" stimulation, as the brain window's click-to-stimulate uses.
    /// In Swift this needed a lock because it was called from the UI thread; in
    /// the Rust shell the sim owns its thread and this is a plain method.
    pub fn stimulate(&mut self, indices: &[usize], strength: f32, duration_ms: i64) {
        if indices.is_empty() {
            return;
        }
        self.pending_stims.push(Stim {
            idx: indices.to_vec(),
            strength,
            duration_ms,
            until_ms: 0,
        });
        if self.pending_stims.len() > 8 {
            self.pending_stims.remove(0);
        }
    }

    /// Reads and clears the giant-fiber latch. A spike means "take off NOW".
    pub fn consume_gf(&mut self) -> bool {
        let s = self.gf_latch;
        self.gf_latch = false;
        s
    }

    pub fn step(&mut self, ms: i64) {
        if ms <= 0 {
            return;
        }
        for mut p in self.pending_stims.drain(..).collect::<Vec<_>>() {
            p.until_ms = self.sim_ms + p.duration_ms;
            self.active_stims.push(p);
        }
        let now = self.sim_ms;
        self.active_stims.retain(|s| now < s.until_ms);

        if self.collect_spikes {
            self.last_spikes.clear();
        }

        let p = &self.params;
        let n = self.n;

        for _ in 0..ms {
            self.sim_ms += 1;
            if self.sim_ms >= self.burst_next {
                self.burst_until = self.sim_ms + 400;
                self.burst_next = self.sim_ms + self.rng.int_range(15_000, 40_000);
            }
            let p_noise = if self.sim_ms < self.burst_until {
                p.p_noise * 6.0
            } else {
                p.p_noise
            } * self.activity_scale;

            // 2. leak, baseline, noise
            for i in 0..n {
                if self.refr[i] > 0.0 {
                    self.refr[i] -= 1.0;
                    self.v[i] *= p.decay;
                    continue;
                }
                let mut vi = self.v[i] * p.decay + self.baseline[i] * self.activity_scale;
                if self.rng.f32() < p_noise {
                    vi += p.noise_kick;
                }
                self.v[i] = vi;
            }

            // 3. sensory injection
            if self.loom_l > 0.001 {
                let d = self.loom_l * p.loom_gain * self.sensory_gate;
                for &i in &self.loom_left {
                    self.v[i] += d;
                }
            }
            if self.loom_r > 0.001 {
                let d = self.loom_r * p.loom_gain * self.sensory_gate;
                for &i in &self.loom_right {
                    self.v[i] += d;
                }
            }
            // body -> brain: the gait rhythm drives real ascending
            // (proprioceptive) neurons, in phase with the legs.
            if self.gait_drive > 0.001 {
                let ph = self.gait_phase * 2.0 * std::f32::consts::PI;
                for (k, &i) in self.ascend.iter().enumerate() {
                    self.v[i] +=
                        self.gait_drive * 0.09 * (0.5 + 0.5 * (ph + self.ascend_phase[k]).sin());
                }
            }
            if self.air_puff > 0.001 {
                let d = self.air_puff * 0.12 * self.sensory_gate;
                for &i in &self.sens {
                    self.v[i] += d;
                }
            }
            for s in &self.active_stims {
                if self.sim_ms < s.until_ms {
                    for &i in &s.idx {
                        self.v[i] += s.strength;
                    }
                }
            }

            // 4. inhibition scheduled for this millisecond
            {
                let slot = &mut self.inh_queue[self.q_head];
                for j in 0..n {
                    if slot[j] != 0.0 {
                        self.v[j] = (self.v[j] + slot[j]).max(-2.0);
                        slot[j] = 0.0;
                    }
                }
            }

            // 5. spike detection — all at once, no within-millisecond cascade
            let mut spiked: Vec<usize> = Vec::new();
            for i in 0..n {
                if self.refr[i] <= 0.0 && self.v[i] >= p.threshold {
                    self.v[i] = 0.0;
                    self.refr[i] = p.refractory_ms;
                    spiked.push(i);
                }
            }
            self.total_spikes += spiked.len() as u64;

            // 6. propagate
            let inh_slot = (self.q_head + p.inh_delay_ms) % self.inh_queue.len();
            for &i in &spiked {
                for k in self.row_start[i]..self.row_start[i + 1] {
                    let j = self.col_idx[k] as usize;
                    let wk = self.w[k];
                    if wk >= 0.0 {
                        self.v[j] = (self.v[j] + wk).max(-2.0);
                    } else {
                        self.inh_queue[inh_slot][j] += wk;
                    }
                }
            }
            self.q_head = (self.q_head + 1) % self.inh_queue.len();

            // 7. population rates
            let (mut c_loom, mut c_dl, mut c_dr) = (0u32, 0u32, 0u32);
            let (mut c_m, mut c_f, mut c_g, mut c_w) = (0u32, 0u32, 0u32, 0u32);
            for &i in &spiked {
                match self.roles[i].as_str() {
                    "lc4" | "lplc2" => c_loom += 1,
                    "dna01" | "dna02" => {
                        if self.is_dna_left[i] {
                            c_dl += 1
                        } else {
                            c_dr += 1
                        }
                    }
                    "mdn" => c_m += 1,
                    "dnp09" => c_f += 1,
                    "dng11" => c_g += 1,
                    "escw" => c_w += 1,
                    "gf" => self.gf_latch = true,
                    _ => {}
                }
            }
            let a = p.rate_alpha;
            let n_loom = (self.loom_left.len() + self.loom_right.len()).max(1) as f32;
            self.rate_loom += (c_loom as f32 * 1000.0 / n_loom - self.rate_loom) * a;
            self.rate_dna_l +=
                (c_dl as f32 * 1000.0 / self.dna_l.len().max(1) as f32 - self.rate_dna_l) * a;
            self.rate_dna_r +=
                (c_dr as f32 * 1000.0 / self.dna_r.len().max(1) as f32 - self.rate_dna_r) * a;
            self.rate_mdn +=
                (c_m as f32 * 1000.0 / self.mdn.len().max(1) as f32 - self.rate_mdn) * a;
            self.rate_fwd +=
                (c_f as f32 * 1000.0 / self.fwd.len().max(1) as f32 - self.rate_fwd) * a;
            self.rate_groom +=
                (c_g as f32 * 1000.0 / self.groom.len().max(1) as f32 - self.rate_groom) * a;
            self.rate_escw +=
                (c_w as f32 * 1000.0 / self.escw.len().max(1) as f32 - self.rate_escw) * a;
            self.rate_pop +=
                (spiked.len() as f32 * 1000.0 / n.max(1) as f32 - self.rate_pop) * a;

            if self.collect_spikes && !spiked.is_empty() {
                // Sample under heavy activity, as the Swift SpikeBus does.
                let stride = (spiked.len() / 12).max(1);
                let mut i = 0;
                while i < spiked.len() {
                    self.last_spikes.push(SpikeEvent {
                        neuron: spiked[i],
                        is_gf: self.roles[spiked[i]] == "gf",
                    });
                    i += stride;
                }
            }
        }
    }
}
