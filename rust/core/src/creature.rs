//! The creature abstraction (PORT_PLAN.md §5).
//!
//! The engine is no longer hardcoded to *Drosophila*. A creature is four things
//! that vary together, and one seam each:
//!
//! | piece | trait | why it varies |
//! |---|---|---|
//! | wiring | [`Connectome`] + [`Provenance`] | different animal, different data and licence |
//! | dynamics | [`Sim`] / [`DynamicsSpec`] | **a fly spikes; a worm does not** |
//! | senses | `Transduction` (in the shell) | a fly sees looming; a worm is blind |
//! | body | [`Body`] | six legs and wings, or a segmented crawl |
//!
//! The dynamics split is the one that matters most and is easiest to get wrong.
//! *C. elegans* neurons are predominantly **non-spiking** — signalling is by
//! graded potential, and its electrical (gap-junction) connectome is a separate
//! layer of comparable importance to the chemical one. Running worm data
//! through the fly's LIF loop would manufacture action potentials the animal
//! does not have, and quietly turn "the brain data is real" into a lie. So
//! [`DynamicsSpec`] is an enum, and [`Connectome`] carries two edge lists.

use crate::lif::{LifParams, LifSim};
use crate::roles::RoleManifest;
use crate::signals::BrainSignals;
use crate::util::{Ledge, Vec2};

/// Where a connectome came from, and under what terms.
///
/// Put here rather than in a README because the moment a second dataset arrives
/// the licence stops being a promise and becomes a field the code must respect
/// (the FlyWire data is CC BY-NC 4.0 while the code is MIT). It also does
/// double duty for §6.2: a synthetic or chimeric creature is honest exactly
/// when this says so.
#[derive(Debug, Clone, PartialEq)]
pub enum Provenance {
    /// Real, published, synapse-resolution data.
    Measured {
        source: String,
        version: String,
        citation: String,
        license: String,
    },
    /// Grown from a seed by a documented generative process (§6.2 iii).
    Grown {
        generator: String,
        seed: u64,
        matched: Vec<String>,
    },
    /// Hand-authored. Test fixtures, and nothing that ships as a creature.
    Authored { note: String },
}

impl Provenance {
    pub fn is_measured(&self) -> bool {
        matches!(self, Provenance::Measured { .. })
    }

    /// One line for the UI, so a user can always tell what they are looking at.
    pub fn describe(&self) -> String {
        match self {
            Provenance::Measured {
                source, version, ..
            } => format!("{source} {version} (measured)"),
            Provenance::Grown {
                generator, seed, ..
            } => format!("{generator}, seed 0x{seed:X} (synthetic)"),
            Provenance::Authored { note } => format!("{note} (authored)"),
        }
    }

    pub fn flywire_v783() -> Self {
        Provenance::Measured {
            source: "FlyWire FAFB".into(),
            version: "v783".into(),
            citation: "Dorkenwald et al., Nature 634:124-138 (2024); \
                       Schlegel et al., Nature 634:139-152 (2024)"
                .into(),
            license: "CC BY-NC 4.0".into(),
        }
    }
}

/// A signed chemical synapse, or an unsigned electrical one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    pub pre: u32,
    pub post: u32,
    /// Chemical: synapse count signed by neurotransmitter. Electrical:
    /// conductance, always positive and symmetric.
    pub weight: f32,
}

/// A creature's wiring, independent of how it is simulated.
#[derive(Debug, Clone)]
pub struct Connectome {
    pub roles: Vec<String>,
    pub cell_types: Vec<String>,
    pub sides: Vec<String>,
    pub positions: Vec<[f32; 3]>,
    pub chemical: Vec<Edge>,
    /// Gap junctions. Empty for the fly's FlyWire extract; **load-bearing** for
    /// *C. elegans*, where electrical coupling is roughly a third of the graph.
    pub electrical: Vec<Edge>,
    pub provenance: Provenance,
}

impl Connectome {
    pub fn len(&self) -> usize {
        self.roles.len()
    }
    pub fn is_empty(&self) -> bool {
        self.roles.is_empty()
    }
}

/// Which integrator a creature needs.
#[derive(Debug, Clone)]
pub enum DynamicsSpec {
    /// Spiking. *Drosophila*: threshold, refractory period, delayed inhibition.
    Lif(LifParams),
    /// Graded / non-spiking with explicit electrical coupling. *C. elegans*.
    /// Implemented by [`crate::graded::GradedSim`].
    Graded(GradedParams),
}

