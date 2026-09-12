//! Reproducible diagnostic audit, separate from pass/fail regression tests.
//! Run with --ignored --nocapture; findings are reported, never silently blessed.
use dfcore::anchors::{Anchors, Frame};
use dfcore::silk::Anchor;
use dfcore::{Habitat, HabitatKind, Region, Vec2, Weaver as Species, WeaverBody, WeaverState};
use serde_json::json;

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn physics_probes() -> serde_json::Value {
    use dfcore::silk::{Node, Silk, ThreadKind};
    fn bridge() -> Silk {
        let mut s = Silk::new();
        s.pay_out(Vec2::new(-30.0, 0.0), ThreadKind::Frame);
        s.attach(Vec2::ZERO, Anchor::Free);
        s.attach(Vec2::new(30.0, 0.0), Anchor::Fixed);
        s.release();
        s
    }
    let locate = |n: &Node| [n.pos.x, n.pos.y, 40.0];
    let mut heights = Vec::new();
    for fps in [30, 60, 120] {
        let mut s = bridge();
        for _ in 0..fps * 4 {
            s.step_spatial(1.0 / fps as f32, [0.0, 0.0, -28.0], 0.0, locate);
        }
        heights.push(s.nodes[1].spatial.unwrap().pos[2]);
    }
    let mut s = bridge();
    for _ in 0..240 {
        s.step_spatial(1.0 / 60.0, [0.0, 0.0, -28.0], 0.0, locate);
    }
    let pins_correct = s.nodes[0].spatial.unwrap().pos == [-30.0, 0.0, 40.0]
        && s.nodes[2].spatial.unwrap().pos == [30.0, 0.0, 40.0];
    let tension_before: Vec<_> = s.threads.iter().map(|t| t.tension).collect();
    // Stretch both supports while retaining thread identity.
    for _ in 0..120 {
        s.step_spatial(1.0 / 60.0, [0.0, 0.0, -28.0], 0.0, |n| {
            [n.pos.x * 1.5, n.pos.y, 40.0]
        });
    }
    let tension_after: Vec<_> = s.threads.iter().map(|t| t.tension).collect();
    let stretch_changes_tension = tension_before != tension_after;
    for n in &mut s.nodes {
        n.anchor = Anchor::Free;
    }
    for _ in 0..360 {
        s.step_spatial(1.0 / 60.0, [0.0, 0.0, -28.0], 0.0, locate);
    }
    let released_falls = s.nodes.iter().all(|n| n.spatial.unwrap().pos[2] < 1.0);
    let min = heights.iter().copied().fold(f32::INFINITY, f32::min);
    let max = heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    json!({"pinned_endpoints_stay_fixed":pins_correct,"released_silk_falls":released_falls,
        "sag_heights_at_30_60_120_fps":heights,"frame_rate_spread":max-min,
        "tension_before_stretch":tension_before,"tension_after_50_percent_support_stretch":tension_after,
        "stretch_changes_tension":stretch_changes_tension})
}

#[test]
#[ignore = "diagnostic audit: reports defects without treating completion as correctness"]
fn audit_construction_supports_and_movement() {
    run_construction_audit(false);
}

#[test]
fn construction_movement_and_deposition_regression() {
    run_construction_audit(true);
}

