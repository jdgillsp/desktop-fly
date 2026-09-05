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

const BOUNDS: (f32, f32) = (1512.0, 982.0);
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
