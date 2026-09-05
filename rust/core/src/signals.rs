//! Population rates -> body commands.
//!
//! Port of `BrainSignals` (Sim.swift:14) and `SignalBuilder` (main.swift:463).
//! This is the layer where "what the network is doing" becomes "what the body
//! should do", and it is shared by the app loop and `--behaviortest` so both
//! exercise the identical mapping.

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
        s
    }
}
