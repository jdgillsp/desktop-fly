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
use crate::habitat::Region;
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
    /// **No connectome at all**: behaviour is hand-written rules.
    ///
    /// Distinct from [`Provenance::Authored`], which is a connectome someone
    /// wrote by hand, and from [`Provenance::Grown`], which is one a process
    /// generated. This says there are no neurons in the loop whatsoever, which
    /// is a different and stronger claim — and the honest one for an animal
    /// with no published wiring diagram at this scale. The alternative, faking
    /// a connectome so the creature set looks uniform, is the single thing
    /// this project must not do.
    Procedural {
        /// What drives it instead, in one phrase.
        model: String,
        /// Why there is no connectome, and what would change that.
        why: String,
    },
    /// Real circuit modules, recombined into an animal that does not exist
    /// (PORT_PLAN.md §6.2 ii, SPIDER_PLAN.md §2). Every measured neuron and
    /// edge carries the inner provenance; the authored connectives are
    /// counted, and per-neuron/per-edge [`Origin`] says which is which.
    Chimera {
        measured: Box<Provenance>,
        /// What was authored, in one phrase: "pounce connective".
        authored: String,
        /// Behaviours inside this animal that have **no neurons in the loop
        /// at all** — hand-written motor programs, named so the label can say
        /// so (WEB_PLAN.md §3.2). Empty for the salticid; the web builders
        /// list their construction program here. This is the koi's
        /// [`Provenance::Procedural`] claim applied to one behaviour rather
        /// than to a whole creature.
        procedural: Vec<String>,
    },
}

/// Where one neuron or one edge came from. The connectome-level
/// [`Provenance`] says what the measured source is; this says whether a given
/// element is from it at all. A chimera mixes at this granularity, so this is
/// the level its honesty lives at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Measured,
    Authored,
}

