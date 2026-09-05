//! CLI mirroring the Swift build's diagnostic modes (CLAUDE.md "Build, run, verify"):
//!   dfcore --simtest        circuit invariants
//!   dfcore --behaviortest   17 end-to-end sim -> body checks
//! Both must pass after any change to the sim or behaviour.

use dfcore::{data, suites};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed = args
        .iter()
        .position(|a| a == "--seed")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(dfcore::DEFAULT_SEED);

    let brain = match data::load() {
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
        let r = suites::sim_test(&brain, seed);
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
        let r = suites::behavior_test(&brain, seed);
        for l in &r.lines {
            println!("{l}");
        }
        ok &= r.passed;
    }

    if !ran {
        eprintln!("usage: dfcore [--simtest] [--behaviortest] [--seed N]");
        std::process::exit(2);
    }
    std::process::exit(if ok { 0 } else { 1 });
}
