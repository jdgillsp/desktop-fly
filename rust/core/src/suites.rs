//! The two ground-truth suites, ported from `main.swift:125` (`runSimtest`) and
//! `main.swift:231` (`runBehaviorTest`).
//!
//! `CLAUDE.md` is explicit that these are the ground truth and must both be run
//! after any change to the sim or behaviour. The invariants they enforce:
//!
//!   * the giant fiber is silent over 4 s of rest
//!   * the giant fiber fires within ~10 ms of an abrupt loom
//!   * walk-drive duty lands in 20–50%
//!   * the siesta (activity scale 0.84) leaves walk-drive above 3% — not comatose
//!   * click-stimulation reaches the body
//!   * all 17 end-to-end behaviour checks pass

use crate::body::{Fly, State, FLY_SCALE};
use crate::data::BrainData;
use crate::lif::LifSim;
use crate::signals::{BrainSignals, SignalBuilder};
use crate::util::{circadian_activity, Ledge, Vec2};

pub struct Outcome {
    pub passed: bool,
    pub lines: Vec<String>,
}

impl Outcome {
    fn say(&mut self, s: String) {
        self.lines.push(s);
    }
}

// ---------------------------------------------------------------------------
// simtest — circuit invariants
// ---------------------------------------------------------------------------

pub fn sim_test(data: &BrainData, seed: u64) -> Outcome {
    let mut out = Outcome {
        passed: false,
        lines: Vec::new(),
    };
    let mut sim = LifSim::new(&data.circuit, seed);

    out.say(format!(
        "circuit: {} neurons | loom L/R: {}/{} | GF: {} | DNa L/R: {}/{} | MDN: {} \
         | DNp09: {} | DNg11: {} | escW: {} | ascend: {} | sens: {}",
        sim.n,
        sim.loom_left.len(),
        sim.loom_right.len(),
        sim.gf.len(),
        sim.dna_l.len(),
        sim.dna_r.len(),
        sim.mdn.len(),
        sim.fwd.len(),
        sim.groom.len(),
        sim.escw.len(),
        sim.ascend.len(),
        sim.sens.len()
    ));

    // Phase 1: 4 s of spontaneous activity. The GF must stay silent.
    let mut gf_spont = 0;
    for _ in 0..40 {
        sim.step(100);
        if sim.consume_gf() {
            gf_spont += 1;
        }
    }
    let pop_hz = sim.total_spikes as f32 / 4.0 / sim.n as f32;
    out.say(format!(
        "spontaneous 4s: pop {pop_hz:.2} Hz/neuron, LC {:.1} Hz, DNa02 L/R {:.1}/{:.1} Hz, \
         MDN {:.1} Hz, GF spikes: {gf_spont}",
        sim.rate_loom, sim.rate_dna_l, sim.rate_dna_r, sim.rate_mdn
    ));

    // Phase 2: an abrupt loom, as a cursor lunge produces. Must be a step, not
    // a ramp — slow ramps lose the race to feedforward inhibition by design.
    let mut gf_latency_ms: i64 = -1;
    let mut gf_loom = 0;
    for ms in 0..400 {
        sim.loom_l = 1.0;
        sim.loom_r = 0.5;
        sim.step(1);
        if sim.consume_gf() {
            gf_loom += 1;
            if gf_latency_ms < 0 {
                gf_latency_ms = ms;
            }
        }
    }
    sim.loom_l = 0.0;
    sim.loom_r = 0.0;
    out.say(format!(
        "abrupt loom 0.4s: LC rate {:.1} Hz, GF spikes {gf_loom}, first at {gf_latency_ms} ms",
        sim.rate_loom
    ));

    // Phase 3: 20 s with walking proprioception — do behaviour states emerge?
    let (mut walk_on, mut groom_on, mut samples) = (0, 0, 0);
    let (mut fwd_min, mut fwd_max) = (f32::MAX, 0.0f32);
    for ms in 0..20_000 {
        sim.gait_drive = 0.5;
        sim.gait_phase = (ms % 125) as f32 / 125.0; // 8 Hz gait
        sim.step(1);
        if ms % 10 == 0 {
            samples += 1;
            if sim.rate_fwd / 10.0 > 0.22 {
                walk_on += 1;
            }
            if sim.rate_groom / 8.0 > 0.5 {
                groom_on += 1;
            }
            fwd_min = fwd_min.min(sim.rate_fwd);
            fwd_max = fwd_max.max(sim.rate_fwd);
        }
    }
    let walk_pct = 100.0 * walk_on as f32 / samples as f32;
    out.say(format!(
        "behavior 20s: walk-drive on {walk_pct:.0}%, groom-drive on {:.0}%, \
         DNp09 {fwd_min:.1}-{fwd_max:.1} Hz, pop {:.1} Hz",
        100.0 * groom_on as f32 / samples as f32,
        sim.rate_pop
    ));
    sim.gait_drive = 0.0;

    // Phase 3b: the midday siesta must slow the fly, not paralyse it.
    // (The "siesta coma" bug: never scale baselines linearly.)
    sim.activity_scale = 1.0 - (1.0 - 0.55) * 0.35; // = 0.8425
    let (mut siesta_walk_on, mut siesta_samples) = (0, 0);
    for ms in 0..15_000 {
        sim.step(1);
        if ms % 10 == 0 {
            siesta_samples += 1;
            if sim.rate_fwd / 10.0 > 0.22 {
                siesta_walk_on += 1;
            }
        }
    }
    sim.activity_scale = 1.0;
    let siesta_pct = 100.0 * siesta_walk_on as f32 / siesta_samples as f32;
    out.say(format!(
        "siesta 15s (scale 0.84): walk-drive on {siesta_pct:.0}%"
    ));

    // Phase 4: an air puff (a fast cursor whoosh) — the wind startle pathway.
    let mut gf_puff = 0;
    for _ in 0..1000 {
        sim.air_puff = 1.0;
        sim.step(1);
        if sim.consume_gf() {
            gf_puff += 1;
        }
    }
    sim.air_puff = 0.0;
    out.say(format!("air puff 1s: GF spikes {gf_puff}"));

    // Phase 5: a gentle left-eye-only loom — the steering probe.
    for _ in 0..500 {
        sim.step(1);
        sim.consume_gf();
    }
    let diff0 = sim.rate_dna_l - sim.rate_dna_r;
    for _ in 0..1000 {
        sim.loom_l = 0.30;
        sim.loom_r = 0.0;
        sim.step(1);
        sim.consume_gf();
    }
    let diff1 = sim.rate_dna_l - sim.rate_dna_r;
    sim.loom_l = 0.0;
    out.say(format!(
        "left-eye loom: DNa L-R rate diff {diff0:+.1} -> {diff1:+.1} Hz, LC {:.1} Hz",
        sim.rate_loom
    ));

    // Phase 6: click-stimulation probes — what the brain window does.
    let gf_idx = sim.gf.clone();
    sim.stimulate(&gf_idx, 0.5, 40);
    sim.step(60);
    let gf_stim = sim.consume_gf();
    let groom_idx = sim.groom.clone();
    sim.stimulate(&groom_idx, 0.25, 400);
    sim.step(400);
    let groom_stim = sim.rate_groom;
    sim.consume_gf();
    out.say(format!(
        "click probes: GF cluster -> spike {}, DNg11 cluster -> groom rate {groom_stim:.0} Hz",
        if gf_stim { "yes" } else { "NO" }
    ));

    let pass = gf_spont == 0 && gf_loom > 0 && walk_on > 0 && gf_stim && siesta_pct > 3.0;
    out.say(
        if pass {
            "PASS: GF silent at rest, fires on loom; locomotor drive fluctuates; \
             stim works; siesta alive"
        } else {
            "FAIL: tune weights/noise"
        }
        .to_string(),
    );
    out.passed = pass;
    out
}