impl Origin {
    pub fn from_tag(tag: Option<&str>) -> Origin {
        match tag {
            Some("authored") => Origin::Authored,
            _ => Origin::Measured,
        }
    }
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
            Provenance::Procedural { model, .. } => {
                format!("{model} - PROCEDURAL, no connectome")
            }
            Provenance::Chimera {
                measured,
                authored,
                procedural,
            } => {
                let mut d = format!("chimera: {} modules + authored {authored}", measured.describe());
                if !procedural.is_empty() {
                    d.push_str(&format!("; {}: PROCEDURAL, no neurons", procedural.join(", ")));
                }
                d
            }
        }
    }

    /// The origin of any element this provenance does not itemise: a measured
    /// dataset's elements are measured, everything else's are authored. A
    /// chimera's elements are itemised in the connectome, never defaulted.
    pub fn default_origin(&self) -> Origin {
        match self {
            Provenance::Measured { .. } => Origin::Measured,
            _ => Origin::Authored,
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
    /// Per-neuron and per-chemical-edge origin. May be empty for a
    /// single-source connectome, in which case [`Provenance::default_origin`]
    /// applies to every element; a chimera fills both.
    pub neuron_origin: Vec<Origin>,
    pub edge_origin: Vec<Origin>,
}

impl Connectome {
    pub fn len(&self) -> usize {
        self.roles.len()
    }
    pub fn is_empty(&self) -> bool {
        self.roles.is_empty()
    }

    pub fn origin_of_neuron(&self, i: usize) -> Origin {
        self.neuron_origin
            .get(i)
            .copied()
            .unwrap_or_else(|| self.provenance.default_origin())
    }

    pub fn origin_of_edge(&self, k: usize) -> Origin {
        self.edge_origin
            .get(k)
            .copied()
            .unwrap_or_else(|| self.provenance.default_origin())
    }

    /// How many neurons and chemical edges are authored — the number the
    /// README's measured-vs-modelled table has to print.
    pub fn authored_counts(&self) -> (usize, usize) {
        let n = (0..self.len())
            .filter(|&i| self.origin_of_neuron(i) == Origin::Authored)
            .count();
        let e = (0..self.chemical.len())
            .filter(|&k| self.origin_of_edge(k) == Origin::Authored)
            .count();
        (n, e)
    }

    /// Build from a shipped `circuit.json`. The fly's [`LifSim`] reads the
    /// file directly (it predates this type and is checked against the Swift
    /// oracle, so it stays untouched); every other integrator starts here.
    pub fn from_circuit(file: &crate::data::CircuitFile, provenance: Provenance) -> Self {
        let n = file.neurons.len();
        let edge = |row: &Vec<f32>| -> Option<Edge> {
            if row.len() < 3 {
                return None;
            }
            let (pre, post) = (row[0] as usize, row[1] as usize);
            if pre >= n || post >= n {
                return None;
            }
            Some(Edge {
                pre: pre as u32,
                post: post as u32,
                weight: row[2],
            })
        };
        // Edge rows are `[pre, post, weight]` or `[pre, post, weight, origin]`
        // with origin 1 = authored. Keep the origin list aligned with the
        // edges that survive the range check.
        let edge_origin = |row: &Vec<f32>| -> Option<Origin> {
            edge(row)?;
            Some(if row.get(3).copied().unwrap_or(0.0) >= 0.5 {
                Origin::Authored
            } else {
                Origin::Measured
            })
        };
        let itemised = file.neurons.iter().any(|x| x.origin.is_some())
            || file.edges.iter().any(|e| e.len() >= 4);
        Connectome {
            neuron_origin: if itemised {
                file.neurons
                    .iter()
                    .map(|x| Origin::from_tag(x.origin.as_deref()))
                    .collect()
            } else {
                Vec::new()
            },
            edge_origin: if itemised {
                file.edges.iter().filter_map(edge_origin).collect()
            } else {
                Vec::new()
            },
            roles: file.neurons.iter().map(|x| x.role.clone()).collect(),
            cell_types: file.neurons.iter().map(|x| x.cell_type.clone()).collect(),
            sides: file.neurons.iter().map(|x| x.side.clone()).collect(),
            positions: file
                .neurons
                .iter()
                .map(|x| {
                    let p = &x.pos;
                    [
                        p.first().copied().unwrap_or(0.0),
                        p.get(1).copied().unwrap_or(0.0),
                        p.get(2).copied().unwrap_or(0.0),
                    ]
                })
                .collect(),
            chemical: file.edges.iter().filter_map(edge).collect(),
            electrical: file.electrical.iter().filter_map(edge).collect(),
            provenance,
        }
    }
}

/// Every creature the engine can run, in menu order. The fly is first because
/// it is the one with shipped data.
pub const CREATURE_IDS: [&str; 7] = [
    "drosophila",
    "salticid",
    "c_elegans",
    "koi",
    "araneus",
    "parasteatoda",
    "agelenopsis",
];

/// Look a creature up by its `id()`. `None` for an id that is not a creature,
/// so a stale settings file or a typo on the command line degrades to the
/// caller's default rather than a panic.
pub fn by_id(id: &str) -> Option<Box<dyn Creature>> {
    match id {
        "drosophila" => Some(Box::new(Drosophila)),
        "salticid" => Some(Box::new(Salticid)),
        "c_elegans" => Some(Box::new(CElegans)),
        "koi" => Some(Box::new(Koi)),
        "araneus" => Some(Box::new(Weaver::Araneus)),
        "parasteatoda" => Some(Box::new(Weaver::Parasteatoda)),
        "agelenopsis" => Some(Box::new(Weaver::Agelenopsis)),
        _ => None,
    }
}

/// Creature #3: a jumping spider that does not exist (SPIDER_PLAN.md).
///
/// **No spider connectome exists**, so this is a chimera: the fly's measured
/// looming, escape, steering, walking, grooming and backing-up circuits, plus
/// FlyWire's LC11 small-object detectors for prey, all real — joined by one
/// authored connective, the pounce node, which no animal has. The integrator
/// is the fly's LIF because the neurons are the fly's; the body is a spider's
/// because the silhouette is ours to choose. The rules in SPIDER_PLAN.md §2
/// are enforced by `chimera_labels_are_honest` below: this is never called a
/// spider brain.
pub struct Salticid;

impl Creature for Salticid {
    fn id(&self) -> &'static str {
        "salticid"
    }
    fn display_name(&self) -> &'static str {
        "Jumping spider (chimera)"
    }
    fn manifest(&self) -> RoleManifest {
        crate::roles::salticid()
    }
    fn dynamics(&self) -> DynamicsSpec {
        DynamicsSpec::Lif(LifParams::default())
    }
    fn provenance(&self) -> Provenance {
        Provenance::Chimera {
            measured: Box::new(Provenance::flywire_v783()),
            authored: "pounce connective".into(),
            procedural: Vec::new(),
        }
    }
    fn data_dir(&self) -> &'static str {
        "salticid"
    }
}

