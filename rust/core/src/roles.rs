//! The role manifest — what makes a creature's neuron populations *data*
//! rather than string literals scattered through a `match`.
//!
//! In the Swift build, role slugs are hardcoded in at least six places: the
//! group-assignment switch, the baseline switch and the spike-counting switch
//! (all `Sim.swift`), `SignalBuilder`, `brainBehavior`, and the brain view's
//! colour and label tables. `CLAUDE.md`'s own "adding a new neuron population"
//! recipe is **eight steps across five files** — that is the abstraction asking
//! to be written (PORT_PLAN.md §5).
//!
//! Everything that is *per-population data* now lives in one table here:
//! membership, laterality, resting excitability, display colour and label.
//! What stays in code is the part that genuinely is behaviour — how a rate
//! becomes a command, and what the body does about it.

use std::collections::HashMap;

/// A role slug, e.g. `"gf"` or `"dnp09"`.
pub type RoleId = &'static str;

/// Where a population's members come from in the shipped data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Membership {
    /// Neurons whose `role` field equals this slug.
    Role(RoleId),
    /// Partners, selected by their FlyWire `super_class` in the `type` field.
    /// The fly's proprioceptive and wind inputs arrive this way.
    PartnerClass(&'static str),
}

#[derive(Debug, Clone)]
pub struct Population {
    pub slug: RoleId,
    /// Shown in the brain window when this region is clicked.
    pub label: &'static str,
    pub membership: Membership,
    /// Split into left/right groups? Bilateral command pairs are.
    pub bilateral: bool,
    /// Resting drive. Command DNs get a *deterministic* value on purpose:
    /// their left/right asymmetry must come from the wiring, never from luck.
    pub baseline: Baseline,
    /// Colour in the brain window.
    pub color: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Baseline {
    Fixed(f32),
    /// Randomised per neuron within a range — interneurons only.
    Range(f32, f32),
}

/// The full per-creature manifest.
#[derive(Debug, Clone)]
pub struct RoleManifest {
    pub creature: &'static str,
    pub populations: Vec<Population>,
    /// Baseline for any neuron not matched by a population above.
    pub default_baseline: Baseline,
    pub default_color: [f32; 3],
}

impl RoleManifest {
    pub fn find(&self, slug: &str) -> Option<&Population> {
        self.populations.iter().find(|p| p.slug == slug)
    }

    /// Colour for a neuron with this role, for the brain window.
    pub fn color_for(&self, role: &str) -> [f32; 4] {
        let c = self
            .find(role)
            .map(|p| p.color)
            .unwrap_or(self.default_color);
        [c[0], c[1], c[2], 1.0]
    }

    /// Human-readable name, for click-to-stimulate labels.
    pub fn label_for(&self, role: &str) -> &str {
        self.find(role).map(|p| p.label).unwrap_or("circuit partners")
    }

    /// Resolve every population's member indices from the loaded connectome.
    ///
    /// Returns `slug -> (left_or_all, right)`; for non-bilateral populations the
    /// right list is empty.
    pub fn resolve(
        &self,
        roles: &[String],
        types: &[String],
        sides: &[String],
    ) -> HashMap<RoleId, (Vec<usize>, Vec<usize>)> {
        let mut out: HashMap<RoleId, (Vec<usize>, Vec<usize>)> = HashMap::new();
        for pop in &self.populations {
            let mut left = Vec::new();
            let mut right = Vec::new();
            for i in 0..roles.len() {
                let matches = match pop.membership {
                    Membership::Role(slug) => roles[i] == slug,
                    // Partner classes only apply to neurons that are not core
                    // circuit members, which the data marks as role "other".
                    Membership::PartnerClass(class) => roles[i] == "other" && types[i] == class,
                };
                if !matches {
                    continue;
                }
                if pop.bilateral && sides[i] != "left" {
                    right.push(i);
                } else {
                    left.push(i);
                }
            }
            out.insert(pop.slug, (left, right));
        }
        out
    }

