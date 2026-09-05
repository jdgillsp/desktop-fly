//! Population rates -> body commands.
//!
//! Port of `BrainSignals` (Sim.swift:14) and `SignalBuilder` (main.swift:463).
//! This is the layer where "what the network is doing" becomes "what the body
//! should do", and it is shared by the app loop and `--behaviortest` so both
//! exercise the identical mapping.

use crate::graded::GradedSim;
use crate::lif::LifSim;
use crate::util::clamp;

/// What the brain tells the body each frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct BrainSignals {
    /// Giant fiber spiked -> take off NOW.
    pub escape: bool,
    /// Looming-detector population rate, 0..1.
    pub nervous: f32,
    /// rad/s steering from the DNa01/DNa02 left-right rate difference.
    pub turn_bias: f32,
    /// MDN burst -> backward walking.
    pub backward: bool,
    /// DNp09 forward-walking command rate, ~0..1.3.
    pub walk_drive: f32,
    /// DNg11 grooming command rate, ~0..1.5 (deliberately unclamped).
    pub groom_drive: f32,
    /// DNp02/04/11 escape-maneuver DN rate, ~0..1.3.
    pub wing_drive: f32,
    /// Whole-population activity, ~0..1.
    pub arousal: f32,
    /// Thermal "temperature" scaling of locomotion.
    pub tempo: f32,
    /// Circadian + idle -> sleep-like state.
    pub sleep: bool,
    /// LC11 small-object population rate, 0..1. Zero for the fly, whose
    /// extract has no LC11; the chimera's prey-detection drive.
    pub pursuit: f32,
    /// LC11 left-minus-right, -1..1: which eye the small object is in.
    pub prey_bias: f32,
    /// The authored pounce node spiked -> pounce NOW, if something is in range.
    pub pounce: bool,
}

impl BrainSignals {
    /// `BrainSignals()` in Swift defaults `tempo` to 1, not 0 — a plain
    /// `Default::default()` would silently freeze the fly.
    pub fn new() -> Self {
        BrainSignals {
            tempo: 1.0,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SignalBuilder {
    dna_baseline: f32,
}

impl SignalBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn make(&mut self, sim: &mut LifSim, dt: f32) -> BrainSignals {
        let diff = sim.rate_dna_l - sim.rate_dna_r;
        // Slow adaptation (tau ~8 s). The connectome has a persistent left/right
        // wiring asymmetry; adapting it out is what makes steady-state walking
        // straight, so only *transient* DNa asymmetries steer.
        self.dna_baseline += (diff - self.dna_baseline) * (dt / 8.0).min(1.0);

        let mut s = BrainSignals::new();
        s.escape = sim.consume_gf();
        s.nervous = clamp(sim.rate_loom / 80.0, 0.0, 1.0);
        s.turn_bias = clamp((diff - self.dna_baseline) * 0.04, -1.0, 1.0);
        s.backward = sim.rate_mdn > 8.0;
        // Always clamp: an unclamped walk_drive once sent the fly to 1,100 pt/s.
        s.walk_drive = clamp(sim.rate_fwd / 10.0, 0.0, 1.3);
        // groom_drive is intentionally NOT clamped, matching main.swift:477.
        s.groom_drive = sim.rate_groom / 8.0;
        s.wing_drive = clamp(sim.rate_escw / 10.0, 0.0, 1.3);
        s.arousal = clamp(sim.rate_pop / 20.0, 0.0, 1.0);
        // Chimera-only populations. Every one of these is zero for the fly,
        // whose extract has no LC11 and no pounce node, so its readout is
        // numerically unchanged.
        let lc11 = sim.rate_lc11_l + sim.rate_lc11_r;
        s.pursuit = clamp(lc11 / 80.0, 0.0, 1.0);
        s.prey_bias = if lc11 > 1.0 {
            clamp((sim.rate_lc11_l - sim.rate_lc11_r) / lc11, -1.0, 1.0)
        } else {
            0.0
        };
        s.pounce = sim.consume_pounce();
        s
    }
}

/// The worm's readout: graded population activity -> the same command struct.
///
/// *C. elegans* has no giant fiber and no wings, so most of [`BrainSignals`]
/// stays at rest. What it does have is a forward/reverse command pair (AVB/PVC
/// versus AVA/AVD/AVE) whose *balance* selects locomotion direction; that is
/// the whole mapping. `population_activity` is normalised depolarisation, so
/// the thresholds here are fractions of full activation rather than spike
/// rates, and there is nothing to adapt out: graded neurons do not have the
/// fly's noise-driven left/right rate asymmetry.
///
/// Like the fly's builder, this is the *only* place worm activity becomes a
/// command, so the app loop and any future worm suite share it.
#[derive(Debug, Default)]
pub struct GradedSignalBuilder {
    /// Reversal is a discrete bout in the animal; edge-trigger it so a
    /// sustained reverse-command depolarisation is one pirouette, not a
    /// stream of them.
    reversing: bool,
}

impl GradedSignalBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn make(&mut self, sim: &GradedSim) -> BrainSignals {
        let forward = sim.population_activity("forward");
        let reverse = sim.population_activity("reverse");
        let mut s = BrainSignals::new();
        // Forward drive is the command population's activation, net of the
        // reverse command's — the two inhibit each other in the animal, and
        // this keeps a worm from crawling forward while it is trying to back up.
        s.walk_drive = clamp(forward - reverse * 0.5, 0.0, 1.0);
        let want_reverse = reverse > forward + 0.15;
        s.backward = want_reverse && !self.reversing;
        self.reversing = want_reverse;
        s.arousal = clamp(forward.max(reverse), 0.0, 1.0);
        s
    }
}

#[cfg(test)]
mod graded_tests {
    use super::*;
    use crate::creature::{Connectome, GradedParams, Provenance};
    use crate::roles::{Baseline, Membership, Population, RoleManifest};

