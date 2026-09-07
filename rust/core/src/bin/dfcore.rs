//! CLI mirroring the Swift build's diagnostic modes (CLAUDE.md "Build, run, verify"):
//!   dfcore --simtest                      circuit invariants (the fly)
//!   dfcore --behaviortest                 17 end-to-end sim -> body checks (the fly)
//!   dfcore --creature salticid --simtest  the chimera's invariants, on its own data
//! Both fly suites must pass after any change to the sim or behaviour; the
//! chimera suite after any change to its data or the LC11/pounce pathway.

use dfcore::{data, suites};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed = args
        .iter()
        .position(|a| a == "--seed")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(dfcore::DEFAULT_SEED);
    let creature_id = args
        .iter()
        .position(|a| a == "--creature")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "drosophila".to_string());
    let Some(creature) = dfcore::by_id(&creature_id) else {
        eprintln!(
            "unknown creature '{creature_id}' (have: {})",
            dfcore::CREATURE_IDS.join(", ")
        );
        std::process::exit(2);
    };

    let brain = match data::load_for(creature.data_dir()) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let mut ran = false;
    let mut ok = true;

    if args.iter().any(|a| a == "--simtest") {
        ran = true;
        let r = match creature.id() {
            "salticid" => suites::chimera_test(&brain, seed),
            "araneus" | "parasteatoda" | "agelenopsis" => suites::weaver_test(&brain, seed),
            "drosophila" => suites::sim_test(&brain, seed),
            other => {
                eprintln!("no circuit suite for {other}");
                std::process::exit(2);
            }
        };
        for l in &r.lines {
            println!("{l}");
        }
        ok &= r.passed;
    }
    if args.iter().any(|a| a == "--behaviortest") {
        ran = true;
        if ok {
            println!();
        }
        let r = match creature.id() {
            "salticid" => suites::spider_behavior_test(&brain, seed),
            "araneus" | "parasteatoda" | "agelenopsis" => {
                suites::weaver_behavior_test(&brain, seed, creature.id())
            }
            "drosophila" => suites::behavior_test(&brain, seed),
            other => {
                eprintln!("no behaviour suite for {other}");
                std::process::exit(2);
            }
        };
        for l in &r.lines {
            println!("{l}");
        }
        ok &= r.passed;
    }

    if !ran {
        eprintln!("usage: dfcore [--creature ID] [--simtest] [--behaviortest] [--seed N]");
        std::process::exit(2);
    }
    std::process::exit(if ok { 0 } else { 1 });
}
