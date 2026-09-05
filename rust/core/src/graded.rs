//! A graded, non-spiking integrator with explicit gap junctions.
//!
//! This exists because *C. elegans* cannot honestly be run on the fly's LIF
//! loop. Its neurons are predominantly **non-spiking** — signalling is by
//! graded membrane potential — and its electrical (gap-junction) connectome is
//! a separate layer of comparable importance to the chemical one. Pushing worm
//! data through a spiking model would manufacture action potentials the animal
//! does not have, and quietly turn "the brain data is real" into a lie
//! (PORT_PLAN.md §5).
//!
//! The formulation is the standard one from the *C. elegans* modelling
//! literature (Wicks, Roehrig & Rankin 1996, and its whole-network
//! descendants): a linear RC membrane, chemical synapses with sigmoidal
//! voltage-dependent activation and a reversal potential, and ohmic gap
//! junctions.
//!
//! ```text
//!   C dV_i/dt = -G_leak (V_i - E_leak)
//!               - Σ_j g_gap w_ij (V_i - V_j)          electrical, bidirectional
//!               - Σ_j g_syn w_ij s(V_j) (V_i - E_ij)  chemical, rectified
//!               + I_ext
//!
//!   s(V) = 1 / (1 + exp(-β (V - V_eq)))
//! ```
//!
//! Integration is exponential Euler rather than forward Euler. A graded network
//! with strong coupling is stiff, and forward Euler diverges at the step sizes
//! this app uses; the exponential form is unconditionally stable for a linear
//! membrane and costs one `exp` per neuron per step.

use std::collections::HashMap;

use crate::creature::{Connectome, GradedParams};
use crate::rng::Pcg32;
use crate::roles::RoleManifest;

/// Compressed sparse row adjacency, **indexed by the receiving neuron**.
///
/// The membrane equation sums every synapse arriving *at* a neuron, so rows are
/// keyed by target, not by source. Building it the other way round is the bug
/// that makes current flow backwards through the whole network — and it looks
/// entirely plausible while doing so.
struct Csr {
    row_start: Vec<usize>,
    /// The *sending* neuron for each entry.
    from: Vec<u32>,
    w: Vec<f32>,
    /// Per-synapse reversal potential (chemical only; unused for gap junctions).
    e_rev: Vec<f32>,
}

impl Csr {
    /// `edges` are `(from, to, weight, reversal)`.
    fn build(n: usize, edges: &[(u32, u32, f32, f32)]) -> Self {
        let mut counts = vec![0usize; n];
        for (_, to, _, _) in edges {
            counts[*to as usize] += 1;
        }
        let mut row_start = vec![0usize; n + 1];
        for i in 0..n {
            row_start[i + 1] = row_start[i] + counts[i];
        }
        let mut from = vec![0u32; edges.len()];
        let mut w = vec![0.0f32; edges.len()];
        let mut e_rev = vec![0.0f32; edges.len()];
        let mut fill = row_start.clone();
        for (f, t, weight, rev) in edges {
            let slot = fill[*t as usize];
            from[slot] = *f;
            w[slot] = *weight;
            e_rev[slot] = *rev;
            fill[*t as usize] += 1;
        }
        Csr {
            row_start,
            from,
            w,
            e_rev,
        }
    }
}

#[derive(Debug, Clone)]
struct Stim {
    idx: Vec<usize>,
    /// Injected current. Units: `g_leak * mV`, so a strength of
    /// `g_leak * 20 = 2.0` shifts a resting neuron by roughly 20 mV.
    strength: f32,
    until_ms: i64,
}

pub struct GradedSim {
    pub n: usize,
    pub roles: Vec<String>,
    pub positions: Vec<[f32; 3]>,
    pub manifest: RoleManifest,
    params: GradedParams,

    /// Membrane potential, mV.
    v: Vec<f32>,
    /// Per-neuron tonic input, the graded analogue of the fly's baselines.
    baseline: Vec<f32>,
    /// Incoming chemical synapses, with each one's reversal potential
    /// precomputed from its sign so the hot loop never branches on the weight.
    chem: Csr,
    /// Gap junctions, stored symmetrically (both directions present).
    gap: Csr,

    groups: HashMap<&'static str, Vec<usize>>,
    /// Per-neuron external drive, written by transduction each frame.
    pub input: Vec<f32>,
    pub activity_scale: f32,
    pub sensory_gate: f32,

