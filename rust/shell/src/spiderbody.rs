//! The jumping spider's geometry (SPIDER_PLAN.md §5, Phase 5).
//!
//! Same convention as the fly: the model faces +y, z is up, and the body
//! computes angles while this file turns them into capsules and spheres.
//! Nothing here decides anything.
//!
//! Glass is the register: a soft translucent cephalothorax and abdomen with
//! the chimera's circuit lit inside, the two principal eyes as the one place
//! the glass is denser. The literal register is a plain dark salticid — hairy
//! it is not, at forty pixels nobody could tell, and it is the register that
//! fails the creep filter, so it is deliberately the less finished of the two.

use dfcore::{LifSim, Origin, Spider, SpiderPose, SpiderState, ThreadKind};

use crate::brain::AUTHORED_COLOR;
use crate::flybody::{finish, setae, tapered_limb, GlassPalette, NeuronLayout};
use crate::math::{self, Mat4};
use crate::mesh::{self, Material, Mesh, Vertex};

/// The literal register is a **bold jumping spider**, *Phidippus audax*: a
/// black velvet body, one large white spot and two small ones on the abdomen,
/// pale bands at the leg joints, iridescent green chelicerae, and big glossy
/// front eyes with pale tufts above them. Fur does not read at forty pixels;
/// the markings do, and they are what makes it *that* spider.
const BODY_DARK: [f32; 4] = [0.10, 0.09, 0.10, 1.0];
const BODY_MID: [f32; 4] = [0.13, 0.12, 0.13, 1.0];
const LEG_DARK: [f32; 4] = [0.11, 0.10, 0.11, 1.0];
const LEG_BAND: [f32; 4] = [0.78, 0.76, 0.70, 1.0];
const SPOT_WHITE: [f32; 4] = [0.92, 0.90, 0.84, 1.0];
const TUFT: [f32; 4] = [0.85, 0.80, 0.68, 1.0];
const CHELICERA_GREEN: [f32; 4] = [0.10, 0.62, 0.42, 1.0];
const EYE_BLACK: [f32; 4] = [0.04, 0.04, 0.05, 1.0];
const EYE_GLINT: [f32; 4] = [0.80, 0.88, 0.95, 0.95];
const BUG_COLOR: [f32; 4] = [0.55, 0.55, 0.50, 0.9];
const LINE_COLOR: [f32; 4] = [0.80, 0.85, 0.92, 0.55];

/// Per-leg geometry, front (rank 0) to back (rank 3): attachment (x, y),
/// resting yaw offset, femur, tibia, tarsus. Salticid legs are short and the
/// front pair is the longest and held forward.
const LEG_GEOM: [(f32, f32, f32, f32, f32, f32); 4] = [
    (2.4, 3.0, 1.05, 5.6, 5.0, 3.4),
    (2.7, 1.5, 0.40, 4.6, 4.4, 3.1),
    (2.7, 0.0, -0.25, 4.3, 4.2, 3.0),
    (2.4, -1.5, -0.85, 5.2, 5.0, 3.3),
];
const LEG_Z: f32 = 4.2;
/// Body height above the desktop when standing tall; a crouch lowers it.
const BODY_Z: f32 = 4.6;

pub struct SpiderMeshes {
    cephalothorax: Mesh,
    abdomen: Mesh,
    head_setae: Mesh,
    abdomen_setae: Mesh,
    palp: Mesh,
    eye_big: Mesh,
    eye_small: Mesh,
    eye_glint: Mesh,
    chelicera: Mesh,
    bug: Mesh,
    /// `[rank][segment]`; both sides share a rank's lengths.
    leg_segments: Vec<[Mesh; 3]>,
}