/// Parameters of a graded, non-spiking membrane model — the standard honest
/// formulation for *C. elegans* (a linear RC membrane with sigmoidal synaptic
/// activation and ohmic gap junctions).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradedParams {
    /// Membrane capacitance.
    pub c_m: f32,
    /// Leak conductance and reversal potential.
    pub g_leak: f32,
    pub e_leak: f32,
    /// Peak chemical synaptic conductance per unit weight.
    pub g_syn: f32,
    /// Gap-junction conductance per unit weight.
    pub g_gap: f32,
    /// Half-activation voltage and slope of the synaptic sigmoid.
    pub v_eq: f32,
    pub beta: f32,
    /// Reversal potentials for excitatory and inhibitory transmitters.
    pub e_exc: f32,
    pub e_inh: f32,
}

impl Default for GradedParams {
    fn default() -> Self {
        GradedParams {
            // Membrane time constant is c_m / g_leak = 15 ms. Getting this
            // wrong by three orders of magnitude is easy and silent: with
            // c_m = 1.0 the membrane takes ten *seconds* to respond and the
            // creature simply never does anything.
            c_m: 0.0015,
            g_leak: 0.1,
            e_leak: -35.0,
            // Synaptic conductances are small relative to leak, so a single
            // synapse nudges rather than clamps — the graded analogue of the
            // fly's 0.0008 weight scale.
            g_syn: 0.02,
            g_gap: 0.04,
            // v_eq must sit ABOVE rest. Setting it equal to e_leak leaves every
            // neuron 50% activated while quiescent, so the whole network is
            // permanently half-driven and nothing reads as a response.
            v_eq: -25.0,
            beta: 0.30,
            e_exc: 0.0,
            e_inh: -45.0,
        }
    }
}

/// What every integrator must offer the rest of the engine.
///
/// Deliberately narrow. The shell drives senses in and reads population
/// activity out; it never needs to know whether the numbers underneath are
/// spikes or membrane potentials.
pub trait Sim {
    fn n(&self) -> usize;
    fn step(&mut self, ms: i64);
    fn stimulate(&mut self, indices: &[usize], strength: f32, duration_ms: i64);
    /// Per-neuron activity in 0..1, for the brain window. Spike flashes for a
    /// spiking model; normalised depolarisation for a graded one.
    fn activity(&self) -> Vec<f32>;
    fn manifest(&self) -> &RoleManifest;
    fn positions(&self) -> &[[f32; 3]];
    fn roles(&self) -> &[String];
    /// Members of a named population, for stimulation and readout.
    fn group(&self, slug: &str) -> &[usize];
}

impl Sim for LifSim {
    fn n(&self) -> usize {
        self.n
    }
    fn step(&mut self, ms: i64) {
        LifSim::step(self, ms)
    }
    fn stimulate(&mut self, indices: &[usize], strength: f32, duration_ms: i64) {
        LifSim::stimulate(self, indices, strength, duration_ms)
    }
    /// A spike is a third of full scale; a giant-fiber spike is full scale.
    /// The GF firing *is* the maximal event in this animal — it is the escape
    /// command the whole app is built around — so it earns the top of the range
    /// and reads as a distinct flash rather than one bright dot among many.
    fn activity(&self) -> Vec<f32> {
        let mut a = vec![0.0; self.n];
        for ev in &self.last_spikes {
            if ev.neuron < a.len() {
                a[ev.neuron] = if ev.is_gf { 1.0 } else { 1.0 / 3.0 };
            }
        }
        a
    }
    fn manifest(&self) -> &RoleManifest {
        &self.manifest
    }
    fn positions(&self) -> &[[f32; 3]] {
        &self.positions
    }
    fn roles(&self) -> &[String] {
        &self.roles
    }
    fn group(&self, slug: &str) -> &[usize] {
        self.groups.get(slug).map(|g| g.as_slice()).unwrap_or(&[])
    }
}

impl Sim for crate::graded::GradedSim {
    fn n(&self) -> usize {
        self.n
    }
    fn step(&mut self, ms: i64) {
        crate::graded::GradedSim::step(self, ms)
    }
    fn stimulate(&mut self, indices: &[usize], strength: f32, duration_ms: i64) {
        crate::graded::GradedSim::stimulate(self, indices, strength, duration_ms)
    }
    /// Normalised depolarisation above rest. There are no spikes to flash, so
    /// the brain window shows *how depolarised* each neuron is — which is what
    /// "active" means for this animal.
    fn activity(&self) -> Vec<f32> {
        self.potentials()
            .iter()
            .map(|v| crate::util::clamp((v + 35.0) / 25.0, 0.0, 1.0))
            .collect()
    }
    fn manifest(&self) -> &RoleManifest {
        &self.manifest
    }
    fn positions(&self) -> &[[f32; 3]] {
        &self.positions
    }
    fn roles(&self) -> &[String] {
        &self.roles
    }
    fn group(&self, slug: &str) -> &[usize] {
        crate::graded::GradedSim::group(self, slug)
    }
}