/// Creature #4: a koi, driven by rules rather than by neurons.
///
/// **There is no connectome for this animal.** No fish has a published
/// synapse-resolution whole-brain wiring diagram at the scale the fly's
/// FlyWire extract provides, so there is nothing real to run — and a
/// plausible-looking invented one would be worse than none, because the whole
/// claim of this app is that the brain data is real. So the koi is explicitly
/// [`Provenance::Procedural`]: a hand-written behaviour model, labelled as such
/// in the tray, the console and the brain window, which stays closed because
/// there is no brain to show.
///
/// The nearest honest future path is **zebrafish** (*Danio rerio*): larval
/// whole-brain activity imaging exists, and EM connectomics is progressing, but
/// a synapse-resolution whole-brain connectome does not exist yet. If one
/// lands, this creature can become measured without the body or the behaviour
/// changing — that is what the `Creature` seam is for. See KOI_PLAN.md.
///
/// There is also a nice piece of symmetry worth noting and *not* over-claiming:
/// a fish's fast escape is driven by the Mauthner cell, a single giant
/// reticulospinal neuron that is the functional counterpart of the fly's giant
/// fiber. The C-start this creature performs is modelled on that behaviour. It
/// is not simulating a Mauthner cell; it is imitating what one produces.
pub struct Koi;

impl Creature for Koi {
    fn id(&self) -> &'static str {
        "koi"
    }
    fn display_name(&self) -> &'static str {
        "Koi (procedural)"
    }
    /// An empty manifest: no populations, because there are no neurons. The
    /// brain window and the readout both resolve to nothing, which is correct.
    fn manifest(&self) -> RoleManifest {
        crate::roles::procedural()
    }
    /// Nominally LIF, but nothing ever constructs a simulation for this
    /// creature — `data_dir` has no data and `KoiRuntime` never asks for one.
    fn dynamics(&self) -> DynamicsSpec {
        DynamicsSpec::Lif(LifParams::default())
    }
    fn provenance(&self) -> Provenance {
        Provenance::Procedural {
            model: "koi behaviour model".into(),
            why: "no fish connectome exists at this scale; zebrafish is the \
                  nearest future path (see KOI_PLAN.md)"
                .into(),
        }
    }
    fn data_dir(&self) -> &'static str {
        "koi"
    }
}

/// Creatures #5–#7: the web-building chimeras (WEB_PLAN.md).
///
/// Three species, one circuit. An orb weaver, a gumfoot-tangle weaver and a
/// sheet-and-funnel weaver differ in body and in the construction program
/// they run — not in wiring, because there is no wiring to differ in: **no
/// spider connectome exists** (verified 2026-09-05, SPIDER_PLAN.md §8.6). So
/// all three run the same labelled chimera, `data/weaver/`: the fly's
/// measured motor and escape modules plus its measured mechanosensory
/// partners, with the salticid's LC11 dropped (these animals hunt by web
/// vibration, not by sight — a visual prey pathway would be real data in a
/// false place) and **one authored node**, the strike, that integrates
/// sustained small vibration on a slow membrane. The web-building program
/// itself has no neurons in it at all, and `describe()` says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weaver {
    /// *Araneus diadematus*, the garden cross spider: an orb, rebuilt daily.
    Araneus,
    /// *Parasteatoda tepidariorum*, the common house spider: a gumfoot tangle
    /// with a central retreat, added to over nights.
    Parasteatoda,
    /// *Agelenopsis* sp., a grass spider: a non-sticky sheet with a funnel,
    /// thickened by every crossing.
    Agelenopsis,
}