impl SpiderMeshes {
    pub fn build() -> Self {
        let leg_segments = LEG_GEOM
            .iter()
            .map(|&(_, _, _, femur, tibia, tarsus)| {
                [
                    tapered_limb(0.42, femur, 0.20),
                    tapered_limb(0.36, tibia, 0.28),
                    tapered_limb(0.28, tarsus, 0.48),
                ]
            })
            .collect();
        SpiderMeshes {
            cephalothorax: mesh::sphere(3.2, 36, 56),
            // Finer than the fly's abdomen: the literal register paints
            // spots onto it per vertex, and 12x16 cannot hold a spot.
            abdomen: mesh::sphere(3.3, 72, 96),
            head_setae: setae(3.2, 620, 0.39),
            abdomen_setae: setae(3.3, 850, 0.42),
            palp: mesh::capsule(0.34, 1.9, 6, 8),
            eye_big: finish(mesh::sphere(0.95, 28, 40), Material::EYE),
            eye_small: finish(mesh::sphere(0.45, 20, 28), Material::EYE),
            eye_glint: mesh::sphere(0.10, 12, 16),
            chelicera: finish(mesh::capsule(0.45, 2.2, 6, 8), Material::METAL),
            bug: mesh::sphere(0.85, 6, 8),
            leg_segments,
        }
    }
}

/// Appends `src` with a colour chosen per vertex from its *local* position,
/// which is how the abdomen gets its spots and the legs their bands without
/// any extra geometry.
fn emit_painted(out: &mut Mesh, src: &Mesh, m: &Mat4, paint: impl Fn([f32; 3]) -> [f32; 4]) {
    let base = out.verts.len() as u32;
    for v in &src.verts {
        out.verts.push(Vertex {
            texcoord: v.texcoord,
            pos: math::transform_point(m, v.pos),
            normal: math::transform_normal(m, v.normal),
            color: {
                let mut c = paint(v.pos);
                let p = v.pos;
                let grain = 0.91
                    + 0.09
                        * ((p[0] * 19.1 + p[1] * 13.7).sin() * (p[2] * 21.3 - p[0] * 9.2).sin())
                            .abs();
                for k in 0..3 {
                    c[k] *= grain;
                }
                c
            },
            material: v.material,
        });
    }
    out.indices.extend(src.indices.iter().map(|i| i + base));
}

/// A capsule segment with one pale ring at its distal end: the joint
/// marking. Distal only — banding both ends merges into long pale stretches
/// across every joint, and the animal's legs are mostly black.
fn emit_banded(out: &mut Mesh, src: &Mesh, m: &Mat4, length: f32, base: [f32; 4], band: [f32; 4]) {
    let edge = length / 2.0 - 0.55;
    emit_painted(out, src, m, |p| if p[1] > edge { band } else { base });
}

fn emit(out: &mut Mesh, src: &Mesh, m: &Mat4, color: [f32; 4]) {
    let base = out.verts.len() as u32;
    for v in &src.verts {
        out.verts.push(Vertex {
            texcoord: v.texcoord,
            pos: math::transform_point(m, v.pos),
            normal: math::transform_normal(m, v.normal),
            color,
            material: if color == GlassPalette::SHELL
                || color == GlassPalette::SHELL_DENSE
                || color == GlassPalette::LIMB
                || color == GlassPalette::WING
            {
                Material::GLASS
            } else if color == TUFT {
                Material::MATTE
            } else {
                v.material
            },
        });
    }
    out.indices.extend(src.indices.iter().map(|i| i + base));
}

/// Root transform: scene position, heading, altitude, scale. Same as the
/// fly's, minus pitch (a spider does not bank).
fn root_of(pose: &SpiderPose) -> Mat4 {
    math::mul(
        math::translate(pose.pos.x, pose.pos.y, pose.z),
        math::mul(
            math::rotate_z(pose.heading - std::f32::consts::FRAC_PI_2),
            math::scale(pose.scale, pose.scale, pose.scale),
        ),
    )
}