    /// Per-neuron resting drive, drawn once at construction.
    pub fn baselines(&self, roles: &[String], rng: &mut crate::rng::Pcg32) -> Vec<f32> {
        roles
            .iter()
            .map(|r| {
                let b = self
                    .populations
                    .iter()
                    .find(|p| matches!(p.membership, Membership::Role(s) if s == r))
                    .map(|p| p.baseline)
                    .unwrap_or(self.default_baseline);
                match b {
                    Baseline::Fixed(v) => v,
                    Baseline::Range(lo, hi) => rng.range(lo, hi),
                }
            })
            .collect()
    }
}

/// *Drosophila melanogaster*, FlyWire FAFB v783.
///
/// Every number here is lifted from `Sim.swift:163-210` and the colour/label
/// tables from `BrainView.swift`. Adding a population is now: extend the ETL,
/// add one row here, write the readout, add a test — four steps in two files,
/// down from eight across five.
pub fn drosophila() -> RoleManifest {
    use Baseline::*;
    use Membership::*;
    RoleManifest {
        creature: "drosophila",
        default_baseline: Fixed(0.002), // gf and anything unlisted: silent unless driven
        default_color: [0.55, 0.55, 0.62],
        populations: vec![
            Population {
                slug: "lc4",
                label: "LC4 - looming detectors",
                membership: Role("lc4"),
                bilateral: true,
                baseline: Fixed(0.004),
                color: [0.20, 0.70, 0.95],
            },
            Population {
                slug: "lplc2",
                label: "LPLC2 - looming detectors",
                membership: Role("lplc2"),
                bilateral: true,
                baseline: Fixed(0.004),
                color: [0.20, 0.70, 0.95],
            },
            Population {
                slug: "gf",
                label: "DNp01 - GIANT FIBER (escape command)",
                membership: Role("gf"),
                bilateral: false,
                baseline: Fixed(0.002),
                color: [1.00, 0.85, 0.25],
            },
            Population {
                slug: "dna01",
                label: "DNa01 - steering",
                membership: Role("dna01"),
                bilateral: true,
                baseline: Fixed(0.036),
                color: [0.35, 0.95, 0.55],
            },
            Population {
                slug: "dna02",
                label: "DNa02 - steering",
                membership: Role("dna02"),
                bilateral: true,
                baseline: Fixed(0.036),
                color: [0.35, 0.95, 0.55],
            },
            Population {
                slug: "dnp09",
                label: "DNp09 - forward walking",
                membership: Role("dnp09"),
                bilateral: false,
                baseline: Fixed(0.038),
                color: [0.95, 0.55, 0.20],
            },
            Population {
                slug: "dng11",
                label: "DNg11 - grooming",
                membership: Role("dng11"),
                bilateral: false,
                baseline: Fixed(0.036),
                color: [0.85, 0.45, 0.90],
            },
            Population {
                slug: "mdn",
                label: "MDN - backward walking (moonwalker)",
                membership: Role("mdn"),
                bilateral: false,
                baseline: Fixed(0.036),
                color: [0.95, 0.30, 0.40],
            },
            Population {
                slug: "escw",
                label: "DNp02/04/11 - escape manoeuvre",
                membership: Role("escw"),
                bilateral: false,
                baseline: Fixed(0.036),
                color: [1.00, 0.55, 0.35],
            },
            Population {
                slug: "ascend",
                label: "ascending - leg proprioception",
                membership: PartnerClass("ascending"),
                bilateral: false,
                baseline: Range(0.010, 0.070),
                color: [0.20, 0.45, 0.18],
            },
            Population {
                slug: "sens",
                label: "sensory - wind and touch",
                membership: PartnerClass("sensory"),
                bilateral: false,
                baseline: Range(0.010, 0.070),
                color: [0.14, 0.36, 0.34],
            },
            Population {
                slug: "other",
                label: "circuit partners",
                membership: Role("other"),
                bilateral: false,
                baseline: Range(0.010, 0.070),
                color: [0.55, 0.55, 0.62],
            },
        ],
    }
}

/// The jumping-spider chimera (SPIDER_PLAN.md §3): the fly's manifest with the
/// wing module removed and two populations added — FlyWire's **LC11**
/// small-object detectors (measured), and the **pounce** node (authored, the
/// one connective no animal has). Built from [`drosophila`] rather than copied,
/// so a change to a shared module's row cannot silently diverge between the
/// two creatures.
pub fn salticid() -> RoleManifest {
    use Baseline::*;
    use Membership::*;
    let mut m = drosophila();
    m.creature = "salticid";
    m.populations.retain(|p| p.slug != "escw");
    let at = m
        .populations
        .iter()
        .position(|p| p.slug == "gf")
        .unwrap_or(m.populations.len());
    m.populations.insert(
        at,
        Population {
            slug: "lc11",
            label: "LC11 - small-object detectors (prey)",
            membership: Role("lc11"),
            bilateral: true,
            // A sensory input population, like LC4: driven by transduction,
            // resting like the looming detectors do.
            baseline: Fixed(0.004),
            color: [0.30, 0.90, 0.85],
        },
    );
    m.populations.insert(
        at + 1,
        Population {
            slug: "pounce",
            label: "pounce connective - AUTHORED (no such neuron)",
            membership: Role("pounce"),
            bilateral: false,
            // Silent unless the LC11 population drives it: the same discipline
            // as the giant fiber, and deterministic like every command node.
            baseline: Fixed(0.002),
            // Cool, and unlike any measured population's colour: the brain
            // window's provenance colouring must be legible without a legend.
            color: [0.60, 0.70, 1.00],
        },
    );
    m
}

/// *Caenorhabditis elegans*, hermaphrodite.
///
/// 302 neurons, 279 of them with synapses — the only complete, cell-identified
/// whole-animal connectome (White et al. 1986; Cook et al. 2019 covers both
/// sexes). Unlike the fly, **every neuron has a name and a known job**, which
/// is why the brain window for a worm can label individual cells rather than
/// regions.
///
/// Baselines here are membrane-potential offsets, not spike-rate drives: this
/// creature runs on [`crate::graded::GradedSim`], because *C. elegans* neurons
/// are predominantly non-spiking.
///
/// Populations are grouped by function, following the standard locomotor
/// circuit description:
///
/// - **command interneurons** set direction: `AVB`/`PVC` drive forward,
///   `AVA`/`AVD`/`AVE` drive reversal.
/// - **motor neuron classes** execute it: `VB`/`DB` forward, `VA`/`DA`
///   backward, `VD`/`DD` inhibitory.
/// - **mechanosensors** decide when: `ALM`/`AVM` anterior touch triggers
///   reversal, `PLM` posterior touch accelerates forward. This is the
///   tap-withdrawal circuit, the canonical assay for habituation.
/// - **`AFD`** is the thermosensor — the worm migrates toward its cultivation
///   temperature, which is the mapping that makes machine heat meaningful.
/// - **`AWA`/`AWC`** are chemosensors; **`RIS`** gates sleep-like quiescence.
pub fn c_elegans() -> RoleManifest {
    use Baseline::*;
    use Membership::*;
    RoleManifest {
        creature: "c_elegans",
        default_baseline: Fixed(0.0),
        default_color: [0.55, 0.58, 0.62],
        populations: vec![
            Population {
                slug: "forward",
                label: "AVB / PVC - forward command",
                membership: Role("forward"),
                bilateral: true,
                baseline: Fixed(0.6),
                color: [0.35, 0.90, 0.60],
            },
            Population {
                slug: "reverse",
                label: "AVA / AVD / AVE - reverse command",
                membership: Role("reverse"),
                bilateral: true,
                baseline: Fixed(0.4),
                color: [0.95, 0.45, 0.35],
            },
            Population {
                slug: "motor_b",
                label: "VB / DB - forward motor neurons",
                membership: Role("motor_b"),
                bilateral: false,
                baseline: Fixed(0.2),
                color: [0.40, 0.80, 0.95],
            },
            Population {
                slug: "motor_a",
                label: "VA / DA - backward motor neurons",
                membership: Role("motor_a"),
                bilateral: false,
                baseline: Fixed(0.2),
                color: [0.95, 0.65, 0.35],
            },
            Population {
                slug: "motor_d",
                label: "VD / DD - inhibitory motor neurons",
                membership: Role("motor_d"),
                bilateral: false,
                baseline: Fixed(0.1),
                color: [0.65, 0.45, 0.90],
            },
            Population {
                slug: "touch",
                label: "ALM / AVM - anterior touch (reversal)",
                membership: Role("touch"),
                bilateral: true,
                baseline: Fixed(0.0),
                color: [1.00, 0.85, 0.30],
            },
            Population {
                slug: "touch_post",
                label: "PLM / PVM - posterior touch (accelerate)",
                membership: Role("touch_post"),
                bilateral: true,
                baseline: Fixed(0.0),
                color: [1.00, 0.70, 0.20],
            },
            Population {
                slug: "thermo",
                label: "AFD - thermosensor (thermotaxis)",
                membership: Role("thermo"),
                bilateral: true,
                baseline: Fixed(0.0),
                color: [0.95, 0.35, 0.55],
            },
            Population {
                slug: "chemo",
                label: "AWA / AWC - chemosensors",
                membership: Role("chemo"),
                bilateral: true,
                baseline: Fixed(0.0),
                color: [0.40, 0.95, 0.85],
            },
            Population {
                slug: "turn",
                label: "RIM / RIV / SMD - omega turn",
                membership: Role("turn"),
                bilateral: true,
                baseline: Fixed(0.15),
                color: [0.90, 0.55, 0.95],
            },
            Population {
                slug: "sleep",
                label: "RIS - quiescence",
                membership: Role("sleep"),
                bilateral: false,
                baseline: Fixed(0.05),
                color: [0.45, 0.50, 0.85],
            },
            Population {
                slug: "inter",
                label: "interneurons",
                membership: Role("inter"),
                bilateral: false,
                baseline: Range(0.05, 0.25),
                color: [0.55, 0.58, 0.62],
            },
            Population {
                slug: "sensory",
                label: "sensory neurons",
                membership: Role("sensory"),
                bilateral: false,
                baseline: Fixed(0.0),
                color: [0.30, 0.70, 0.80],
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny() -> (Vec<String>, Vec<String>, Vec<String>) {
        let roles: Vec<String> = ["lc4", "lc4", "gf", "dna01", "dna01", "other", "other"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let types: Vec<String> = ["LC4", "LC4", "DNp01", "DNa01", "DNa01", "ascending", "sensory"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let sides: Vec<String> = ["left", "right", "center", "left", "right", "center", "center"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        (roles, types, sides)
    }

    #[test]
    fn bilateral_populations_split_by_side() {
        let (r, t, s) = tiny();
        let g = drosophila().resolve(&r, &t, &s);
        let (l, rt) = &g["lc4"];
        assert_eq!(l, &vec![0]);
        assert_eq!(rt, &vec![1]);
        let (dl, dr) = &g["dna01"];
        assert_eq!(dl, &vec![3]);
        assert_eq!(dr, &vec![4]);
    }

    #[test]
    fn non_bilateral_populations_collect_into_one_list() {
        let (r, t, s) = tiny();
        let g = drosophila().resolve(&r, &t, &s);
        let (gf, right) = &g["gf"];
        assert_eq!(gf, &vec![2]);
        assert!(right.is_empty());
    }

    /// Partner populations are selected by super-class, not by role — that
    /// distinction is what puts the proprioceptive and wind inputs in the right
    /// groups.
    #[test]
    fn partner_classes_select_by_super_class() {
        let (r, t, s) = tiny();
        let g = drosophila().resolve(&r, &t, &s);
        assert_eq!(g["ascend"].0, vec![5]);
        assert_eq!(g["sens"].0, vec![6]);
    }

    /// Bilateral command pairs must get identical, deterministic baselines.
    /// A random per-side value would let asymmetry come from luck instead of
    /// from the wiring — the exact failure CLAUDE.md warns about.
    #[test]
    fn command_neuron_baselines_are_deterministic_and_side_symmetric() {
        let (r, t, _s) = tiny();
        let m = drosophila();
        let a = m.baselines(&r, &mut crate::rng::Pcg32::new(1));
        let b = m.baselines(&r, &mut crate::rng::Pcg32::new(999));
        // DNa01 left and right (indices 3, 4) must match each other...
        assert_eq!(a[3], a[4]);
        // ...and must not vary with the seed.
        assert_eq!(a[3], b[3]);
        assert_eq!(a[2], b[2], "gf baseline must be deterministic");
        // Interneurons, by contrast, are meant to vary.
        assert_ne!(a[5], b[5], "partner baselines should be randomised");
        let _ = t;
    }

    #[test]
    fn baselines_match_the_swift_constants() {
        let (r, _t, _s) = tiny();
        let a = drosophila().baselines(&r, &mut crate::rng::Pcg32::new(7));
        assert_eq!(a[0], 0.004, "lc4");
        assert_eq!(a[2], 0.002, "gf");
        assert_eq!(a[3], 0.036, "dna01");
        assert!((0.010..=0.070).contains(&a[5]), "partner range");
    }

    #[test]
    fn every_population_has_a_label_and_a_colour() {
        let m = drosophila();
        for p in &m.populations {
            assert!(!p.label.is_empty(), "{} has no label", p.slug);
            assert!(
                p.color.iter().any(|c| *c > 0.0),
                "{} has a black colour",
                p.slug
            );
        }
        // Unknown roles must degrade, not panic.
        assert_eq!(m.label_for("nonsense"), "circuit partners");
        assert_eq!(m.color_for("nonsense")[3], 1.0);
    }

    /// The worm's manifest must name the circuit that actually produces its
    /// behaviour, or the readout has nothing to read.
    #[test]
    fn the_worm_manifest_covers_the_locomotor_and_touch_circuits() {
        let m = c_elegans();
        assert_eq!(m.creature, "c_elegans");
        for required in [
            "forward",
            "reverse",
            "motor_b",
            "motor_a",
            "motor_d",
            "touch",
            "touch_post",
            "thermo",
        ] {
            assert!(m.find(required).is_some(), "missing population {required}");
        }
        // Anterior and posterior touch drive opposite behaviours, so they must
        // never be collapsed into one population.
        assert_ne!(m.color_for("touch"), m.color_for("touch_post"));
        assert!(m.label_for("reverse").contains("AVA"));
        assert!(m.label_for("thermo").contains("AFD"));
    }

    /// The two creatures are independent tables; a change to one must not
    /// silently reach the other.
    #[test]
    fn the_two_creatures_have_separate_manifests() {
        let fly = drosophila();
        let worm = c_elegans();
        assert_ne!(fly.creature, worm.creature);
        assert!(fly.find("gf").is_some());
        assert!(worm.find("gf").is_none(), "the worm has no giant fiber");
        assert!(worm.find("reverse").is_some());
        assert!(fly.find("reverse").is_none());
    }

    #[test]
    fn the_giant_fiber_is_the_brightest_population() {
        let m = drosophila();
        let lum = |c: [f32; 4]| c[0] + c[1] + c[2];
        assert!(lum(m.color_for("gf")) > lum(m.color_for("other")));
    }
}

/// The manifest for a creature that has no neurons.
///
/// Empty on purpose. A procedural creature has no populations to name, colour
/// or read out, and every consumer already degrades to "nothing here": group
/// lookups return empty slices, the brain window has no points to draw, and
/// the readout produces a resting `BrainSignals`. Handing back a plausible
/// table instead would be inventing anatomy.
pub fn procedural() -> RoleManifest {
    RoleManifest {
        creature: "procedural",
        default_baseline: Baseline::Fixed(0.0),
        default_color: [0.55, 0.58, 0.62],
        populations: Vec::new(),
    }
}