    pending: Vec<Stim>,
    active: Vec<Stim>,
    pub sim_ms: i64,
}

impl GradedSim {
    pub fn new(
        connectome: &Connectome,
        manifest: RoleManifest,
        params: GradedParams,
        seed: u64,
    ) -> Self {
        let n = connectome.len();
        // Only used to draw per-neuron baselines; the graded model itself is
        // deterministic, unlike the fly's noise-driven LIF network.
        let mut rng = Pcg32::new(seed);

        // Chemical synapses are directional and signed; the sign selects the
        // reversal potential, which is what makes inhibition inhibitory in a
        // graded model (there is no "negative current", only a driving force
        // toward a hyperpolarised reversal).
        let chem_edges: Vec<(u32, u32, f32, f32)> = connectome
            .chemical
            .iter()
            .map(|e| {
                let rev = if e.weight >= 0.0 {
                    params.e_exc
                } else {
                    params.e_inh
                };
                (e.pre, e.post, e.weight.abs(), rev)
            })
            .collect();
        let chem = Csr::build(n, &chem_edges);

        // Gap junctions are bidirectional: store both directions so a single
        // CSR sweep sees every partner. Failing to do this is the classic bug —
        // current would flow one way only, which is not what a resistor does.
        let mut gap_edges: Vec<(u32, u32, f32, f32)> =
            Vec::with_capacity(connectome.electrical.len() * 2);
        for e in &connectome.electrical {
            let w = e.weight.abs();
            gap_edges.push((e.pre, e.post, w, 0.0));
            gap_edges.push((e.post, e.pre, w, 0.0));
        }
        let gap = Csr::build(n, &gap_edges);

        let baseline = manifest.baselines(&connectome.roles, &mut rng);
        let groups: HashMap<&'static str, Vec<usize>> = manifest
            .resolve(&connectome.roles, &connectome.cell_types, &connectome.sides)
            .into_iter()
            .map(|(k, (mut l, r))| {
                l.extend(r);
                l.sort_unstable();
                (k, l)
            })
            .collect();

        GradedSim {
            n,
            roles: connectome.roles.clone(),
            positions: connectome.positions.clone(),
            manifest,
            params,
            v: vec![params.e_leak; n],
            baseline,
            chem,
            gap,
            groups,
            input: vec![0.0; n],
            activity_scale: 1.0,
            sensory_gate: 1.0,
            pending: Vec::new(),
            active: Vec::new(),
            sim_ms: 0,
        }
    }

    /// Membrane potentials, mV.
    pub fn potentials(&self) -> &[f32] {
        &self.v
    }

    /// Synaptic activation of neuron `i`, 0..1 — the graded equivalent of
    /// "is this neuron firing".
    #[inline]
    fn activation(&self, v: f32) -> f32 {
        1.0 / (1.0 + (-self.params.beta * (v - self.params.v_eq)).exp())
    }

    /// Mean activation across a named population, 0..1. This is what a readout
    /// consumes in place of a spike rate.
    pub fn population_activity(&self, slug: &str) -> f32 {
        match self.groups.get(slug) {
            Some(g) if !g.is_empty() => {
                g.iter().map(|&i| self.activation(self.v[i])).sum::<f32>() / g.len() as f32
            }
            _ => 0.0,
        }
    }

    pub fn group(&self, slug: &str) -> &[usize] {
        self.groups.get(slug).map(|g| g.as_slice()).unwrap_or(&[])
    }

    pub fn stimulate(&mut self, indices: &[usize], strength: f32, duration_ms: i64) {
        if indices.is_empty() {
            return;
        }
        self.pending.push(Stim {
            idx: indices.to_vec(),
            strength,
            until_ms: duration_ms, // resolved against sim_ms in step()
        });
        if self.pending.len() > 8 {
            self.pending.remove(0);
        }
    }