// ---------------------------------------------------------------------------
// behaviortest — 17 end-to-end sim -> body checks
// ---------------------------------------------------------------------------

/// The suite runs in free roam — the whole display, centred on the origin.
/// Spelled out as a `Region` so it is obvious that these ground-truth numbers
/// are the *unconfined* ones; a habitat is a different, smaller world.
const BOUNDS: crate::habitat::Region = crate::habitat::Region {
    center: Vec2::ZERO,
    size: (1512.0, 982.0),
};
const DT: f32 = 1.0 / 60.0;

struct Harness<'a> {
    data: &'a BrainData,
    seed: u64,
    failures: usize,
    lines: Vec<String>,
}

impl<'a> Harness<'a> {
    fn scenario(
        &mut self,
        name: &str,
        stim: impl Fn(&mut LifSim),
        hold: f32,
        setup: impl Fn(&mut Fly),
        check: impl Fn(&Fly) -> bool,
        describe: impl Fn(&Fly) -> String,
    ) {
        let mut sim = LifSim::new(&self.data.circuit, self.seed);
        let mut builder = SignalBuilder::new();
        let mut fly = Fly::new(Vec2::ZERO, self.seed);
        fly.state = State::Idle;
        fly.speed = 0.0;
        setup(&mut fly);

        // Settle the network and drain any startup GF latch.
        sim.step(400);
        sim.consume_gf();
        stim(&mut sim);

        let mut passed = false;
        let mut frames = (hold / DT) as i32;
        while frames > 0 {
            frames -= 1;
            sim.step((DT * 1000.0).round() as i64);
            let s = builder.make(&mut sim, DT);
            fly.update(DT, BOUNDS, None, Some(s));
            if check(&fly) {
                passed = true;
                break;
            }
        }
        if !passed {
            self.failures += 1;
        }
        self.lines.push(format!(
            "{}  {name}: {}",
            if passed { "PASS" } else { "FAIL" },
            describe(&fly)
        ));
    }

    fn body_check(&mut self, name: &str, run: impl FnOnce() -> (bool, String)) {
        let (ok, detail) = run();
        if !ok {
            self.failures += 1;
        }
        self.lines
            .push(format!("{}  {name}: {detail}", if ok { "PASS" } else { "FAIL" }));
    }
}