impl Weaver {
    pub const ALL: [Weaver; 3] = [Weaver::Araneus, Weaver::Parasteatoda, Weaver::Agelenopsis];

    /// The Latin binomial, for the console and the README.
    pub fn species(&self) -> &'static str {
        match self {
            Weaver::Araneus => "Araneus diadematus",
            Weaver::Parasteatoda => "Parasteatoda tepidariorum",
            Weaver::Agelenopsis => "Agelenopsis sp.",
        }
    }

    /// Which web family's construction program this species runs.
    pub fn web(&self) -> &'static str {
        match self {
            Weaver::Araneus => "orb web",
            Weaver::Parasteatoda => "gumfoot tangle web",
            Weaver::Agelenopsis => "sheet web with funnel",
        }
    }
}

impl Creature for Weaver {
    fn id(&self) -> &'static str {
        match self {
            Weaver::Araneus => "araneus",
            Weaver::Parasteatoda => "parasteatoda",
            Weaver::Agelenopsis => "agelenopsis",
        }
    }
    fn display_name(&self) -> &'static str {
        match self {
            Weaver::Araneus => "Garden cross spider (chimera)",
            Weaver::Parasteatoda => "House spider (chimera)",
            Weaver::Agelenopsis => "Grass spider (chimera)",
        }
    }
    fn manifest(&self) -> RoleManifest {
        crate::roles::weaver()
    }
    fn dynamics(&self) -> DynamicsSpec {
        DynamicsSpec::Lif(LifParams::default())
    }
    fn provenance(&self) -> Provenance {
        Provenance::Chimera {
            measured: Box::new(Provenance::flywire_v783()),
            authored: "strike node".into(),
            procedural: vec![format!("{} construction program", self.web())],
        }
    }
    fn data_dir(&self) -> &'static str {
        "weaver"
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
    /// Where neuron `i` came from, so the brain window can colour by
    /// provenance. Measured for everything in a measured dataset.
    fn origin(&self, i: usize) -> Origin;
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
    fn origin(&self, i: usize) -> Origin {
        self.origins.get(i).copied().unwrap_or(Origin::Measured)
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
    fn origin(&self, i: usize) -> Origin {
        self.origins.get(i).copied().unwrap_or(Origin::Measured)
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
    /// Walks on the desktop and on window ledges, and jumps between them
    /// ballistically on a dragline. No flight.
    WalkerJumper,
    /// Swims. Ignores window ledges entirely — the desktop is water, not
    /// terrain — and has no gait, no altitude and nothing to stand on.
    Swimmer,
    /// Walks on the desktop, on window ledges and on its own silk, and
    /// builds a web between whatever it can anchor to. No flight, no
    /// ballistic jump; a drop on the dragline instead.
    WalkerWeaver,
}

/// The world a body moves through, as the body sees it.
#[derive(Debug, Clone)]
pub struct World {
    /// Where the world ends. In free roam this is the whole display, centred on
    /// the scene origin — which is what every body used to assume outright. In
    /// habitat mode it is the enclosure, which can sit anywhere.
    pub region: Region,
    pub ledges: Vec<Ledge>,
    pub cursor: Option<Vec2>,
    /// Something in the enclosure worth going to look at, if anything is. The
    /// bodies never learn what it is — a flake, a ball — only that it is there,
    /// which keeps prop logic out of four separate animals.
    pub attractor: Option<Vec2>,
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
        self.attractor = world.attractor;
        self.update(dt, world.region, world.cursor, Some(*drives));
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
            neuron_origin: Vec::new(),
            edge_origin: Vec::new(),
        };
        assert_eq!(c.len(), 2);
        assert_eq!(c.electrical.len(), 1);
        // An authored fixture's elements are authored, without itemising.
        assert_eq!(c.origin_of_neuron(0), Origin::Authored);
        assert_eq!(c.authored_counts(), (2, 1));
    }

    /// SPIDER_PLAN.md §2 rule 4, as a test: the chimera is never presented as
    /// a spider's brain, and it never passes as measured.
    #[test]
    fn chimera_labels_are_honest() {
        let s = Salticid;
        let p = s.provenance();
        assert!(!p.is_measured(), "a chimera must never pass as measured");
        let d = p.describe().to_lowercase();
        assert!(d.contains("chimera"), "{d}");
        assert!(d.contains("flywire"), "the measured source must be named: {d}");
        assert!(d.contains("authored"), "the invented part must be named: {d}");
        for forbidden in ["spider connectome", "spider brain"] {
            assert!(!d.contains(forbidden), "'{forbidden}' in {d}");
        }
        assert!(!s.display_name().to_lowercase().contains("connectome"));
        assert!(s.display_name().to_lowercase().contains("chimera"));
        assert_eq!(p.default_origin(), Origin::Authored, "un-itemised elements of a chimera are not measured");
    }

    /// The web builders' version: the same chimera rules, plus the claim that
    /// their construction program has no neurons in it must be in the line a
    /// user sees, and LC11 must be gone from their manifest.
    #[test]
    fn weaver_labels_are_honest_and_lc11_is_gone() {
        for w in Weaver::ALL {
            let p = w.provenance();
            assert!(!p.is_measured());
            let d = p.describe();
            let lower = d.to_lowercase();
            assert!(lower.contains("chimera"), "{d}");
            assert!(lower.contains("flywire"), "{d}");
            assert!(lower.contains("authored strike"), "{d}");
            assert!(d.contains("PROCEDURAL"), "the program's label must be unmissable: {d}");
            assert!(lower.contains("construction program"), "{d}");
            for forbidden in ["spider connectome", "spider brain"] {
                assert!(!lower.contains(forbidden), "'{forbidden}' in {d}");
            }
            assert!(w.display_name().to_lowercase().contains("chimera"));
            assert!(!w.display_name().to_lowercase().contains("connectome"));
            let m = w.manifest();
            assert!(m.find("lc11").is_none(), "a blind hunter has no visual prey pathway");
            assert!(m.find("escw").is_none(), "no wings");
            assert!(m.find("strike").is_some());
            assert_eq!(w.data_dir(), "weaver", "one shared circuit, not three copies");
        }
        assert_eq!(Weaver::Araneus.web(), "orb web");
    }

    /// The koi's version of the same rule. It is the strongest claim in the
    /// set — *no neurons at all* — so it has the most to get wrong: it must
    /// not read as measured, must not borrow another animal's dataset, must
    /// say "procedural" in the line a user actually sees, and must carry the
    /// reason there is no connectome rather than leaving it to a commit
    /// message nobody will read.
    #[test]
    fn procedural_labels_are_honest() {
        let k = Koi;
        let p = k.provenance();
        assert!(!p.is_measured(), "a procedural creature must never pass as measured");
        let d = p.describe();
        assert!(d.contains("PROCEDURAL"), "the label must be unmissable: {d}");
        let lower = d.to_lowercase();
        assert!(lower.contains("no connectome"), "{d}");
        for forbidden in ["flywire", "measured", "chimera", "connectome-derived"] {
            assert!(!lower.contains(forbidden), "'{forbidden}' in {d}");
        }
        // The display name a user picks from must carry it too.
        assert!(k.display_name().to_lowercase().contains("procedural"));
        assert!(!k.display_name().to_lowercase().contains("connectome"));

        match p {
            Provenance::Procedural { why, .. } => {
                assert!(
                    why.to_lowercase().contains("zebrafish"),
                    "the honest future path belongs in the label: {why}"
                );
            }
            _ => panic!("the koi must be Procedural"),
        }
        assert_eq!(k.provenance().default_origin(), Origin::Authored);
    }

    /// A creature with no neurons must resolve to an empty manifest rather
    /// than to a plausible-looking table of invented populations.
    #[test]
    fn the_procedural_manifest_names_no_populations() {
        let m = Koi.manifest();
        assert!(m.populations.is_empty(), "invented anatomy");
        assert_eq!(m.label_for("anything"), "circuit partners");
    }

    /// A chimera file itemises origins per neuron and per edge, and the counts
    /// come out of the data rather than a constant somebody has to remember.
    #[test]
    fn a_chimera_circuit_file_itemises_its_authored_parts() {
        use crate::data::{CircuitFile, CircuitNeuron};
        let neuron = |role: &str, origin: Option<&str>| CircuitNeuron {
            id: role.into(),
            cell_type: role.to_uppercase(),
            role: role.into(),
            side: "center".into(),
            pos: vec![0.0, 0.0, 0.0],
            origin: origin.map(|s| s.to_string()),
        };
        let file = CircuitFile {
            neurons: vec![
                neuron("lc11", Some("measured")),
                neuron("lc11", Some("measured")),
                neuron("pounce", Some("authored")),
            ],
            edges: vec![
                vec![0.0, 1.0, 3.0, 0.0],
                vec![0.0, 2.0, 8.0, 1.0],
                vec![1.0, 2.0, 8.0, 1.0],
            ],
            electrical: vec![],
        };
        let c = Connectome::from_circuit(&file, Salticid.provenance());
        assert_eq!(c.origin_of_neuron(2), Origin::Authored);
        assert_eq!(c.origin_of_neuron(0), Origin::Measured);
        assert_eq!(c.origin_of_edge(0), Origin::Measured);
        assert_eq!(c.origin_of_edge(1), Origin::Authored);
        assert_eq!(c.authored_counts(), (1, 2));
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

    /// The picker's contract: every listed id resolves, and resolves to a
    /// creature that reports the same id back — otherwise a persisted choice
    /// could silently load a different animal.
    #[test]
    fn every_listed_creature_id_round_trips() {
        for id in CREATURE_IDS {
            let c = by_id(id).unwrap_or_else(|| panic!("{id} is listed but not constructible"));
            assert_eq!(c.id(), id);
        }
        assert!(by_id("honeybee").is_none(), "no fabricated species (PORT_PLAN.md §6.2 rule 4)");
    }

    /// A circuit file becomes a connectome with both edge lists, and malformed
    /// rows are dropped rather than indexing out of range in the integrator.
    #[test]
    fn a_circuit_file_becomes_a_connectome_with_both_edge_lists() {
        use crate::data::{CircuitFile, CircuitNeuron};
        let neuron = |role: &str, side: &str| CircuitNeuron {
            id: role.into(),
            cell_type: role.to_uppercase(),
            role: role.into(),
            side: side.into(),
            pos: vec![1.0, 2.0, 3.0],
            origin: None,
        };
        let file = CircuitFile {
            neurons: vec![neuron("touch", "left"), neuron("forward", "right")],
            edges: vec![vec![0.0, 1.0, -4.0], vec![0.0, 9.0, 1.0], vec![1.0]],
            electrical: vec![vec![0.0, 1.0, 2.0]],
        };
        let c = Connectome::from_circuit(&file, Provenance::Authored { note: "fixture".into() });
        assert_eq!(c.len(), 2);
        assert_eq!(c.roles, vec!["touch", "forward"]);
        assert_eq!(c.cell_types[0], "TOUCH");
        assert_eq!(c.positions[1], [1.0, 2.0, 3.0]);
        assert_eq!(c.chemical.len(), 1, "out-of-range and short rows are dropped");
        assert_eq!(c.chemical[0].weight, -4.0);
        assert_eq!(c.electrical.len(), 1);
    }
}