pub fn build_frame(
    out: &mut Mesh,
    meshes: &SpiderMeshes,
    spider: &Spider,
    pose: &SpiderPose,
    glass: bool,
) -> usize {
    out.verts.clear();
    out.indices.clear();
    let root = root_of(pose);
    let body_z = BODY_Z - 2.2 * pose.crouch;

    let (c_body, c_abd, c_leg, c_eye, c_chel) = if glass {
        (
            GlassPalette::SHELL,
            GlassPalette::SHELL,
            GlassPalette::LIMB,
            GlassPalette::SHELL_DENSE,
            GlassPalette::LIMB,
        )
    } else {
        (BODY_DARK, BODY_MID, LEG_DARK, EYE_BLACK, CHELICERA_GREEN)
    };

    // --- cephalothorax, yawed by the head ---
    let head = math::mul(
        root,
        math::mul(
            math::translate(0.0, 2.0, body_z),
            math::rotate_z(pose.head_yaw),
        ),
    );
    emit(
        out,
        &meshes.cephalothorax,
        &math::mul(head, math::scale(1.05, 1.0, 0.72)),
        c_body,
    );
    if !glass {
        emit(
            out,
            &meshes.head_setae,
            &math::mul(head, math::scale(1.05, 1.0, 0.72)),
            [0.24, 0.22, 0.20, 1.0],
        );
    }
    // Principal eyes (AME): large, forward, the salticid's signature.
    for side in [-1.0f32, 1.0] {
        emit(
            out,
            &meshes.eye_big,
            &math::mul(head, math::translate(side * 1.15, 3.05, 0.9)),
            c_eye,
        );
        if !glass {
            // A highlight on each principal eye, and the pale tuft above it.
            emit(
                out,
                &meshes.eye_glint,
                &math::mul(head, math::translate(side * 0.85, 3.8, 1.3)),
                EYE_GLINT,
            );
            emit(
                out,
                &meshes.eye_small,
                &math::mul(
                    head,
                    math::trs([side * 1.2, 2.6, 1.9], [0.0; 3], [1.3, 0.52, 0.32]),
                ),
                TUFT,
            );
        }
        // Anterior lateral eyes, smaller and outboard.
        emit(
            out,
            &meshes.eye_small,
            &math::mul(head, math::translate(side * 2.55, 2.4, 1.15)),
            c_eye,
        );
        // Posterior median and lateral pairs complete the salticid's eight eyes.
        for (y, size) in [(1.0, 0.48), (-0.25, 0.70)] {
            emit(
                out,
                &meshes.eye_small,
                &math::mul(head, math::trs([side * 2.7, y, 1.3], [0.0; 3], [size; 3])),
                c_eye,
            );
        }
        // Paired short sensory palps frame the iridescent mouthparts.
        emit(
            out,
            &meshes.palp,
            &math::mul(
                head,
                math::trs(
                    [side * 1.55, 3.35, -0.65],
                    [0.15, 0.0, side * -0.25],
                    [1.0; 3],
                ),
            ),
            if glass { GlassPalette::LIMB } else { TUFT },
        );
        // Chelicerae, hanging below the front.
        emit(
            out,
            &meshes.chelicera,
            &math::mul(
                head,
                math::mul(math::translate(side * 0.7, 3.3, -1.4), math::rotate_x(0.35)),
            ),
            c_chel,
        );
    }

    // --- abdomen ---
    let abd_m = math::mul(
        root,
        math::trs(
            [0.0, -4.4, body_z - 0.3],
            [0.0; 3],
            [1.0, 1.3, 0.85 * pose.abdomen_breathe],
        ),
    );
    if glass {
        emit(out, &meshes.abdomen, &abd_m, c_abd);
    } else {
        // Phidippus audax: one large dorsal spot forward of centre, two
        // smaller ones behind it. Painted on the unit sphere, so they ride
        // the breathing scale.
        let r = 3.3;
        emit_painted(out, &meshes.abdomen, &abd_m, |p| {
            let (x, y, z) = (p[0] / r, p[1] / r, p[2] / r);
            if z < 0.35 {
                return c_abd;
            }
            let d = |cx: f32, cy: f32| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            let edge = 0.018 * (x * 67.0 + y * 43.0).sin();
            if d(0.0, 0.15) < 0.34 + edge
                || d(-0.40, -0.50) < 0.18 + edge
                || d(0.40, -0.50) < 0.18 + edge
            {
                SPOT_WHITE
            } else {
                c_abd
            }
        });
    }

    if !glass {
        emit(out, &meshes.abdomen_setae, &abd_m, [0.23, 0.21, 0.20, 1.0]);
    }

    // --- legs: cephalothorax -> femur, knee -> tibia, ankle -> tarsus ---
    let lie_along_x = math::euler(0.0, 0.0, -std::f32::consts::FRAC_PI_2);
    for (i, leg) in spider.legs.iter().enumerate() {
        let (ax, ay, yaw_off, femur, tibia, tarsus) = LEG_GEOM[leg.rank];
        let side = leg.side;
        let base_yaw = if side > 0.0 {
            yaw_off
        } else {
            std::f32::consts::PI - yaw_off
        };
        // A crouch splays the legs outward and flattens the femur.
        let lift = pose.legs[i].1 - 0.30 * pose.crouch;
        let angle = pose.legs[i].0;
        let leg_root = math::mul(
            root,
            math::trs(
                [side * ax, ay, LEG_Z - 1.6 * pose.crouch],
                [0.0, -lift, base_yaw + side * angle],
                [1.0; 3],
            ),
        );
        let segs = &meshes.leg_segments[leg.rank];
        let femur_m = math::mul(
            leg_root,
            math::mul(math::translate(femur / 2.0, 0.0, 0.0), lie_along_x),
        );
        if glass {
            emit(out, &segs[0], &femur_m, c_leg);
        } else {
            emit_banded(out, &segs[0], &femur_m, femur, c_leg, LEG_BAND);
        }
        // Salticid knees sit high: the femur rises, the tibia drops steeply.
        let knee = math::mul(
            leg_root,
            math::trs(
                [femur, 0.0, 0.0],
                [0.0, 0.95 + 0.25 * pose.crouch, -0.25 * side],
                [1.0; 3],
            ),
        );
        let tibia_m = math::mul(
            knee,
            math::mul(math::translate(tibia / 2.0, 0.0, 0.0), lie_along_x),
        );
        if glass {
            emit(out, &segs[1], &tibia_m, c_leg);
        } else {
            emit_banded(out, &segs[1], &tibia_m, tibia, c_leg, LEG_BAND);
        }
        let ankle = math::mul(
            knee,
            math::trs([tibia, 0.0, 0.0], [0.0, 0.45, -0.12 * side], [1.0; 3]),
        );
        emit(
            out,
            &segs[2],
            &math::mul(
                ankle,
                math::mul(math::translate(tarsus / 2.0, 0.0, 0.0), lie_along_x),
            ),
            c_leg,
        );
    }

    let inspection_vertices = out.verts.len();

    // --- silk: world space. Every thread the spider has out, and the line
    // in progress from its anchor to the spider. Today that is only the
    // dragline; a web is the same loop with more segments (WEB_PLAN.md §4).
    for seg in spider.silk.segments(pose.pos) {
        let dx = seg.b.x - seg.a.x;
        let dy = seg.b.y - seg.a.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len > 1.0 {
            let line = mesh::capsule(0.22, len, 2, 5);
            let m = math::mul(
                math::translate((seg.a.x + seg.b.x) * 0.5, (seg.a.y + seg.b.y) * 0.5, 3.0),
                math::rotate_z(dy.atan2(dx) - std::f32::consts::FRAC_PI_2),
            );
            emit(out, &line, &m, silk_color(seg.kind, glass));
        }
    }

    // --- bugs: three drifting points each, nothing with legs ---
    for b in &spider.prey {
        let t = b.age * 9.0;
        for k in 0..3 {
            let ph = t + k as f32 * 2.1;
            let m = math::translate(
                b.pos.x + ph.cos() * 2.2,
                b.pos.y + (ph * 1.3).sin() * 2.2,
                3.0 + (ph * 0.7).sin() * 0.8,
            );
            emit(out, &meshes.bug, &m, BUG_COLOR);
        }
    }

    if spider.state == SpiderState::Sleeping {
        for v in out.verts.iter_mut() {
            v.pos[2] -= 0.5;
        }
    }
    inspection_vertices
}