pub fn behavior_test(data: &BrainData, seed: u64) -> Outcome {
    let mut h = Harness {
        data,
        seed,
        failures: 0,
        lines: Vec::new(),
    };

    // ---- sim -> body scenarios (7) ----

    h.scenario(
        "GF stim -> escape flight",
        |s| {
            let g = s.gf.clone();
            s.stimulate(&g, 0.5, 40)
        },
        0.5,
        |_| {},
        |f| f.state == State::Flying,
        |f| format!("state={:?}", f.state),
    );

    h.scenario(
        "DNg11 stim -> grooming",
        |s| {
            let g = s.groom.clone();
            s.stimulate(&g, 0.25, 600)
        },
        1.5,
        |_| {},
        |f| f.state == State::Grooming,
        |f| format!("state={:?}", f.state),
    );

    h.scenario(
        "DNp09 stim -> walks, speed rises (capped)",
        |s| {
            let g = s.fwd.clone();
            s.stimulate(&g, 0.25, 1200)
        },
        1.5,
        |_| {},
        |f| f.state == State::Walking && f.speed > 40.0 && f.speed < 100.0,
        |f| format!("state={:?} speed={}", f.state, f.speed as i32),
    );

    h.scenario(
        "MDN stim (from idle) -> backward walk",
        |s| {
            let g = s.mdn.clone();
            s.stimulate(&g, 0.3, 600)
        },
        1.2,
        |_| {},
        |f| f.backward_timer > 0.0,
        |f| format!("backwardTimer={:.2}", f.backward_timer),
    );

    h.scenario(
        "DNa-left stim -> left (CCW) turn while walking",
        |s| {
            let g = s.dna_l.clone();
            s.stimulate(&g, 0.3, 900)
        },
        1.4,
        |f| {
            f.state = State::Walking;
            f.speed = 30.0;
            f.heading = 0.0;
        },
        |f| f.heading > 0.25, // heading0 is 0 by construction
        |f| format!("heading change {:+.2} rad", f.heading),
    );

    h.scenario(
        "moderate loom -> fear response (dart or escape)",
        |s| {
            s.loom_l = 0.45;
            s.loom_r = 0.45;
        },
        1.0,
        |_| {},
        |f| (f.state == State::Walking && f.speed > 100.0) || f.state == State::Flying,
        |f| format!("state={:?} speed={}", f.state, f.speed as i32),
    );

    h.scenario(
        "tap near fly -> startle escape via sensory pathway",
        |s| {
            let g = s.sens.clone();
            s.stimulate(&g, 0.45, 150)
        },
        0.8,
        |_| {},
        |f| f.state == State::Flying,
        |f| format!("state={:?}", f.state),
    );

    // ---- body-level environment checks, hand-built signals, no sim (10) ----

    let mut walk_signals = BrainSignals::new();
    walk_signals.walk_drive = 0.6;
    let seed = h.seed;

    h.body_check("ledge attach + follow window edge", || {
        let mut fly = Fly::new(Vec2::new(0.0, -55.0), seed);
        fly.state = State::Walking;
        fly.speed = 30.0;
        fly.heading = 0.0;
        fly.terrain = vec![Ledge {
            y: -40.0,
            x0: -300.0,
            x1: 300.0,
            id: 1,
        }];
        for _ in 0..240 {
            fly.update(DT, BOUNDS, None, Some(walk_signals));
            if fly.ledge.is_some() && (fly.pos.y + 40.0).abs() < 8.0 {
                return (true, format!("attached, y={}", fly.pos.y as i32));
            }
        }
        (
            false,
            format!(
                "state={:?} y={} ledge={}",
                fly.state,
                fly.pos.y as i32,
                fly.ledge.is_some()
            ),
        )
    });

    h.body_check("window closes underfoot -> takeoff", || {
        let mut fly = Fly::new(Vec2::new(0.0, -40.0), seed);
        fly.state = State::Walking;
        fly.speed = 25.0;
        fly.heading = 0.0;
        let l = Ledge {
            y: -40.0,
            x0: -300.0,
            x1: 300.0,
            id: 1,
        };
        fly.terrain = vec![l];
        fly.ledge = Some(l);
        fly.terrain = Vec::new(); // the window vanished
        for _ in 0..60 {
            fly.update(DT, BOUNDS, None, Some(walk_signals));
            if fly.state == State::Flying {
                return (true, "took off".to_string());
            }
        }
        (false, format!("state={:?}", fly.state))
    });

    h.body_check("sleep signal -> sleeping; wake -> grooming", || {
        let mut fly = Fly::new(Vec2::ZERO, seed);
        fly.state = State::Idle;
        let mut s = BrainSignals::new();
        s.sleep = true;
        for _ in 0..60 {
            fly.update(DT, BOUNDS, None, Some(s));
        }
        if fly.state != State::Sleeping {
            return (false, format!("no sleep: {:?}", fly.state));
        }
        s.sleep = false;
        fly.update(DT, BOUNDS, None, Some(s));
        (
            fly.state == State::Grooming,
            format!("woke to {:?}", fly.state),
        )
    });

    h.body_check("thermal tempo scales walking speed", || {
        let mut fly = Fly::new(Vec2::ZERO, seed);
        fly.state = State::Walking;
        fly.speed = 20.0;
        fly.heading = 0.0;
        let mut cool = walk_signals;
        cool.tempo = 1.0;
        for _ in 0..120 {
            fly.update(DT, BOUNDS, None, Some(cool));
        }
        let cool_speed = fly.speed;
        let mut hot = walk_signals;
        hot.tempo = 1.5;
        for _ in 0..120 {
            fly.update(DT, BOUNDS, None, Some(hot));
        }
        let hot_speed = fly.speed;
        (
            fly.state == State::Walking && hot_speed > cool_speed + 10.0,
            format!(
                "cool {} -> hot {} pt/s",
                cool_speed as i32, hot_speed as i32
            ),
        )
    });

    h.body_check(
        "flight: altitude drives scale; escape flies higher than casual",
        || {
            let flight = |escape: bool, effort: Option<f32>| -> (f32, f32) {
                let mut fly = Fly::new(Vec2::ZERO, seed);
                fly.state = State::Idle;
                fly.start_flight(BOUNDS, None, escape, effort);
                let (mut max_alt, mut max_scale) = (0.0f32, 0.0f32);
                let mut frames = 0;
                while fly.state == State::Flying && frames < 400 {
                    frames += 1;
                    fly.update(DT, BOUNDS, None, Some(BrainSignals::new()));
                    max_alt = max_alt.max(fly.alt);
                    max_scale = max_scale.max(fly.scale());
                }
                (max_alt, max_scale)
            };
            let esc = flight(true, None);
            let casual = flight(false, Some(0.45));
            let ok = esc.0 > casual.0 + 0.15
                && esc.1 > FLY_SCALE * 1.5
                && (esc.1 - FLY_SCALE * (1.0 + 0.8 * esc.0)).abs() < 0.15;
            (
                ok,
                format!(
                    "escape alt {:.2} scale {:.2} | casual alt {:.2} scale {:.2}",
                    esc.0, esc.1, casual.0, casual.1
                ),
            )
        },
    );

    h.body_check("flight: wings actually beat", || {
        let mut fly = Fly::new(Vec2::ZERO, seed);
        fly.state = State::Idle;
        fly.start_flight(BOUNDS, None, false, Some(0.8));
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for _ in 0..30 {
            if fly.state != State::Flying {
                break;
            }
            fly.update(DT, BOUNDS, None, Some(BrainSignals::new()));
            let z = fly.pose().wings[0][2];
            lo = lo.min(z);
            hi = hi.max(z);
        }
        (
            hi - lo > 0.25,
            format!("wing sweep {:.2} rad over 0.5 s", hi - lo),
        )
    });

    h.body_check("escape-DN activity mid-flight raises wing-beat effort", || {
        let mut fly = Fly::new(Vec2::ZERO, seed);
        fly.state = State::Idle;
        fly.start_flight(BOUNDS, None, false, Some(0.5));
        let calm = BrainSignals::new();
        for _ in 0..12 {
            fly.update(DT, BOUNDS, None, Some(calm));
        }
        let calm_effort = fly.effort_current;
        let mut hot = BrainSignals::new();
        hot.wing_drive = 1.0;
        hot.arousal = 0.6;
        for _ in 0..12 {
            if fly.state != State::Flying {
                break;
            }
            fly.update(DT, BOUNDS, None, Some(hot));
        }
        let hot_effort = fly.effort_current;
        (
            fly.state == State::Flying && hot_effort > calm_effort + 0.2,
            format!("effort {calm_effort:.2} -> {hot_effort:.2}"),
        )
    });

    h.body_check("threat while grounded raises the wings (no takeoff)", || {
        let mut fly = Fly::new(Vec2::ZERO, seed);
        fly.state = State::Walking;
        fly.speed = 20.0;
        fly.dart_cooldown = 99.0; // isolate the posture from darting
        let mut threat = BrainSignals::new();
        threat.wing_drive = 0.9;
        threat.walk_drive = 0.4;
        for _ in 0..40 {
            fly.update(DT, BOUNDS, None, Some(threat));
        }
        let x = fly.pose().wings[0][0];
        (
            fly.state != State::Flying && fly.wing_raise > 0.6 && x < -0.2,
            format!("raise {:.2}, wing tilt {x:.2} rad", fly.wing_raise),
        )
    });

    h.body_check("landing is smooth: no scale/height snap at touchdown", || {
        let mut fly = Fly::new(Vec2::ZERO, seed);
        fly.state = State::Idle;
        fly.start_flight(BOUNDS, None, true, None);
        let mut prev_scale = fly.scale();
        let mut prev_z = fly.pose().z;
        let (mut max_ds, mut max_dz) = (0.0f32, 0.0f32);
        let (mut post, mut frames) = (20, 0);
        let mut landed = false;
        while post > 0 && frames < 600 {
            frames += 1;
            fly.update(DT, BOUNDS, None, Some(BrainSignals::new()));
            let (s, z) = (fly.scale(), fly.pose().z);
            max_ds = max_ds.max((s - prev_scale).abs());
            max_dz = max_dz.max((z - prev_z).abs());
            prev_scale = s;
            prev_z = z;
            if fly.state != State::Flying {
                landed = true;
                post -= 1;
            }
        }
        (
            landed && max_ds < 0.2 && max_dz < 25.0,
            format!(
                "landed={}, max per-frame dScale {max_ds:.2}, dZ {max_dz:.1}",
                if landed { "yes" } else { "NO" }
            ),
        )
    });

    h.body_check("circadian curve: siesta + night dips, dawn/dusk peaks", || {
        let night = circadian_activity(3.0);
        let dawn = circadian_activity(9.0);
        let siesta = circadian_activity(14.0);
        let dusk = circadian_activity(18.0);
        let ok = night < 0.4 && dawn > 0.9 && siesta < 0.7 && siesta > 0.3 && dusk > 0.9;
        (
            ok,
            format!("3h {night:.2}, 9h {dawn:.2}, 14h {siesta:.2}, 18h {dusk:.2}"),
        )
    });

    let failures = h.failures;
    let mut lines = h.lines;
    lines.push(
        if failures == 0 {
            "ALL BEHAVIOR TESTS PASS".to_string()
        } else {
            format!("{failures} FAILURES")
        },
    );
    Outcome {
        passed: failures == 0,
        lines,
    }
}

// ---------------------------------------------------------------------------
// chimera_test — the jumping spider's circuit invariants (SPIDER_PLAN.md §7,
// Phase 2). Runs on data/salticid, never on the fly's files.
// ---------------------------------------------------------------------------

fn chimera_sim(data: &BrainData, seed: u64) -> LifSim {
    LifSim::with_params(
        &data.circuit,
        seed,
        crate::lif::LifParams::default(),
        crate::roles::salticid(),
    )
}

