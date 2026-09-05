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

use dfcore::{LifSim, Origin, Spider, SpiderPose, SpiderState};

use crate::brain::AUTHORED_COLOR;
use crate::flybody::{GlassPalette, NeuronLayout};
use crate::math::{self, Mat4};
use crate::mesh::{self, Mesh, Vertex};

const BODY_DARK: [f32; 4] = [0.20, 0.17, 0.16, 1.0];
const BODY_MID: [f32; 4] = [0.30, 0.25, 0.22, 1.0];
const LEG_DARK: [f32; 4] = [0.22, 0.18, 0.16, 1.0];
const EYE_BLACK: [f32; 4] = [0.05, 0.05, 0.07, 1.0];
const EYE_GLINT: [f32; 4] = [0.35, 0.55, 0.65, 0.9];
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
    eye_big: Mesh,
    eye_small: Mesh,
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
                    mesh::capsule(0.42, femur, 6, 8),
                    mesh::capsule(0.36, tibia, 6, 8),
                    mesh::capsule(0.28, tarsus, 6, 8),
                ]
            })
            .collect();
        SpiderMeshes {
            cephalothorax: mesh::sphere(3.2, 12, 16),
            abdomen: mesh::sphere(3.3, 12, 16),
            eye_big: mesh::sphere(0.95, 8, 10),
            eye_small: mesh::sphere(0.45, 6, 8),
            chelicera: mesh::capsule(0.45, 2.2, 6, 8),
            bug: mesh::sphere(0.85, 6, 8),
            leg_segments,
        }
    }
}

fn emit(out: &mut Mesh, src: &Mesh, m: &Mat4, color: [f32; 4]) {
    let base = out.verts.len() as u32;
    for v in &src.verts {
        out.verts.push(Vertex {
            pos: math::transform_point(m, v.pos),
            normal: math::transform_dir(m, v.normal),
            color,
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

pub fn build_frame(out: &mut Mesh, meshes: &SpiderMeshes, spider: &Spider, pose: &SpiderPose, glass: bool) {
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
        (BODY_DARK, BODY_MID, LEG_DARK, EYE_BLACK, LEG_DARK)
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
    // Principal eyes (AME): large, forward, the salticid's signature.
    for side in [-1.0f32, 1.0] {
        emit(
            out,
            &meshes.eye_big,
            &math::mul(head, math::translate(side * 1.15, 3.05, 0.9)),
            c_eye,
        );
        if !glass {
            emit(
                out,
                &meshes.eye_small,
                &math::mul(head, math::translate(side * 0.9, 3.75, 1.25)),
                EYE_GLINT,
            );
        }
        // Anterior lateral eyes, smaller and outboard.
        emit(
            out,
            &meshes.eye_small,
            &math::mul(head, math::translate(side * 2.55, 2.4, 1.15)),
            c_eye,
        );
        // Chelicerae, hanging below the front.
        emit(
            out,
            &meshes.chelicera,
            &math::mul(
                head,
                math::mul(
                    math::translate(side * 0.7, 3.3, -1.4),
                    math::rotate_x(0.35),
                ),
            ),
            c_chel,
        );
    }

    // --- abdomen ---
    emit(
        out,
        &meshes.abdomen,
        &math::mul(
            root,
            math::trs(
                [0.0, -4.4, body_z - 0.3],
                [0.0; 3],
                [1.0, 1.3, 0.85 * pose.abdomen_breathe],
            ),
        ),
        c_abd,
    );

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
        emit(
            out,
            &segs[0],
            &math::mul(leg_root, math::mul(math::translate(femur / 2.0, 0.0, 0.0), lie_along_x)),
            c_leg,
        );
        // Salticid knees sit high: the femur rises, the tibia drops steeply.
        let knee = math::mul(
            leg_root,
            math::trs([femur, 0.0, 0.0], [0.0, 0.95 + 0.25 * pose.crouch, -0.25 * side], [1.0; 3]),
        );
        emit(
            out,
            &segs[1],
            &math::mul(knee, math::mul(math::translate(tibia / 2.0, 0.0, 0.0), lie_along_x)),
            c_leg,
        );
        let ankle = math::mul(
            knee,
            math::trs([tibia, 0.0, 0.0], [0.0, 0.45, -0.12 * side], [1.0; 3]),
        );
        emit(
            out,
            &segs[2],
            &math::mul(ankle, math::mul(math::translate(tarsus / 2.0, 0.0, 0.0), lie_along_x)),
            c_leg,
        );
    }

    // --- dragline: world space, from the anchor to the spider ---
    if let Some(a) = pose.dragline {
        let dx = pose.pos.x - a.x;
        let dy = pose.pos.y - a.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len > 1.0 {
            let line = mesh::capsule(0.22, len, 2, 5);
            let m = math::mul(
                math::translate((a.x + pose.pos.x) * 0.5, (a.y + pose.pos.y) * 0.5, 3.0),
                math::rotate_z(dy.atan2(dx) - std::f32::consts::FRAC_PI_2),
            );
            emit(out, &line, &m, if glass { LINE_COLOR } else { [0.75, 0.75, 0.72, 0.6] });
        }
    }

    // --- bugs: three drifting points each, nothing with legs ---
    for b in &spider.prey {
        let t = b.age * 9.0;
        for k in 0..3 {
            let ph = t + k as f32 * 2.1;
            let m = math::translate(b.pos.x + ph.cos() * 2.2, b.pos.y + (ph * 1.3).sin() * 2.2, 3.0 + (ph * 0.7).sin() * 0.8);
            emit(out, &meshes.bug, &m, BUG_COLOR);
        }
    }

    if spider.state == SpiderState::Sleeping {
        for v in out.verts.iter_mut() {
            v.pos[2] -= 0.5;
        }
    }
}

/// The circuit inside the glass body. The chimera's authored node is drawn
/// in the AUTHORED colour and larger, exactly as in the brain window: the
/// invented part is never allowed to look like the measured ones.
pub fn build_neuron_field(out: &mut Mesh, sim: &LifSim, flash: &[f32], spider: &Spider, pose: &SpiderPose) {
    out.verts.clear();
    out.indices.clear();
    let root = root_of(pose);
    let mood = if spider.state == SpiderState::Sleeping { 0.45 } else { 1.0 };
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
                pos: [centre[0] + dx * radius, centre[1] + dy * radius, centre[2]],
                normal: [dx, dy, 0.0],
                color: col,
            });
        }
        out.indices.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
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
        assert!(m.verts.len() > 24 * capsule_verts / 2, "too little geometry for eight legs");
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