/// Creature #2: *C. elegans*, the only complete cell-identified whole-animal
/// connectome — and the reason the dynamics seam exists.
///
/// **Data status:** the connectome file is not shipped. PORT_PLAN.md §8 flags
/// the redistribution terms as unverified, and shipping a dataset whose licence
/// has not been checked is exactly the kind of thing `Provenance` exists to
/// prevent. `etl/etl_celegans.py` documents how to fetch and convert it.
pub struct CElegans;

impl Creature for CElegans {
    fn id(&self) -> &'static str {
        "c_elegans"
    }
    fn display_name(&self) -> &'static str {
        "Roundworm"
    }
    fn manifest(&self) -> RoleManifest {
        crate::roles::c_elegans()
    }
    fn dynamics(&self) -> DynamicsSpec {
        DynamicsSpec::Graded(GradedParams::default())
    }
    fn provenance(&self) -> Provenance {
        Provenance::Measured {
            source: "C. elegans hermaphrodite connectome".into(),
            version: "White 1986 / Cook 2019".into(),
            citation: "White et al., Phil. Trans. R. Soc. B 314:1-340 (1986);                        Cook et al., Nature 571:63-71 (2019)"
                .into(),
            license: "UNVERIFIED - see PORT_PLAN.md sec 8".into(),
        }
    }
    fn data_dir(&self) -> &'static str {
        "c_elegans"
    }
}

/// How a creature meets the desktop. A fly walks and flies; a worm crawls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Substrate {
    /// Walks on the desktop and on window ledges, and can fly between them.
    WalkerFlier,
    /// Crawls, tracking surfaces. No flight — so no altitude, no wing beat.
    Crawler,
}

/// The world a body moves through, as the body sees it.
#[derive(Debug, Clone)]
pub struct World {
    pub bounds: (f32, f32),
    pub ledges: Vec<Ledge>,
    pub cursor: Option<Vec2>,
}

/// What the body reports back — including, crucially, proprioception.
///
/// The fly already closes the loop (gait phase drives its real ascending
/// neurons). For a worm this is *load-bearing*: undulation is generated by
/// proprioceptive coupling along the body, so the body is not a display of the
/// brain's output, it is part of the circuit.
#[derive(Debug, Clone, Copy, Default)]
pub struct Proprioception {
    /// Locomotor intensity, 0..1.
    pub drive: f32,
    /// Phase of the locomotor rhythm, 0..1.
    pub phase: f32,
}

/// A creature's body: geometry and locomotion, with no rendering in sight.
pub trait Body {
    fn substrate(&self) -> Substrate;
    fn step(&mut self, dt: f32, drives: &BrainSignals, world: &World);
    fn position(&self) -> Vec2;
    fn heading(&self) -> f32;
    fn proprioception(&self) -> Proprioception;
}

impl Body for crate::body::Fly {
    fn substrate(&self) -> Substrate {
        Substrate::WalkerFlier
    }
    fn step(&mut self, dt: f32, drives: &BrainSignals, world: &World) {
        self.terrain = world.ledges.clone();
        self.update(dt, world.bounds, world.cursor, Some(*drives));
    }
    fn position(&self) -> Vec2 {
        self.pos
    }
    fn heading(&self) -> f32 {
        self.heading
    }
    fn proprioception(&self) -> Proprioception {
        Proprioception {
            drive: self.walking_intensity(),
            phase: self.gait_phase,
        }
    }
}

/// A species the engine can run. One is chosen at startup.
pub trait Creature {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn manifest(&self) -> RoleManifest;
    fn dynamics(&self) -> DynamicsSpec;
    fn provenance(&self) -> Provenance;
    /// Where this creature's data lives, relative to `data/`.
    fn data_dir(&self) -> &'static str;
}

/// Creature #1: the fruit fly, exactly as the Swift build runs it.
pub struct Drosophila;