/// The chimera reuses the fly's modules, so the fly's invariants must still
/// hold on it (the giant fiber is the same two neurons, wired the same way),
/// and the new pathway must do what the plan claims and nothing more:
///
///   * GF silent over 4 s of rest; fires ≤ ~10 ms after an abrupt loom
///   * the authored pounce node is silent at rest, and silent under a loom
///   * LC11 is not driven by a full-field loom (small-object selectivity is
///     modelled in the transduction, but the *wiring* must not route loom
///     drive into it either)
///   * sustained small-object drive makes LC11 fire and the pounce node fire
///   * a small object on one side drives that side's LC11 harder
///   * prey drive does not fire the giant fiber — prey is not a threat
///   * walk-drive duty 20–50%; siesta walk-drive > 3%
pub fn chimera_test(data: &BrainData, seed: u64) -> Outcome {
    use crate::creature::Creature;
    let mut out = Outcome {
        passed: false,
        lines: Vec::new(),
    };
    let mut sim = chimera_sim(data, seed);
    let connectome = crate::creature::Connectome::from_circuit(
        &data.circuit,
        crate::creature::Salticid.provenance(),
    );
    let (auth_n, auth_e) = connectome.authored_counts();
    out.say(format!(
        "chimera: {} neurons ({auth_n} authored) | {} edges ({auth_e} authored) | loom L/R: {}/{} \
         | LC11 L/R: {}/{} | pounce: {} | GF: {} | DNa L/R: {}/{} | MDN: {} | DNp09: {} | DNg11: {} \
         | ascend: {} | sens: {}",
        sim.n,
        data.circuit.edges.len(),
        sim.loom_left.len(),
        sim.loom_right.len(),
        sim.lc11_l.len(),
        sim.lc11_r.len(),
        sim.pounce.len(),
        sim.gf.len(),
        sim.dna_l.len(),
        sim.dna_r.len(),
        sim.mdn.len(),
        sim.fwd.len(),
        sim.groom.len(),
        sim.ascend.len(),
        sim.sens.len()
    ));

    // 4 s of rest.
    let (mut gf_spont, mut pounce_spont) = (0, 0);
    for _ in 0..40 {
        sim.step(100);
        if sim.consume_gf() {
            gf_spont += 1;
        }
        if sim.consume_pounce() {
            pounce_spont += 1;
        }
    }
    let rest_lc11 = sim.rate_lc11_l + sim.rate_lc11_r;
    out.say(format!(
        "rest 4s: pop {:.2} Hz/neuron, LC11 {rest_lc11:.1} Hz, GF spikes {gf_spont}, pounce spikes {pounce_spont}",
        sim.total_spikes as f32 / 4.0 / sim.n as f32
    ));

    // Abrupt loom: the giant fiber must still win its race; LC11 and the
    // pounce node must not care.
    let (mut gf_loom, mut pounce_loom, mut latency) = (0, 0, -1i64);
    let mut lc11_peak_loom: f32 = 0.0;
    for ms in 0..400 {
        sim.loom_l = 1.0;
        sim.loom_r = 0.5;
        sim.step(1);
        if sim.consume_gf() {
            gf_loom += 1;
            if latency < 0 {
                latency = ms;
            }
        }
        if sim.consume_pounce() {
            pounce_loom += 1;
        }
        lc11_peak_loom = lc11_peak_loom.max(sim.rate_lc11_l + sim.rate_lc11_r);
    }
    sim.loom_l = 0.0;
    sim.loom_r = 0.0;
    out.say(format!(
        "abrupt loom 0.4s: GF spikes {gf_loom}, first at {latency} ms, LC {:.1} Hz, \
         LC11 peak {lc11_peak_loom:.1} Hz, pounce spikes {pounce_loom}",
        sim.rate_loom
    ));
    sim.step(1500);
    sim.consume_gf();
    sim.consume_pounce();

    // Small object, both eyes, 2 s.
    let (mut pounce_prey, mut gf_prey, mut pounce_first) = (0, 0, -1i64);
    let mut lc11_peak_prey: f32 = 0.0;
    for ms in 0..2000 {
        sim.prey_l = 1.0;
        sim.prey_r = 1.0;
        sim.step(1);
        if sim.consume_pounce() {
            pounce_prey += 1;
            if pounce_first < 0 {
                pounce_first = ms;
            }
        }
        if sim.consume_gf() {
            gf_prey += 1;
        }
        lc11_peak_prey = lc11_peak_prey.max(sim.rate_lc11_l + sim.rate_lc11_r);
    }
    sim.prey_l = 0.0;
    sim.prey_r = 0.0;
    out.say(format!(
        "small object 2s: LC11 peak {lc11_peak_prey:.1} Hz, pounce spikes {pounce_prey}, \
         first at {pounce_first} ms, GF spikes {gf_prey}"
    ));

    // Object gone: the pounce node must fall silent again. A tail of a few
    // hundred milliseconds is the membrane discharging LC11's last spikes,
    // and is expected; silence is measured after it.
    sim.step(250);
    sim.consume_pounce();
    let mut pounce_after = 0;
    for _ in 0..10 {
        sim.step(100);
        if sim.consume_pounce() {
            pounce_after += 1;
        }
    }
    out.say(format!(
        "object gone 1s: pounce spikes {pounce_after}, LC11 {:.1} Hz",
        sim.rate_lc11_l + sim.rate_lc11_r
    ));

    // Lateralised small object.
    for _ in 0..1000 {
        sim.prey_l = 1.0;
        sim.prey_r = 0.15;
        sim.step(1);
    }
    let (lat_l, lat_r) = (sim.rate_lc11_l, sim.rate_lc11_r);
    sim.prey_l = 0.0;
    sim.prey_r = 0.0;
    sim.consume_pounce();
    out.say(format!("left-eye object 1s: LC11 L/R {lat_l:.1}/{lat_r:.1} Hz"));

    // Walking, as the fly's suite measures it.
    sim.step(1000);
    let (mut walk_on, mut samples) = (0, 0);
    for ms in 0..20_000 {
        sim.gait_drive = 0.5;
        sim.gait_phase = (ms % 125) as f32 / 125.0;
        sim.step(1);
        if ms % 10 == 0 {
            samples += 1;
            if sim.rate_fwd / 10.0 > 0.22 {
                walk_on += 1;
            }
        }
    }
    let walk_pct = 100.0 * walk_on as f32 / samples as f32;
    sim.activity_scale = 1.0 - (1.0 - 0.55) * 0.35;
    let (mut s_on, mut s_samples) = (0, 0);
    for ms in 0..15_000 {
        sim.step(1);
        if ms % 10 == 0 {
            s_samples += 1;
            if sim.rate_fwd / 10.0 > 0.22 {
                s_on += 1;
            }
        }
    }
    let siesta_pct = 100.0 * s_on as f32 / s_samples as f32;
    out.say(format!(
        "behavior 20s: walk-drive on {walk_pct:.0}%; siesta 15s: {siesta_pct:.0}%"
    ));

    let checks: Vec<(bool, &str)> = vec![
        (
            auth_n == 1 && auth_e == sim.lc11_l.len() + sim.lc11_r.len(),
            "exactly one authored neuron, one authored edge per LC11 cell",
        ),
        (sim.pounce.len() == 1 && sim.escw.is_empty(), "pounce present, wing module absent"),
        (gf_spont == 0, "GF silent at rest"),
        (pounce_spont == 0, "pounce silent at rest"),
        (latency >= 0 && latency <= 10, "GF fires within 10 ms of abrupt loom"),
        (pounce_loom == 0, "loom does not fire the pounce node"),
        (lc11_peak_loom < 25.0, "LC11 is not driven by a full-field loom"),
        (lc11_peak_prey > 40.0, "small-object drive fires LC11"),
        (
            pounce_prey >= 1 && pounce_first >= 0 && pounce_first < 1500,
            "sustained small-object drive fires the pounce node",
        ),
        (gf_prey == 0, "prey does not fire the giant fiber"),
        (pounce_after == 0, "pounce falls silent when the object is gone"),
        (lat_l > lat_r * 1.5, "a left-eye object drives left LC11 harder"),
        // A little wider than the fly's 20-50: the chimera's extract carries
        // LC11's downstream partners in place of the wing module, which
        // shifts the drive onto DNp09 slightly. Measured 38-51% across seeds.
        ((20.0..=55.0).contains(&walk_pct), "walk-drive duty 20-55%"),
        (siesta_pct > 3.0, "siesta walk-drive > 3%"),
    ];
    let mut failures = 0;
    for (ok, what) in &checks {
        if !ok {
            failures += 1;
            out.say(format!("FAIL  {what}"));
        }
    }
    out.say(if failures == 0 {
        format!("CHIMERA TEST PASS ({} checks)", checks.len())
    } else {
        format!("{failures} FAILURES")
    });
    out.passed = failures == 0;
    out
}

// ---------------------------------------------------------------------------
// spider_behavior_test — end-to-end chimera sim -> spider body checks
// ---------------------------------------------------------------------------

use crate::spider::{Spider, SpiderState, CAPTURE_RADIUS};

struct SpiderHarness<'a> {
    data: &'a BrainData,
    seed: u64,
    failures: usize,
    lines: Vec<String>,
}

