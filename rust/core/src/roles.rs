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

    #[test]
    fn the_giant_fiber_is_the_brightest_population() {
        let m = drosophila();
        let lum = |c: [f32; 4]| c[0] + c[1] + c[2];
        assert!(lum(m.color_for("gf")) > lum(m.color_for("other")));
    }
}