fn run_construction_audit(strict: bool) {
    let mut rows = Vec::new();
    for scene in ["bare_habitat", "furnished_habitat", "desktop_controls"] {
        for species in [
            Species::Araneus,
            Species::Parasteatoda,
            Species::Agelenopsis,
        ] {
            for seed in [1, 5, 12] {
                eprintln!("auditing {scene} {species:?} seed {seed}");
                let row = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let desktop = scene == "desktop_controls";
                    let region = Region::centered(if desktop {
                        (1280.0, 900.0)
                    } else {
                        (520.0, 420.0)
                    });
                    let mut h = Habitat::new(HabitatKind::Vivarium, region, seed);
                    if scene == "bare_habitat" {
                        h.props.clear();
                    }
                    let frames = vec![
                        Frame::of(Region::new(Vec2::new(-220.0, 240.0), (160.0, 40.0)), 101),
                        Frame::of(Region::new(Vec2::new(180.0, 60.0), (200.0, 100.0)), 102),
                        Frame::of(Region::new(Vec2::new(-160.0, -220.0), (280.0, 48.0)), 103),
                        Frame::of(Region::centered((1160.0, 800.0)), 77),
                    ];
                    let anchors = if desktop {
                        Anchors::desktop(region, region, &frames)
                    } else {
                        Anchors::habitat(&h)
                    };
                    let mut rng = dfcore::rng::Pcg32::new(seed);
                    let mut w = WeaverBody::new(
                        species,
                        Vec2::ZERO,
                        seed,
                        dfcore::weaver::program_for(species, &anchors, &mut rng),
                    );
                    w.desktop_mode = desktop;
                    if desktop {
                        w.frames = frames;
                    } else {
                        w.habitat_world = Some(anchors);
                    }
                    let mut jumps = 0;
                    let mut max_jump = 0.0f32;
                    let mut knot_gaps = 0;
                    let mut max_gap = 0.0f32;
                    let mut max_pin_relocation = 0.0f32;
                    let mut unsupported_travel = 0;
                    let mut build_frames = 0;
                    let mut first_jump = None;
                    let mut prev = None;
                    let dt = 1.0 / 30.0;
                    let mut elapsed = 0.0;
                    for _ in 0..27_000 {
                        let old_chart = w.pos;
                        let old_nodes = w.silk.nodes.len();
                        w.update(dt, region, None, None);
                        let new_knots: Vec<_> = w
                            .silk
                            .nodes
                            .iter()
                            .enumerate()
                            .skip(old_nodes)
                            .filter(|(_, n)| {
                                dfcore::util::hypot(n.pos.x - w.pos.x, n.pos.y - w.pos.y) < 2.0
                            })
                            .filter_map(|(_, n)| n.spatial.map(|s| s.pos))
                            .collect();
                        let deposition_point = w.spatial_pos;
                        let new_pins:Vec<_>=w.silk.nodes.iter().skip(old_nodes)
                            .filter(|n|n.anchor==Anchor::Fixed).copied().collect();
                        crate::webspace::step(&mut w, if desktop { None } else { Some(&h) }, dt);
                        for pin in new_pins {
                            if let (Some(before),Some(after))=(pin.spatial,w.silk.nodes.iter()
                                .find(|n|n.pos==pin.pos && n.on==pin.on && n.anchor==Anchor::Fixed).and_then(|n|n.spatial)) {
                                max_pin_relocation=max_pin_relocation.max(distance(before.pos,after.pos));
                            }
                        }
                        let physical = w.spatial_pos.unwrap();
                        if matches!(w.state, WeaverState::Building) {
                            build_frames += 1;
                            if let Some(before) = prev {
                                let jump = distance(before, physical);
                                let chart_delta = dfcore::util::hypot(
                                    w.pos.x - old_chart.x,
                                    w.pos.y - old_chart.y,
                                );
                                max_jump = max_jump.max(jump);
                                // Five times maximum construction advance, allowing web motion.
                                if jump > 25.0 && chart_delta <= 5.1 {
                                    jumps += 1;
                                    if first_jump.is_none() {
                                        first_jump = Some(
                                            json!({"seconds":elapsed,"stage":w.program.stage(),"distance":jump,"chart_distance":chart_delta,"from":before,"to":physical}),
                                        );
                                    }
                                }
                            }
                            for knot in new_knots {
                                let gap = distance(knot, deposition_point.unwrap_or(physical));
                                max_gap = max_gap.max(gap);
                                if gap > 12.0 {
                                    knot_gaps += 1;
                                }
                            }
                            if w.speed > 1.0
                                && w.silk.trailing_anchor().is_none()
                                && w.silk.thread_near(w.pos, None, 12.0).is_none()
                                && w.anchors().on(w.pos, 18.0).is_none()
                            {
                                unsupported_travel += 1;
                            }
                        }
                        prev = Some(physical);
                        elapsed += dt;
                        if w.web_complete() {
                            break;
                        }
                    }
                    let mut connected: Vec<bool> = w
                        .silk
                        .nodes
                        .iter()
                        .map(|n| n.anchor == Anchor::Fixed)
                        .collect();
                    loop {
                        let mut changed = false;
                        for t in &w.silk.threads {
                            if connected[t.a] != connected[t.b] {
                                connected[t.a] = true;
                                connected[t.b] = true;
                                changed = true;
                            }
                        }
                        if !changed {
                            break;
                        }
                    }
                    let detached = w.silk.threads.iter().filter(|t| !connected[t.a]).count();
                    json!({"scene":scene,"species":format!("{species:?}"),"seed":seed,
                    "complete":w.web_complete(),"stage":w.program.stage(),"seconds":elapsed,
                    "threads":w.silk.threads.len(),"detached_threads":detached,
                    "fixed_without_support":w.silk.nodes.iter().filter(|n|n.anchor==Anchor::Fixed && n.on.is_none()).count(),
                    "physical_jumps_over_25":jumps,"max_physical_step":max_jump,"first_jump":first_jump,
                    "knots_over_12_from_spider":knot_gaps,"max_knot_gap":max_gap,
                    "max_pin_relocation":max_pin_relocation,
                    "unattached_travel_frames":unsupported_travel,"building_frames":build_frames,
                    "zero_length_threads":w.silk.threads.iter().filter(|t|t.rest_len<0.01).count()})
                }));
                rows.push(row.unwrap_or_else(|_|json!({"scene":scene,"species":format!("{species:?}"),"seed":seed,"panic":true})));
            }
        }
    }
    let report = json!({"dt":1.0/30.0,"note":"This is a diagnostic report, not a passing correctness verdict. Unattached travel is a screening metric, not a full 3D collision verdict. Physical jumps and knot gaps are measured in rendered coordinates.","physics":physics_probes(),"runs":rows});
    if let Some(path) = std::env::var_os("DESKTOPFLY_WEB_AUDIT") {
        std::fs::write(path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }
    if strict {
        for row in report["runs"].as_array().unwrap() {
            assert_eq!(row["complete"], true, "{row}");
            assert!(row["max_physical_step"].as_f64().unwrap() <= 5.01, "{row}");
            assert!(row["max_knot_gap"].as_f64().unwrap() <= 0.26, "{row}");
            assert!(row["max_pin_relocation"].as_f64().unwrap() <= 0.26,"{row}");
            assert_eq!(row["fixed_without_support"], 0, "{row}");
        }
        for probe in [
            "pinned_endpoints_stay_fixed",
            "released_silk_falls",
            "stretch_changes_tension",
        ] {
            assert_eq!(report["physics"][probe], true, "{probe}");
        }
        assert!(report["physics"]["frame_rate_spread"].as_f64().unwrap() < 0.05);
    }
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