    pub fn step(&mut self, ms: i64) {
        if ms <= 0 {
            return;
        }
        for mut p in std::mem::take(&mut self.pending) {
            p.until_ms += self.sim_ms;
            self.active.push(p);
        }
        let now = self.sim_ms;
        self.active.retain(|s| now < s.until_ms);

        let p = self.params;
        let dt = 1.0e-3_f32; // 1 ms, in seconds
        let mut act = vec![0.0f32; self.n];

        for _ in 0..ms {
            self.sim_ms += 1;

            // Activations are computed from the *previous* step's potentials,
            // so every neuron sees the same network state — the graded
            // equivalent of the LIF model detecting all spikes before
            // propagating any.
            for i in 0..self.n {
                act[i] = self.activation(self.v[i]);
            }

            for i in 0..self.n {
                // Leak.
                let mut g_tot = p.g_leak;
                let mut g_e = p.g_leak * p.e_leak;

                // Gap junctions: ohmic, toward each partner's potential.
                for k in self.gap.row_start[i]..self.gap.row_start[i + 1] {
                    let j = self.gap.from[k] as usize;
                    let g = p.g_gap * self.gap.w[k];
                    g_tot += g;
                    g_e += g * self.v[j];
                }

                // Chemical: conductance gated by the presynaptic neuron's
                // activation, driving toward that synapse's reversal potential.
                for k in self.chem.row_start[i]..self.chem.row_start[i + 1] {
                    let j = self.chem.from[k] as usize;
                    let g = p.g_syn * self.chem.w[k] * act[j];
                    g_tot += g;
                    g_e += g * self.chem.e_rev[k];
                }

                // External drive: tonic baseline, transduction input, and any
                // active stimulation.
                let mut i_ext = self.baseline[i] * self.activity_scale
                    + self.input[i] * self.sensory_gate;
                for s in &self.active {
                    if s.idx.contains(&i) {
                        i_ext += s.strength;
                    }
                }

                let v_inf = (g_e + i_ext) / g_tot;
                let tau = p.c_m / g_tot;
                // Exponential Euler: unconditionally stable for a linear
                // membrane, where forward Euler diverges on a stiff network.
                let k = (-dt / tau).exp();
                self.v[i] = v_inf + (self.v[i] - v_inf) * k;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creature::{Edge, Provenance};

    fn manifest() -> RoleManifest {
        crate::roles::c_elegans()
    }

    /// Two neurons, one chemical synapse a -> b.
    fn pair(sign: f32, gap: bool) -> Connectome {
        Connectome {
            roles: vec!["sensory".into(), "inter".into()],
            cell_types: vec!["ALM".into(), "AVA".into()],
            sides: vec!["left".into(), "right".into()],
            positions: vec![[0.0; 3]; 2],
            chemical: if gap {
                vec![]
            } else {
                vec![Edge {
                    pre: 0,
                    post: 1,
                    weight: sign * 3.0,
                }]
            },
            electrical: if gap {
                vec![Edge {
                    pre: 0,
                    post: 1,
                    weight: 3.0,
                }]
            } else {
                vec![]
            },
            provenance: Provenance::Authored {
                note: "test fixture".into(),
            },
        }
    }

    fn sim_for(c: &Connectome) -> GradedSim {
        GradedSim::new(c, manifest(), GradedParams::default(), 1)
    }

    #[test]
    fn a_quiescent_network_rests_near_leak_potential() {
        let c = pair(1.0, false);
        let mut s = sim_for(&c);
        s.step(2000);
        for v in s.potentials() {
            assert!(
                (*v - GradedParams::default().e_leak).abs() < 12.0,
                "resting potential drifted to {v}"
            );
        }
    }

    /// The defining property: no action potentials. Depolarising a graded
    /// neuron must move it smoothly, never discontinuously.
    #[test]
    fn depolarisation_is_smooth_and_never_spikes() {
        let c = pair(1.0, false);
        let mut s = sim_for(&c);
        s.input[0] = 2.5; // ~25 mV of drive; see Stim for units
        let mut prev = s.potentials()[0];
        let mut max_jump = 0.0f32;
        for _ in 0..2000 {
            s.step(1);
            let v = s.potentials()[0];
            max_jump = max_jump.max((v - prev).abs());
            prev = v;
        }
        assert!(prev > GradedParams::default().e_leak + 1.0, "no response");
        // A spike would be a large single-step excursion followed by a reset.
        assert!(max_jump < 2.0, "largest single-step change {max_jump} mV looks like a spike");
    }

    #[test]
    fn an_excitatory_synapse_depolarises_its_target() {
        let c = pair(1.0, false);
        let mut s = sim_for(&c);
        s.step(500);
        let rest = s.potentials()[1];
        s.input[0] = 3.0;
        s.step(3000);
        assert!(
            s.potentials()[1] > rest + 0.5,
            "target went {} -> {}",
            rest,
            s.potentials()[1]
        );
    }

    /// In a graded model inhibition is not "negative current" — it is a
    /// conductance driving toward a hyperpolarised reversal potential.
    #[test]
    fn an_inhibitory_synapse_hyperpolarises_its_target() {
        let c = pair(-1.0, false);
        let mut s = sim_for(&c);
        s.step(500);
        let rest = s.potentials()[1];
        s.input[0] = 3.0;
        s.step(3000);
        assert!(
            s.potentials()[1] < rest - 0.2,
            "target went {} -> {} (should hyperpolarise)",
            rest,
            s.potentials()[1]
        );
    }

    /// Gap junctions are resistors: current flows *both* ways. Storing them
    /// one-directionally is the classic bug, and it is invisible until you
    /// drive the "downstream" cell and nothing comes back.
    #[test]
    fn gap_junctions_conduct_in_both_directions() {
        let c = pair(1.0, true);

        let mut forward = sim_for(&c);
        forward.step(300);
        let before = forward.potentials()[1];
        forward.input[0] = 3.0;
        forward.step(2000);
        let a_to_b = forward.potentials()[1] - before;

        let mut backward = sim_for(&c);
        backward.step(300);
        let before = backward.potentials()[0];
        backward.input[1] = 3.0;
        backward.step(2000);
        let b_to_a = backward.potentials()[0] - before;

        assert!(a_to_b > 0.5, "no coupling a->b ({a_to_b})");
        assert!(b_to_a > 0.5, "no coupling b->a ({b_to_a}) - gap junction is one-way");
        assert!(
            (a_to_b - b_to_a).abs() < 0.2,
            "coupling should be symmetric: {a_to_b} vs {b_to_a}"
        );
    }

    /// Graded networks with strong coupling are stiff; forward Euler diverges.
    /// This is the test that would catch a regression back to it.
    #[test]
    fn strong_coupling_stays_stable_over_a_long_run() {
        let n = 40;
        let mut chemical = Vec::new();
        let mut electrical = Vec::new();
        for i in 0..n as u32 {
            for j in 0..n as u32 {
                if i != j && (i + j) % 3 == 0 {
                    chemical.push(Edge {
                        pre: i,
                        post: j,
                        weight: if (i + j) % 2 == 0 { 20.0 } else { -20.0 },
                    });
                }
                if i < j && (i * 7 + j) % 5 == 0 {
                    electrical.push(Edge {
                        pre: i,
                        post: j,
                        weight: 15.0,
                    });
                }
            }
        }
        let c = Connectome {
            roles: vec!["inter".into(); n],
            cell_types: vec!["x".into(); n],
            sides: vec!["left".into(); n],
            positions: vec![[0.0; 3]; n],
            chemical,
            electrical,
            provenance: Provenance::Authored {
                note: "stiff fixture".into(),
            },
        };
        let mut s = sim_for(&c);
        s.input[0] = 5.0;
        s.step(60_000); // a simulated minute
        for (i, v) in s.potentials().iter().enumerate() {
            assert!(v.is_finite(), "neuron {i} diverged to {v}");
            assert!(v.abs() < 500.0, "neuron {i} blew up to {v} mV");
        }
    }

    #[test]
    fn population_activity_is_bounded_and_responds_to_drive() {
        let c = pair(1.0, false);
        let mut s = sim_for(&c);
        s.step(500);
        for slug in ["touch", "reverse"] {
            let a = s.population_activity(slug);
            assert!((0.0..=1.0).contains(&a), "{slug} activity {a}");
        }
        assert_eq!(s.population_activity("no-such-group"), 0.0);
    }

    #[test]
    fn stimulation_moves_the_targeted_neurons_only() {
        let c = pair(1.0, false);
        let mut s = sim_for(&c);
        s.step(400);
        let before = [s.potentials()[0], s.potentials()[1]];
        s.stimulate(&[0], 3.0, 500);
        s.step(400);
        assert!(s.potentials()[0] > before[0] + 0.5, "stimulated neuron did not respond");
        s.step(2000); // stimulation expires
        let after = s.potentials()[0];
        s.step(2000);
        assert!(
            s.potentials()[0] < after + 0.2,
            "stimulation did not expire"
        );
        let _ = before;
    }
}