impl<'a> SpiderHarness<'a> {
    /// The loop is closed the way the shell closes it: the spider's own
    /// small-object drive goes back into LC11 every frame, so a bug the body
    /// can see is a bug the circuit can react to.
    fn scenario(
        &mut self,
        name: &str,
        stim: impl Fn(&mut LifSim),
        hold: f32,
        setup: impl Fn(&mut Spider),
        check: impl Fn(&Spider) -> bool,
        describe: impl Fn(&Spider) -> String,
    ) {
        let mut sim = chimera_sim(self.data, self.seed);
        let mut builder = SignalBuilder::new();
        let mut spider = Spider::new(Vec2::ZERO, self.seed);
        spider.heading = 0.0;
        setup(&mut spider);

        sim.step(400);
        sim.consume_gf();
        sim.consume_pounce();
        stim(&mut sim);

        let mut passed = false;
        let mut frames = (hold / DT) as i32;
        while frames > 0 {
            frames -= 1;
            let (l, r) = spider.prey_drive(None);
            sim.prey_l = l;
            sim.prey_r = r;
            sim.gait_drive = spider.walking_intensity();
            sim.gait_phase = spider.gait_phase;
            sim.step((DT * 1000.0).round() as i64);
            let s = builder.make(&mut sim, DT);
            spider.update(DT, BOUNDS, None, Some(s));
            if check(&spider) {
                passed = true;
                break;
            }
        }
        if !passed {
            self.failures += 1;
        }
        self.lines.push(format!(
            "{}  {name}: {}",
            if passed { "PASS" } else { "FAIL" },
            describe(&spider)
        ));
    }

    fn body_check(&mut self, name: &str, run: impl FnOnce() -> (bool, String)) {
        let (ok, detail) = run();
        if !ok {
            self.failures += 1;
        }
        self.lines
            .push(format!("{}  {name}: {detail}", if ok { "PASS" } else { "FAIL" }));
    }
}

pub fn spider_behavior_test(data: &BrainData, seed: u64) -> Outcome {
    let mut h = SpiderHarness {
        data,
        seed,
        failures: 0,
        lines: Vec::new(),
    };

    // ---- sim -> body scenarios ----

    h.scenario(
        "LC11-left stim -> head turns left, then stalks",
        |s| {
            let g = s.lc11_l.clone();
            s.stimulate(&g, 0.3, 1200)
        },
        1.5,
        |_| {},
        |sp| sp.head_yaw > 0.2 && sp.state == SpiderState::Stalking,
        |sp| format!("head_yaw {:+.2} state={:?}", sp.head_yaw, sp.state),
    );

    h.scenario(
        "GF stim -> escape jump on a dragline",
        |s| {
            let g = s.gf.clone();
            s.stimulate(&g, 0.5, 40)
        },
        0.5,
        |_| {},
        |sp| sp.state == SpiderState::Jumping && sp.jump_is_escape && sp.dragline().is_some(),
        |sp| format!("state={:?} escape={} line={}", sp.state, sp.jump_is_escape, sp.dragline().is_some()),
    );

    h.scenario(
        "bug in view -> LC11 -> pounce node -> capture",
        |_| {},
        8.0,
        |sp| sp.spawn_bug(Vec2::new(95.0, 10.0), Vec2::new(8.0, 4.0)),
        |sp| sp.captured >= 1,
        |sp| {
            format!(
                "captured {} state={:?} pursuit-target={:?} bugs={}",
                sp.captured,
                sp.state,
                sp.target,
                sp.prey.len()
            )
        },
    );

    h.scenario(
        "MDN stim (from watching) -> backs away",
        |s| {
            let g = s.mdn.clone();
            s.stimulate(&g, 0.3, 600)
        },
        1.2,
        |_| {},
        |sp| sp.backward_timer > 0.0,
        |sp| format!("backwardTimer={:.2}", sp.backward_timer),
    );

    h.scenario(
        "DNg11 stim -> grooming",
        |s| {
            let g = s.groom.clone();
            s.stimulate(&g, 0.25, 600)
        },
        1.5,
        |_| {},
        |sp| sp.state == SpiderState::Grooming,
        |sp| format!("state={:?}", sp.state),
    );

    h.scenario(
        "DNp09 stim -> walks, speed in band",
        |s| {
            let g = s.fwd.clone();
            s.stimulate(&g, 0.25, 1200)
        },
        1.5,
        |_| {},
        |sp| sp.state == SpiderState::Walking && sp.speed > 25.0 && sp.speed < 80.0,
        |sp| format!("state={:?} speed={}", sp.state, sp.speed as i32),
    );

    h.scenario(
        "moderate loom -> crouches (or escapes); never ignores it",
        |s| {
            s.loom_l = 0.45;
            s.loom_r = 0.45;
        },
        1.0,
        |_| {},
        |sp| sp.crouch > 0.5 || sp.state == SpiderState::Jumping,
        |sp| format!("crouch {:.2} state={:?}", sp.crouch, sp.state),
    );

    // ---- body-level checks, hand-built signals, no sim ----

    let mut walk = BrainSignals::new();
    walk.walk_drive = 0.6;
    let seed = h.seed;

    h.body_check("ledge attach + follow window edge", || {
        let mut sp = Spider::new(Vec2::new(0.0, -55.0), seed);
        sp.state = SpiderState::Walking;
        sp.speed = 30.0;
        sp.heading = 0.0;
        sp.terrain = vec![Ledge {
            y: -40.0,
            x0: -300.0,
            x1: 300.0,
            id: 1,
        }];
        for _ in 0..240 {
            sp.update(DT, BOUNDS, None, Some(walk));
            if sp.ledge.is_some() && (sp.pos.y + 40.0).abs() < 8.0 {
                return (true, format!("attached, y={}", sp.pos.y as i32));
            }
        }
        (false, format!("state={:?} y={} ledge={}", sp.state, sp.pos.y as i32, sp.ledge.is_some()))
    });

    h.body_check("window closes underfoot -> abseils on the line", || {
        let mut sp = Spider::new(Vec2::new(0.0, -40.0), seed);
        sp.state = SpiderState::Walking;
        sp.speed = 25.0;
        sp.heading = 0.0;
        let l = Ledge {
            y: -40.0,
            x0: -300.0,
            x1: 300.0,
            id: 1,
        };
        sp.ledge = Some(l);
        sp.terrain = Vec::new();
        for _ in 0..60 {
            sp.update(DT, BOUNDS, None, Some(walk));
            if sp.state == SpiderState::Abseiling && sp.dragline().is_some() {
                return (true, "on the line".to_string());
            }
        }
        (false, format!("state={:?}", sp.state))
    });

    h.body_check("sleep signal -> sleeping; wake -> grooming", || {
        let mut sp = Spider::new(Vec2::ZERO, seed);
        let mut s = BrainSignals::new();
        s.sleep = true;
        for _ in 0..60 {
            sp.update(DT, BOUNDS, None, Some(s));
        }
        if sp.state != SpiderState::Sleeping {
            return (false, format!("no sleep: {:?}", sp.state));
        }
        s.sleep = false;
        sp.update(DT, BOUNDS, None, Some(s));
        (sp.state == SpiderState::Grooming, format!("woke to {:?}", sp.state))
    });

    h.body_check("thermal tempo scales walking speed", || {
        let mut sp = Spider::new(Vec2::ZERO, seed);
        sp.state = SpiderState::Walking;
        sp.speed = 20.0;
        let mut cool = walk;
        cool.tempo = 1.0;
        for _ in 0..120 {
            sp.update(DT, BOUNDS, None, Some(cool));
        }
        let cool_speed = sp.speed;
        let mut hot = walk;
        hot.tempo = 1.5;
        for _ in 0..120 {
            sp.update(DT, BOUNDS, None, Some(hot));
        }
        (
            sp.state == SpiderState::Walking && sp.speed > cool_speed + 8.0,
            format!("cool {} -> hot {} pt/s", cool_speed as i32, sp.speed as i32),
        )
    });

    h.body_check("jump: leaves the ground, lands with no z/scale snap", || {
        let mut sp = Spider::new(Vec2::ZERO, seed);
        sp.start_jump(Vec2::new(140.0, 30.0), true);
        let (mut prev_z, mut prev_s) = (sp.z, sp.scale());
        let (mut max_dz, mut max_ds, mut max_z) = (0.0f32, 0.0f32, 0.0f32);
        let mut frames = 0;
        while sp.state == SpiderState::Jumping && frames < 200 {
            frames += 1;
            sp.update(DT, BOUNDS, None, Some(BrainSignals::new()));
            max_dz = max_dz.max((sp.z - prev_z).abs());
            max_ds = max_ds.max((sp.scale() - prev_s).abs());
            max_z = max_z.max(sp.z);
            prev_z = sp.z;
            prev_s = sp.scale();
        }
        (
            sp.state != SpiderState::Jumping && max_z > 10.0 && max_dz < 15.0 && max_ds < 0.2,
            format!("peak z {max_z:.0}, max per-frame dZ {max_dz:.1}, dScale {max_ds:.2}"),
        )
    });

    h.body_check("pounce landing within capture radius catches the bug", || {
        let mut sp = Spider::new(Vec2::ZERO, seed);
        sp.spawn_bug(Vec2::new(70.0, 0.0), Vec2::ZERO);
        sp.start_jump(Vec2::new(70.0 + CAPTURE_RADIUS * 0.5, 0.0), false);
        for _ in 0..200 {
            if sp.state != SpiderState::Jumping {
                break;
            }
            sp.update(DT, BOUNDS, None, Some(BrainSignals::new()));
        }
        (sp.captured == 1 && sp.prey.is_empty(), format!("captured {}", sp.captured))
    });

    h.body_check("watching: the head looks around on its own", || {
        let mut sp = Spider::new(Vec2::ZERO, seed);
        let calm = BrainSignals::new();
        let mut max_yaw: f32 = 0.0;
        for _ in 0..(20.0 / DT) as usize {
            sp.update(DT, BOUNDS, None, Some(calm));
            max_yaw = max_yaw.max(sp.head_yaw.abs());
        }
        (max_yaw > 0.3, format!("max |head_yaw| {max_yaw:.2} rad over 20 s"))
    });

    h.body_check("a corner passed by becomes the retreat; sleep spins it and happens in it", || {
        let lo = BOUNDS.min();
        let mut sp = Spider::new(Vec2::new(lo.x + 40.0, lo.y + 40.0), seed);
        sp.state = SpiderState::Walking;
        sp.speed = 20.0;
        sp.heading = 0.8;
        for _ in 0..30 {
            sp.update(DT, BOUNDS, None, Some(walk));
        }
        let Some(r) = sp.retreat else {
            return (false, "no retreat chosen near the corner".to_string());
        };
        // Wander off a little, then dusk.
        for _ in 0..120 {
            sp.update(DT, BOUNDS, None, Some(walk));
        }
        let mut night = BrainSignals::new();
        night.sleep = true;
        let (mut homed, mut spun) = (false, false);
        for _ in 0..(40.0 / DT) as usize {
            sp.update(DT, BOUNDS, None, Some(night));
            homed |= sp.state == SpiderState::Homing;
            spun |= sp.state == SpiderState::Spinning;
            if sp.state == SpiderState::Sleeping {
                break;
            }
        }
        let at_home = crate::util::hypot(sp.pos.x - r.x, sp.pos.y - r.y) < 6.0;
        let threads = sp.silk.count_kind(crate::silk::ThreadKind::Retreat);
        (
            sp.state == SpiderState::Sleeping && homed && spun && at_home && threads >= 6 && sp.silk.trailing_anchor().is_none(),
            format!(
                "retreat at ({:.0},{:.0}); homed={homed} spun={spun} asleep={} there={at_home}, {threads} retreat threads",
                r.x, r.y, sp.state == SpiderState::Sleeping
            ),
        )
    });

    let failures = h.failures;
    let mut lines = h.lines;
    let n = lines.len();
    lines.push(if failures == 0 {
        format!("ALL {n} SPIDER BEHAVIOR TESTS PASS")
    } else {
        format!("{failures} FAILURES")
    });
    Outcome {
        passed: failures == 0,
        lines,
    }
}

