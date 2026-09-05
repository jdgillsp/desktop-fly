//! The suites as real tests, so `cargo test` enforces what CLAUDE.md calls the
//! ground truth. Skipped with a warning if `data/` is not reachable, so the
//! crate still builds in a checkout without the derived files.

use dfcore::{data, suites};

fn brain() -> Option<data::BrainData> {
    match data::load() {
        Ok(b) => Some(b),
        Err(e) => {
            eprintln!("skipping: {e}");
            None
        }
    }
}

/// Population sizes documented in CLAUDE.md's neuron->behavior table. If the
/// data file or the role assignment drifts, this catches it before the
/// dynamics tests report confusing failures.
#[test]
fn circuit_populations_match_documented_counts() {
    let Some(b) = brain() else { return };
    let sim = dfcore::LifSim::new(&b.circuit, dfcore::DEFAULT_SEED);
    assert_eq!(sim.n, 668, "circuit size");
    assert_eq!(
        sim.loom_left.len() + sim.loom_right.len(),
        314,
        "LC4 (104) + LPLC2 (210)"
    );
    assert_eq!(sim.gf.len(), 2, "DNp01 giant fiber");
    assert_eq!(sim.dna_l.len() + sim.dna_r.len(), 4, "DNa01 + DNa02");
    assert_eq!(sim.fwd.len(), 2, "DNp09");
    assert_eq!(sim.groom.len(), 6, "DNg11");
    assert_eq!(sim.mdn.len(), 4, "MDN");
    assert_eq!(sim.escw.len(), 6, "DNp02/04/11");
    assert_eq!(sim.ascend.len(), 27, "ascending partners");
    assert_eq!(sim.sens.len(), 16, "sensory partners");
}

#[test]
fn simtest_invariants_hold() {
    let Some(b) = brain() else { return };
    let r = suites::sim_test(&b, dfcore::DEFAULT_SEED);
    for l in &r.lines {
        println!("{l}");
    }
    assert!(r.passed, "simtest failed");
}

#[test]
fn all_seventeen_behavior_checks_pass() {
    let Some(b) = brain() else { return };
    let r = suites::behavior_test(&b, dfcore::DEFAULT_SEED);
    for l in &r.lines {
        println!("{l}");
    }
    assert_eq!(r.lines.len(), 18, "17 checks plus the summary line");
    assert!(r.passed, "behaviortest failed");
}

/// The port seeds its PRNG where the Swift build did not, so the suites must
/// not be passing by luck on one lucky stream.
#[test]
fn suites_are_not_seed_fragile() {
    let Some(b) = brain() else { return };
    for seed in [1u64, 2, 7, 42, 1337] {
        assert!(
            suites::sim_test(&b, seed).passed,
            "simtest failed on seed {seed}"
        );
        assert!(
            suites::behavior_test(&b, seed).passed,
            "behaviortest failed on seed {seed}"
        );
    }
}

/// The escape latency is the project's headline claim (README: "~4 ms").
/// It is a race between gap-junction-boosted LC->GF excitation and ~1,200
/// synapses of 4 ms-delayed feedforward inhibition, so it is exactly the thing
/// a careless transliteration would break.
#[test]
fn giant_fiber_is_silent_at_rest_but_fires_fast_on_abrupt_loom() {
    let Some(b) = brain() else { return };
    let mut sim = dfcore::LifSim::new(&b.circuit, dfcore::DEFAULT_SEED);

    for _ in 0..40 {
        sim.step(100);
        assert!(!sim.consume_gf(), "giant fiber fired spontaneously at rest");
    }

    let mut latency = -1i64;
    for ms in 0..400 {
        sim.loom_l = 1.0;
        sim.loom_r = 0.5;
        sim.step(1);
        if sim.consume_gf() && latency < 0 {
            latency = ms;
        }
    }
    assert!(latency >= 0, "giant fiber never fired on an abrupt loom");
    assert!(latency <= 10, "escape latency {latency} ms, expected ~4");
}

/// The "siesta coma" bug: scaling baselines linearly silences the network.
/// The compressed scale must leave the fly slowed, not paralysed.
#[test]
fn siesta_slows_the_fly_without_paralysing_it() {
    let Some(b) = brain() else { return };
    let mut sim = dfcore::LifSim::new(&b.circuit, dfcore::DEFAULT_SEED);
    sim.step(2000);
    sim.activity_scale = 1.0 - (1.0 - 0.55) * 0.35; // 0.8425
    let (mut on, mut samples) = (0, 0);
    for ms in 0..15_000 {
        sim.step(1);
        if ms % 10 == 0 {
            samples += 1;
            if sim.rate_fwd / 10.0 > 0.22 {
                on += 1;
            }
        }
    }
    let pct = 100.0 * on as f32 / samples as f32;
    assert!(pct > 3.0, "siesta walk-drive {pct:.1}% — network went comatose");
}
