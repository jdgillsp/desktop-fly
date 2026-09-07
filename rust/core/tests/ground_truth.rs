//! The suites as real tests, so `cargo test` enforces what CLAUDE.md calls the
//! ground truth. Skipped with a warning if `data/` is not reachable, so the
//! crate still builds in a checkout without the derived files.

use dfcore::{data, suites, Creature};

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

/// Phase 4 was specified as a *pure refactor*: moving roles, baselines and
/// laterality out of three hand-synchronised `match` statements and into one
/// data manifest must not change the animal. Two sims built the same way must
/// produce byte-identical dynamics, and the manifest must reproduce exactly the
/// population counts CLAUDE.md documents.
#[test]
fn the_role_manifest_reproduces_the_circuit_exactly() {
    let Some(b) = brain() else { return };

    let run = || {
        let mut sim = dfcore::LifSim::new(&b.circuit, dfcore::DEFAULT_SEED);
        sim.loom_l = 0.0;
        sim.step(3000);
        sim.loom_l = 1.0;
        sim.loom_r = 0.5;
        sim.step(400);
        (sim.total_spikes, sim.rate_loom, sim.rate_pop)
    };
    let a = run();
    let c = run();
    assert_eq!(a.0, c.0, "spike totals must be deterministic");
    assert_eq!(a.1, c.1, "loom rate must be deterministic");
    assert_eq!(a.2, c.2, "population rate must be deterministic");

    // And the manifest must resolve the same groups the hardcoded switch did.
    let sim = dfcore::LifSim::new(&b.circuit, dfcore::DEFAULT_SEED);
    assert_eq!(sim.loom_left.len(), 162);
    assert_eq!(sim.loom_right.len(), 152);
    assert_eq!(sim.dna_l.len(), 2);
    assert_eq!(sim.dna_r.len(), 2);
}

/// The `Sim` trait must expose the same populations the concrete type does, or
/// a second creature would be wired to different neurons than the fly is.
#[test]
fn the_sim_trait_agrees_with_the_concrete_simulation() {
    use dfcore::Sim;
    let Some(b) = brain() else { return };
    let sim = dfcore::LifSim::new(&b.circuit, dfcore::DEFAULT_SEED);

    assert_eq!(Sim::n(&sim), sim.n);
    assert_eq!(Sim::group(&sim, "gf"), sim.gf.as_slice());
    assert_eq!(Sim::group(&sim, "dnp09"), sim.fwd.as_slice());
    assert_eq!(Sim::group(&sim, "sens"), sim.sens.as_slice());
    assert!(Sim::group(&sim, "no-such-population").is_empty());
    assert_eq!(Sim::positions(&sim).len(), sim.n);
    assert_eq!(Sim::manifest(&sim).creature, "drosophila");
}

// ---------------------------------------------------------------------------
// Creature #3: the chimera (SPIDER_PLAN.md). Its data ships, so these are not
// skipped; if data/salticid is missing the ETL was not run and that is a
// failure worth seeing.
// ---------------------------------------------------------------------------

fn chimera() -> data::BrainData {
    data::load_for("salticid").expect("data/salticid - run etl_chimera.py")
}

#[test]
fn chimera_populations_match_the_etl_report() {
    let b = chimera();
    let sim = dfcore::LifSim::with_params(
        &b.circuit,
        dfcore::DEFAULT_SEED,
        dfcore::LifParams::default(),
        dfcore::roles::salticid(),
    );
    assert_eq!(sim.n, 790, "789 measured + 1 authored");
    assert_eq!(sim.lc11_l.len() + sim.lc11_r.len(), 127, "LC11");
    assert_eq!(sim.pounce.len(), 1, "the authored pounce node");
    assert!(sim.escw.is_empty(), "no wing module");
    assert_eq!(sim.gf.len(), 2);
    assert_eq!(sim.loom_left.len() + sim.loom_right.len(), 314, "LC4 + LPLC2, as the fly");
    // The honesty mechanism, on the real file.
    let c = dfcore::Connectome::from_circuit(&b.circuit, dfcore::Salticid.provenance());
    assert_eq!(c.authored_counts(), (1, 127));
    let authored: Vec<usize> = (0..c.len())
        .filter(|&i| c.origin_of_neuron(i) == dfcore::Origin::Authored)
        .collect();
    assert_eq!(authored.len(), 1);
    assert_eq!(b.circuit.neurons[authored[0]].role, "pounce");
    assert_eq!(b.circuit.neurons[authored[0]].id, "authored:pounce");
    use dfcore::Sim;
    assert_eq!(sim.origin(authored[0]), dfcore::Origin::Authored);
    assert_eq!(sim.origin(0), dfcore::Origin::Measured);
}

#[test]
fn chimera_invariants_hold() {
    let b = chimera();
    let r = suites::chimera_test(&b, dfcore::DEFAULT_SEED);
    for l in &r.lines {
        println!("{l}");
    }
    assert!(r.passed, "chimera test failed");
}

#[test]
fn all_spider_behavior_checks_pass() {
    let b = chimera();
    let r = suites::spider_behavior_test(&b, dfcore::DEFAULT_SEED);
    for l in &r.lines {
        println!("{l}");
    }
    assert_eq!(r.lines.len(), 16, "15 checks plus the summary line");
    assert!(r.passed, "spider behaviour test failed");
}

#[test]
fn chimera_suites_are_not_seed_fragile() {
    let b = chimera();
    for seed in [1u64, 2, 7, 42, 1337] {
        assert!(suites::chimera_test(&b, seed).passed, "chimera test failed on seed {seed}");
        assert!(
            suites::spider_behavior_test(&b, seed).passed,
            "spider behaviour test failed on seed {seed}"
        );
    }
}