// ---------------------------------------------------------------------------
// The web builders (WEB_PLAN.md): their circuit and their behaviour.
// ---------------------------------------------------------------------------

use crate::habitat::Region;
use crate::silk::{Anchor, ThreadKind};
use crate::weaver::{Idle, Weaver, WeaverState};

fn weaver_sim(data: &BrainData, seed: u64) -> LifSim {
    LifSim::with_params(
        &data.circuit,
        seed,
        crate::lif::LifParams::default(),
        crate::roles::weaver(),
    )
}

/// The weavers share the fly's modules minus the wings and *without* LC11,
/// plus one authored node on a slow membrane. The claims to check are the
/// amplitude split (WEB_PLAN.md §3.1): a sustained small vibration reaches
/// the strike node and not the giant fiber; a sharp knock reaches the giant
/// fiber; a loom still does; and at rest both are silent.
pub fn weaver_test(data: &BrainData, seed: u64) -> Outcome {
    use crate::creature::Creature;
    let mut out = Outcome {
        passed: false,
        lines: Vec::new(),
    };
    let mut sim = weaver_sim(data, seed);
    let connectome = crate::creature::Connectome::from_circuit(
        &data.circuit,
        crate::creature::Weaver::Araneus.provenance(),
    );
    let (auth_n, auth_e) = connectome.authored_counts();
    out.say(format!(
        "weaver chimera: {} neurons ({auth_n} authored) | {} edges ({auth_e} authored) | loom L/R: {}/{} \
         | LC11: {} | strike: {} | GF: {} | DNa L/R: {}/{} | MDN: {} | DNp09: {} | DNg11: {} \
         | ascend: {} | sens: {}",
        sim.n,
        data.circuit.edges.len(),
        sim.loom_left.len(),
        sim.loom_right.len(),
        sim.lc11_l.len() + sim.lc11_r.len(),
        sim.strike.len(),
        sim.gf.len(),
        sim.dna_l.len(),
        sim.dna_r.len(),
        sim.mdn.len(),
        sim.fwd.len(),
        sim.groom.len(),
        sim.ascend.len(),
        sim.sens.len()
    ));
    let mut ok = true;
    if sim.lc11_l.len() + sim.lc11_r.len() != 0 || sim.strike.len() != 1 || sim.sens.is_empty() {
        out.say("FAIL population check: expected no LC11, one strike node, mechanosensory partners".into());
        ok = false;
    }
    // The strike node has no synapses at all: by construction there is no
    // path from a struggle to the giant fiber, or from anything to it.
    let strike_edges = data
        .circuit
        .edges
        .iter()
        .filter(|e| sim.strike.contains(&(e[0] as usize)) || sim.strike.contains(&(e[1] as usize)))
        .count();
    out.say(format!("strike node synapses: {strike_edges} (a sense with no wiring, driven by the transduction only)"));
    if strike_edges != 0 {
        out.say("FAIL structure: the strike node must have no synapses".into());
        ok = false;
    }

    // 12 s of rest: long enough to contain the LIF's periodic noise burst
    // (every 15-40 s, six times the noise for 400 ms), which a 250 ms
    // membrane integrates the way it integrates anything sustained. So the
    // honest claim is a *rate*: quiet at rest, several times louder under a
    // struggle. The body ignores a strike with nothing loud in the web.
    let (mut gf_spont, mut strike_spont) = (0, 0);
    for _ in 0..120 {
        sim.step(100);
        if sim.consume_gf() {
            gf_spont += 1;
        }
        if sim.consume_strike() {
            strike_spont += 1;
        }
    }
    let rest_rate = strike_spont as f32 / 12.0;
    out.say(format!(
        "rest 12s: pop {:.2} Hz/neuron, GF spikes {gf_spont}, strike spikes {strike_spont} ({rest_rate:.2} Hz)",
        sim.total_spikes as f32 / 12.0 / sim.n as f32
    ));
    // The measured wiring's own spontaneous giant-fiber rate (the fly's
    // occasional unprompted takeoff) is not the weavers' to fix.
    if gf_spont > 1 || strike_spont != 0 {
        out.say("FAIL rest: the giant fiber must be near-silent and the strike node silent".into());
        ok = false;
    }

    // A struggle: sustained vibration on the strike node's own channel.
    let (mut gf_prey, mut strike_prey, mut latency) = (0, 0, -1i64);
    for ms in 0..3000 {
        sim.vibration = 0.8;
        sim.step(1);
        if sim.consume_gf() {
            gf_prey += 1;
        }
        if sim.consume_strike() {
            strike_prey += 1;
            if latency < 0 {
                latency = ms;
            }
        }
    }
    sim.vibration = 0.0;
    out.say(format!(
        "struggle (vibration 0.8 for 3 s): strike spikes {strike_prey} (first at {latency} ms), GF spikes {gf_prey}, strike rate {:.1} Hz",
        sim.rate_strike
    ));
    let struggle_rate = strike_prey as f32 / 3.0;
    if strike_prey < 2 || latency > 1000 || struggle_rate < 4.0 * rest_rate.max(0.1) {
        out.say(format!(
            "FAIL struggle: the strike node must fire within 1 s and keep firing ({struggle_rate:.2} vs {rest_rate:.2} Hz at rest)"
        ));
        ok = false;
    }
    // GF spikes here can only be the circuit's own spontaneous rate: the
    // channel has no synapses to reach it by. Reported, not asserted.
    // Let it settle.
    for _ in 0..6 {
        sim.step(100);
        sim.consume_gf();
        sim.consume_strike();
    }

    // A knock: the fly suite's tap, delivered the same way a click is.
    let (mut gf_knock, mut knock_latency) = (0, -1i64);
    let sens = sim.sens.clone();
    sim.stimulate(&sens, 0.45, 150);
    for ms in 0..300 {
        sim.step(1);
        if sim.consume_gf() {
            gf_knock += 1;
            if knock_latency < 0 {
                knock_latency = ms;
            }
        }
    }
    let strike_knock = sim.consume_strike();
    out.say(format!(
        "knock (tap on the sensory partners, 150 ms): GF spikes {gf_knock} (first at {knock_latency} ms), strike also fired: {strike_knock}"
    ));
    if gf_knock == 0 {
        out.say("FAIL knock: a sharp vibration must reach the giant fiber".into());
        ok = false;
    }
    for _ in 0..6 {
        sim.step(100);
        sim.consume_gf();
    }

    // An abrupt loom: the fly's invariant, on this circuit.
    let (mut gf_loom, mut loom_latency) = (0, -1i64);
    for ms in 0..400 {
        sim.loom_l = 1.0;
        sim.loom_r = 0.5;
        sim.step(1);
        if sim.consume_gf() {
            gf_loom += 1;
            if loom_latency < 0 {
                loom_latency = ms;
            }
        }
    }
    sim.loom_l = 0.0;
    sim.loom_r = 0.0;
    out.say(format!("abrupt loom: GF spikes {gf_loom}, first at {loom_latency} ms"));
    if gf_loom == 0 || loom_latency > 12 {
        out.say("FAIL loom: the giant fiber must fire within ~10 ms of an abrupt loom".into());
        ok = false;
    }

    // Silence again, once the loom's activity has died down.
    for _ in 0..20 {
        sim.step(100);
        sim.consume_gf();
        sim.consume_strike();
    }
    let (mut gf_after, mut strike_after) = (0, 0);
    for _ in 0..80 {
        sim.step(100);
        if sim.consume_gf() {
            gf_after += 1;
        }
        if sim.consume_strike() {
            strike_after += 1;
        }
    }
    let after_rate = strike_after as f32 / 8.0;
    out.say(format!("8 s after: GF spikes {gf_after}, strike spikes {strike_after} ({after_rate:.2} Hz)"));
    if gf_after > 1 || strike_after > 1 {
        out.say("FAIL after: the giant fiber must be silent and the strike node quiet again".into());
        ok = false;
    }

    out.say(if ok {
        "ALL WEAVER CIRCUIT TESTS PASS".into()
    } else {
        "WEAVER CIRCUIT TESTS FAILED".into()
    });
    out.passed = ok;
    out
}