impl Creature for Drosophila {
    fn id(&self) -> &'static str {
        "drosophila"
    }
    fn display_name(&self) -> &'static str {
        "Fruit fly"
    }
    fn manifest(&self) -> RoleManifest {
        crate::roles::drosophila()
    }
    fn dynamics(&self) -> DynamicsSpec {
        DynamicsSpec::Lif(LifParams::default())
    }
    fn provenance(&self) -> Provenance {
        Provenance::flywire_v783()
    }
    fn data_dir(&self) -> &'static str {
        "."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fly_is_a_measured_spiking_creature() {
        let d = Drosophila;
        assert_eq!(d.id(), "drosophila");
        assert!(matches!(d.dynamics(), DynamicsSpec::Lif(_)));
        assert!(d.provenance().is_measured());
        assert!(d.provenance().describe().contains("FlyWire"));
    }

    /// The licence split is a FlyWire term, not a preference, so it must be
    /// carried in the data rather than only in a README.
    #[test]
    fn flywire_provenance_carries_its_licence_and_citation() {
        match Provenance::flywire_v783() {
            Provenance::Measured {
                license, citation, ..
            } => {
                assert!(license.contains("CC BY-NC"));
                assert!(citation.contains("Dorkenwald"));
            }
            _ => panic!("FlyWire data must be Measured"),
        }
    }

    /// A synthetic creature must never be able to pass as measured — that is
    /// the whole honesty mechanism in §6.2.
    #[test]
    fn synthetic_provenance_is_never_measured_and_says_so() {
        let g = Provenance::Grown {
            generator: "growth-v1".into(),
            seed: 0xC0FFEE,
            matched: vec!["degree_dist:flywire-v783".into()],
        };
        assert!(!g.is_measured());
        assert!(g.describe().contains("synthetic"));
        assert!(g.describe().contains("C0FFEE"));

        let a = Provenance::Authored {
            note: "test fixture".into(),
        };
        assert!(!a.is_measured());
        assert!(a.describe().contains("authored"));
    }

    /// The seam that matters: a worm needs a different integrator, and the
    /// type system should make that a variant rather than a rewrite.
    #[test]
    fn the_dynamics_seam_admits_a_non_spiking_model() {
        let graded = DynamicsSpec::Graded(GradedParams::default());
        assert!(matches!(graded, DynamicsSpec::Graded(_)));
        // Graded models rest below their synaptic half-activation, so a
        // quiescent network sits near leak potential rather than at threshold.
        let p = GradedParams::default();
        assert!(p.e_inh < p.e_leak, "inhibition must hyperpolarise");
        assert!(p.e_exc > p.e_leak, "excitation must depolarise");
    }

    #[test]
    fn a_connectome_can_carry_gap_junctions_even_when_the_fly_has_none() {
        let c = Connectome {
            roles: vec!["a".into(), "b".into()],
            cell_types: vec!["x".into(), "y".into()],
            sides: vec!["left".into(), "right".into()],
            positions: vec![[0.0; 3]; 2],
            chemical: vec![Edge {
                pre: 0,
                post: 1,
                weight: 3.0,
            }],
            electrical: vec![Edge {
                pre: 0,
                post: 1,
                weight: 1.0,
            }],
            provenance: Provenance::Authored {
                note: "fixture".into(),
            },
        };
        assert_eq!(c.len(), 2);
        assert_eq!(c.electrical.len(), 1);
    }

    #[test]
    fn the_worm_is_a_graded_creature_with_its_own_manifest() {
        let w = CElegans;
        assert_eq!(w.id(), "c_elegans");
        assert!(matches!(w.dynamics(), DynamicsSpec::Graded(_)));
        assert_eq!(w.manifest().creature, "c_elegans");
        // The fly and the worm must not share an integrator.
        assert!(matches!(Drosophila.dynamics(), DynamicsSpec::Lif(_)));
    }

    /// The worm's licence is genuinely unverified, and the code should say so
    /// rather than quietly implying the data is cleared for redistribution.
    #[test]
    fn the_worm_data_licence_is_flagged_as_unverified() {
        match CElegans.provenance() {
            Provenance::Measured { license, .. } => {
                assert!(
                    license.contains("UNVERIFIED"),
                    "licence must not claim clearance it does not have: {license}"
                );
            }
            _ => panic!("worm data is measured, not synthetic"),
        }
    }

    #[test]
    fn a_crawler_and_a_flier_are_distinguishable_substrates() {
        assert_ne!(Substrate::Crawler, Substrate::WalkerFlier);
    }
}