    /// Two command neurons, no wiring: activity is whatever is injected.
    fn sim() -> GradedSim {
        let manifest = RoleManifest {
            creature: "fixture",
            default_baseline: Baseline::Fixed(0.0),
            default_color: [0.5; 3],
            populations: vec![
                Population {
                    slug: "forward",
                    label: "fwd",
                    membership: Membership::Role("forward"),
                    bilateral: false,
                    baseline: Baseline::Fixed(0.0),
                    color: [0.0; 3],
                },
                Population {
                    slug: "reverse",
                    label: "rev",
                    membership: Membership::Role("reverse"),
                    bilateral: false,
                    baseline: Baseline::Fixed(0.0),
                    color: [0.0; 3],
                },
            ],
        };
        let c = Connectome {
            roles: vec!["forward".into(), "reverse".into()],
            cell_types: vec!["AVB".into(), "AVA".into()],
            sides: vec!["left".into(), "left".into()],
            positions: vec![[0.0; 3]; 2],
            chemical: vec![],
            electrical: vec![],
            neuron_origin: Vec::new(),
            edge_origin: Vec::new(),
            provenance: Provenance::Authored { note: "fixture".into() },
        };
        GradedSim::new(&c, manifest, GradedParams::default(), 1)
    }

    #[test]
    fn a_resting_worm_network_commands_nothing() {
        let mut s = sim();
        s.step(500);
        let mut b = GradedSignalBuilder::new();
        let out = b.make(&s);
        assert!(out.walk_drive < 0.1, "walk_drive {}", out.walk_drive);
        assert!(!out.backward);
        assert!(!out.escape, "a worm has no giant fiber");
    }

    #[test]
    fn forward_command_activity_becomes_walk_drive() {
        let mut s = sim();
        let fwd = s.group("forward").to_vec();
        s.stimulate(&fwd, 4.0, 400);
        s.step(300);
        let out = GradedSignalBuilder::new().make(&s);
        assert!(out.walk_drive > 0.3, "walk_drive {}", out.walk_drive);
        assert!(!out.backward);
    }

    /// A sustained reverse command is one reversal bout, then the body's own
    /// timers take over — the same edge-trigger discipline as the fly's MDN.
    #[test]
    fn reverse_command_triggers_one_reversal_not_a_stream() {
        let mut s = sim();
        let rev = s.group("reverse").to_vec();
        s.stimulate(&rev, 4.0, 600);
        s.step(300);
        let mut b = GradedSignalBuilder::new();
        let first = b.make(&s);
        s.step(50);
        let second = b.make(&s);
        assert!(first.backward, "reverse > forward must trigger a reversal");
        assert!(!second.backward, "and only once while it persists");
        assert!(first.walk_drive < 0.2, "no forward drive while reversing");
    }
}