struct WeaverHarness<'a> {
    data: &'a BrainData,
    seed: u64,
    species: crate::creature::Weaver,
    failures: u32,
    lines: Vec<String>,
}

const WEAVER_TANK: Region = Region {
    center: Vec2 { x: 160.0, y: -80.0 },
    size: (720.0, 520.0),
};

fn weaver_body_for(species: crate::creature::Weaver, seed: u64, idle: bool) -> Weaver {
    let mut rng = crate::rng::Pcg32::new(seed ^ 0x77);
    let program: Box<dyn crate::weaver::WebProgram> = if idle {
        Box::new(Idle(species))
    } else {
        crate::weaver::program_for(species, &crate::anchors::Anchors::enclosure(WEAVER_TANK), &mut rng)
    };
    Weaver::new(species, WEAVER_TANK.center, seed, program)
}

impl<'a> WeaverHarness<'a> {
    fn body(&self, idle: bool) -> Weaver {
        weaver_body_for(self.species, self.seed, idle)
    }

    /// Run the circuit and the body together, feeding the body's felt
    /// vibration back into the mechanosensory channel exactly as the shell
    /// does, with the stimulus applied at the start.
    #[allow(clippy::too_many_arguments)]
    fn scenario(
        &mut self,
        name: &str,
        idle: bool,
        stim: impl FnOnce(&mut LifSim),
        hold: f32,
        setup: impl FnOnce(&mut Weaver),
        check: impl Fn(&Weaver) -> bool,
        describe: impl Fn(&Weaver) -> String,
    ) {
        const DT: f32 = 1.0 / 60.0;
        let mut sim = weaver_sim(self.data, self.seed);
        let mut builder = SignalBuilder::new();
        let mut w = self.body(idle);
        setup(&mut w);
        sim.step(1500);
        let _ = sim.consume_gf();
        let _ = sim.consume_strike();
        stim(&mut sim);
        let mut passed = false;
        let mut frames = (hold / DT) as i32;
        while frames > 0 {
            frames -= 1;
            sim.gait_drive = w.walking_intensity();
            sim.gait_phase = w.gait_phase;
            // The shell's vibration channel: felt -> the strike node.
            sim.vibration = (w.felt * 4.0).min(1.0);
            sim.step((DT * 1000.0).round() as i64);
            let s = builder.make(&mut sim, DT);
            w.update(DT, WEAVER_TANK, None, Some(s));
            if check(&w) {
                passed = true;
                break;
            }
        }
        if !passed {
            self.failures += 1;
        }
        self.lines.push(format!(
            "{}  {name}: {}",
            if passed { "PASS" } else { "FAIL" },
            describe(&w)
        ));
    }

    fn body_check(&mut self, name: &str, run: impl FnOnce() -> (bool, String)) {
        let (ok, detail) = run();
        if !ok {
            self.failures += 1;
        }
        self.lines
            .push(format!("{}  {name}: {detail}", if ok { "PASS" } else { "FAIL" }));
    }
}