/// Thread colour by kind. The dragline keeps the colour it always had; the
/// sticky capture kinds read a touch brighter so a finished orb shows its
/// spiral. Phase 7 of WEB_PLAN.md replaces capsules with a line pipeline.
fn silk_color(kind: ThreadKind, glass: bool) -> [f32; 4] {
    let base = if glass {
        LINE_COLOR
    } else {
        [0.75, 0.75, 0.72, 0.6]
    };
    if kind.is_sticky() {
        [base[0], base[1], base[2], (base[3] * 1.35).min(1.0)]
    } else if kind == ThreadKind::Retreat {
        [base[0], base[1], base[2], (base[3] * 1.6).min(1.0)]
    } else {
        base
    }
}

/// The circuit inside the glass body. The chimera's authored node is drawn
/// in the AUTHORED colour and larger, exactly as in the brain window: the
/// invented part is never allowed to look like the measured ones.
pub fn build_neuron_field(
    out: &mut Mesh,
    sim: &LifSim,
    flash: &[f32],
    spider: &Spider,
    pose: &SpiderPose,
) {
    out.verts.clear();
    out.indices.clear();
    let root = root_of(pose);
    let mood = if spider.state == SpiderState::Sleeping {
        0.45
    } else {
        1.0
    };
    let body_z = BODY_Z - 2.2 * pose.crouch;
    // Cephalothorax and abdomen together, inside the shell.
    let layout = NeuronLayout::fit_into(
        &sim.positions,
        [-3.0, -7.4, body_z - 1.2],
        [3.0, 4.6, body_z + 1.6],
    );
    for (i, p) in sim.positions.iter().enumerate() {
        let f = flash.get(i).copied().unwrap_or(0.0);
        let authored = sim.origins.get(i).copied() == Some(Origin::Authored);
        // The authored node is always faintly visible, so you can find the
        // seam even when it is quiet; measured neurons are drawn only when
        // they fire, as in the fly.
        if f < 0.012 && !authored {
            continue;
        }
        let base = if authored {
            AUTHORED_COLOR
        } else {
            sim.manifest.color_for(&sim.roles[i])
        };
        let (a, radius) = if authored {
            ((0.10 + f * 0.25) * mood, (0.9 + f * 1.4) * pose.scale)
        } else {
            // Lower than the fly's 0.055: LC11 is 127 cells packed into the
            // cephalothorax, and at the fly's alpha a seen prey whites out
            // the whole head instead of lighting the eyes.
            (f * 0.030 * mood, (0.35 + f * 0.9) * pose.scale)
        };
        let centre = math::transform_point(&root, layout.map(*p));
        let col = [
            (base[0] * (0.6 + f * 0.9)).min(1.6),
            (base[1] * (0.6 + f * 0.9)).min(1.6),
            (base[2] * (0.6 + f * 0.9)).min(1.6),
            a,
        ];
        let b = out.verts.len() as u32;
        for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            out.verts.push(Vertex {
                texcoord: [0.0; 4],
                pos: [centre[0] + dx * radius, centre[1] + dy * radius, centre[2]],
                normal: [dx, dy, 0.0],
                color: col,
                material: Material::MATTE,
            });
        }
        out.indices
            .extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dfcore::Vec2;

    #[test]
    fn a_frame_has_eight_legs_worth_of_geometry_and_valid_indices() {
        let meshes = SpiderMeshes::build();
        let s = Spider::new(Vec2::ZERO, 1);
        let mut m = Mesh::default();
        build_frame(&mut m, &meshes, &s, &s.pose(), true);
        assert!(!m.indices.is_empty());
        assert_eq!(m.indices.len() % 3, 0);
        assert!((*m.indices.iter().max().unwrap() as usize) < m.verts.len());
        // Legs alone: 8 legs x 3 capsules; plus body parts. Count by verts.
        let capsule_verts = mesh::capsule(0.42, 5.6, 6, 8).verts.len();
        assert!(
            m.verts.len() > 24 * capsule_verts / 2,
            "too little geometry for eight legs"
        );
    }

    #[test]
    fn the_dragline_and_bugs_are_drawn_when_present() {
        let meshes = SpiderMeshes::build();
        let mut s = Spider::new(Vec2::ZERO, 1);
        let mut bare = Mesh::default();
        build_frame(&mut bare, &meshes, &s, &s.pose(), true);
        s.spawn_bug(Vec2::new(50.0, 0.0), Vec2::ZERO);
        s.start_jump(Vec2::new(80.0, 0.0), true);
        let mut with = Mesh::default();
        build_frame(&mut with, &meshes, &s, &s.pose(), true);
        assert!(with.verts.len() > bare.verts.len());
    }

    #[test]
    fn the_literal_register_is_a_bold_jumping_spider() {
        let meshes = SpiderMeshes::build();
        let s = Spider::new(Vec2::ZERO, 1);
        let mut m = Mesh::default();
        build_frame(&mut m, &meshes, &s, &s.pose(), false);
        // Literal pigment carries fine tonal grain; identify each marking's
        // narrow pigment range rather than requiring flat RGB equality.
        let count = |c: [f32; 4]| {
            m.verts
                .iter()
                .filter(|v| {
                    (0..3).all(|k| v.color[k] >= c[k] * 0.9099 && v.color[k] <= c[k] + 0.0001)
                        && v.color[3] == c[3]
                })
                .count()
        };
        assert!(count(SPOT_WHITE) > 20, "abdominal spots");
        assert!(count(LEG_BAND) > 60, "leg bands");
        assert!(count(LEG_BAND) < count(LEG_DARK), "legs are mostly black");
        assert!(count(CHELICERA_GREEN) > 20, "green chelicerae");
        assert!(count(EYE_GLINT) > 10, "eye highlights");
        assert!(count(BODY_DARK) > count(SPOT_WHITE), "mostly black");
        let mut g = Mesh::default();
        build_frame(&mut g, &meshes, &s, &s.pose(), true);
        let gcount = |c: [f32; 4]| g.verts.iter().filter(|v| v.color == c).count();
        assert_eq!(
            gcount(SPOT_WHITE) + gcount(LEG_BAND),
            0,
            "glass has no markings"
        );
    }

    #[test]
    fn head_yaw_moves_the_eyes_not_the_abdomen() {
        let meshes = SpiderMeshes::build();
        let mut s = Spider::new(Vec2::ZERO, 1);
        s.heading = std::f32::consts::FRAC_PI_2; // model +y = world +y
        let mut straight = Mesh::default();
        build_frame(&mut straight, &meshes, &s, &s.pose(), false);
        s.head_yaw = 0.8;
        let mut turned = Mesh::default();
        build_frame(&mut turned, &meshes, &s, &s.pose(), false);
        // The eyes (the only EYE_BLACK vertices) must have moved sideways;
        // the front legs reach further forward than the eyes, so "frontmost
        // vertex" would test the wrong part.
        let eyes_x = |m: &Mesh| {
            let eyes: Vec<f32> = m
                .verts
                .iter()
                .filter(|v| v.color == EYE_BLACK)
                .map(|v| v.pos[0])
                .collect();
            assert!(!eyes.is_empty());
            eyes.iter().sum::<f32>() / eyes.len() as f32
        };
        assert!((eyes_x(&straight) - eyes_x(&turned)).abs() > 0.5);
        // The abdomen's rearmost vertex is unaffected.
        let back = |m: &Mesh| {
            m.verts
                .iter()
                .min_by(|a, b| a.pos[1].partial_cmp(&b.pos[1]).unwrap())
                .unwrap()
                .pos
        };
        assert!((back(&straight)[0] - back(&turned)[0]).abs() < 1e-3);
    }
}