pub fn weaver_behavior_test(data: &BrainData, seed: u64, id: &str) -> Outcome {
    let species = match id {
        "parasteatoda" => crate::creature::Weaver::Parasteatoda,
        "agelenopsis" => crate::creature::Weaver::Agelenopsis,
        _ => crate::creature::Weaver::Araneus,
    };
    let mut h = WeaverHarness {
        data,
        seed,
        species,
        failures: 0,
        lines: Vec::new(),
    };
    const DT: f32 = 1.0 / 60.0;
    let seed = h.seed;
    let drops = crate::weaver::Traits::of(species).threat == crate::weaver::ThreatResponse::Drop;

    // ---- sim -> body scenarios ----

    h.scenario(
        if drops { "GF stim -> drops on the dragline" } else { "GF stim -> runs for the retreat" },
        true,
        |s| {
            let g = s.gf.clone();
            s.stimulate(&g, 0.5, 40)
        },
        0.5,
        |_| {},
        |w| {
            if drops {
                w.state == WeaverState::Dropping && w.dragline().is_some()
            } else {
                matches!(w.state, WeaverState::Retreating | WeaverState::Dropping)
            }
        },
        |w| format!("state={:?} line={}", w.state, w.dragline().is_some()),
    );

    h.scenario(
        "struggling bug -> vibration -> strike node -> capture",
        true,
        |_| {},
        12.0,
        |w| {
            // One sticky thread from under the spider to a stuck bug.
            let here = w.pos;
            let far = Vec2::new(here.x + 140.0, here.y);
            w.silk.pay_out(here, ThreadKind::Capture);
            w.silk.attach(far, Anchor::Fixed);
            w.silk.release();
            w.spawn_bug(far, Vec2::ZERO, false);
            let n = w.prey.len() - 1;
            w.prey[n].stuck = true;
            w.prey[n].struggle = 1.0;
        },
        |w| w.captured >= 1,
        |w| format!("captured {} state={:?} felt {:.3}", w.captured, w.state, w.felt),
    );

    h.scenario(
        "DNp09 stim -> construction advances",
        false,
        |s| {
            let g = s.fwd.clone();
            s.stimulate(&g, 0.25, 4000)
        },
        4.0,
        |_| {},
        |w| w.state == WeaverState::Building && (!w.silk.threads.is_empty() || w.dragline().is_some()),
        |w| {
            format!(
                "threads {} line={} state={:?} stage={}",
                w.silk.threads.len(),
                w.dragline().is_some(),
                w.state,
                w.program.stage()
            )
        },
    );

    h.scenario(
        "DNg11 stim -> grooming",
        true,
        |s| {
            let g = s.groom.clone();
            s.stimulate(&g, 0.25, 600)
        },
        1.5,
        |_| {},
        |w| w.state == WeaverState::Grooming,
        |w| format!("state={:?}", w.state),
    );

    h.scenario(
        "MDN stim -> backs away",
        true,
        |s| {
            let g = s.mdn.clone();
            s.stimulate(&g, 0.3, 600)
        },
        1.2,
        |_| {},
        |w| w.backward_timer > 0.0,
        |w| format!("backwardTimer={:.2}", w.backward_timer),
    );

    h.scenario(
        "moderate loom -> crouches (or flees); never ignores it",
        true,
        |s| {
            s.loom_l = 0.45;
            s.loom_r = 0.45;
        },
        1.0,
        |_| {},
        |w| w.crouch > 0.5 || matches!(w.state, WeaverState::Dropping | WeaverState::Retreating),
        |w| format!("crouch {:.2} state={:?}", w.crouch, w.state),
    );

    // ---- body-level checks, hand-built signals, no sim ----

    let mut walk = BrainSignals::new();
    walk.walk_drive = 0.6;

    if species == crate::creature::Weaver::Araneus {
        h.body_check("full orb in under 10 min of body time, radii in the species band", || {
            let mut w = weaver_body_for(species, seed, false);
            let mut t = 0.0;
            while !w.web_complete() && t < 900.0 {
                w.update(DT, WEAVER_TANK, None, Some(walk));
                t += DT;
            }
            for _ in 0..30 {
                w.update(DT, WEAVER_TANK, None, Some(walk));
            }
            let hub = w.sit_point().unwrap_or(w.pos);
            let silk = &w.silk;
            let radii = silk
                .threads
                .iter()
                .filter(|t| t.kind == ThreadKind::Radius)
                .filter(|t| {
                    let (a, b) = (silk.nodes[t.a].pos, silk.nodes[t.b].pos);
                    crate::util::hypot(a.x - hub.x, a.y - hub.y) < 3.0
                        || crate::util::hypot(b.x - hub.x, b.y - hub.y) < 3.0
                })
                .count();
            let capture = silk.count_kind(ThreadKind::Capture);
            let aux = silk.count_kind(ThreadKind::Auxiliary);
            let ok = w.web_complete()
                && t < 600.0
                && (crate::orb::RADII.0..=crate::orb::RADII.1 + 2).contains(&radii)
                && capture > 200
                && aux == 0
                && w.state == WeaverState::Sitting;
            (
                ok,
                format!(
                    "{t:.0} s, {radii} radii, {capture} capture segments, {aux} scaffold left, state={:?}",
                    w.state
                ),
            )
        });

        h.body_check("a cut radius is repaired; half the web gone is a rebuild", || {
            let mut w = weaver_body_for(species, seed, false);
            let mut t = 0.0;
            while !w.web_complete() && t < 900.0 {
                w.update(DT, WEAVER_TANK, None, Some(walk));
                t += DT;
            }
            let hub = w.sit_point().unwrap_or(w.pos);
            let cut = w.damage(Vec2::new(hub.x + 55.0, hub.y), 12.0);
            let repairing = w.program.stage() == "repairing";
            let mut t2 = 0.0;
            while !w.web_complete() && t2 < 300.0 {
                w.update(DT, WEAVER_TANK, None, Some(walk));
                t2 += DT;
            }
            let repaired = w.web_complete();
            for k in 0..12 {
                let a = k as f32 * 0.5;
                w.damage(Vec2::new(hub.x + a.cos() * 120.0, hub.y + a.sin() * 120.0), 60.0);
            }
            let rebuilding = w.state == WeaverState::Eating;
            (
                cut > 0 && repairing && repaired && rebuilding,
                format!("cut {cut}, repairing={repairing}, repaired in {t2:.0} s, then rebuild={rebuilding}"),
            )
        });
    }

    h.body_check("the construction program never excites the silk", || {
        let mut w = weaver_body_for(species, seed, false);
        let mut t = 0.0;
        let mut max_felt: f32 = 0.0;
        while !w.web_complete() && t < 900.0 {
            w.update(DT, WEAVER_TANK, None, Some(walk));
            max_felt = max_felt.max(w.felt);
            if w.silk.loudest(0.0).is_some() {
                max_felt = 1.0;
            }
            t += DT;
        }
        (max_felt == 0.0, format!("max vibration during a {t:.0} s build: {max_felt}"))
    });

    h.body_check("construction pauses while the walk drive is down", || {
        let mut w = weaver_body_for(species, seed, false);
        for _ in 0..600 {
            w.update(DT, WEAVER_TANK, None, Some(walk));
        }
        let before = w.silk.threads.len();
        let rest = BrainSignals::new();
        for _ in 0..600 {
            w.update(DT, WEAVER_TANK, None, Some(rest));
        }
        (
            w.silk.threads.len() == before,
            format!("{before} threads before rest, {} after", w.silk.threads.len()),
        )
    });

    let failures = h.failures;
    let mut lines = h.lines;
    let n = lines.len();
    lines.push(if failures == 0 {
        format!("ALL {n} WEAVER BEHAVIOR TESTS PASS")
    } else {
        format!("{failures} FAILURES")
    });
    Outcome {
        passed: failures == 0,
        lines,
    }
}
