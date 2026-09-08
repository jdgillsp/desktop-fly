//! The web builders' geometry (WEB_PLAN.md §7): three species on one rig.
//!
//! Same convention as the fly and the salticid: the model faces +y, z is up,
//! and the body computes angles while this file turns them into capsules and
//! spheres. A [`Look`] holds what differs between the species — proportions
//! and markings — so the frame builder is written once.
//!
//! Glass is the register. In the literal register the three are:
//!
//! - *Araneus diadematus*: a big rounded brown abdomen with the pale dorsal
//!   cross that names it, a small cephalothorax, banded legs.
//! - *Parasteatoda tepidariorum*: small, a globular grey-brown abdomen with
//!   darker chevrons, fine legs.
//! - *Agelenopsis*: long, striped — two dark bands down the cephalothorax —
//!   with long legs and two long spinnerets trailing behind.

use dfcore::{LifSim, Origin, ThreadKind, Weaver as Species, WeaverBody, WeaverPose, WeaverState};

use crate::brain::AUTHORED_COLOR;
use crate::flybody::{finish, setae, tapered_limb, GlassPalette, NeuronLayout};
use crate::math::{self, Mat4};
use crate::mesh::{self, Material, Mesh, Vertex};

const BUG_COLOR: [f32; 4] = [0.55, 0.55, 0.50, 0.9];
const LINE_COLOR: [f32; 4] = [0.80, 0.85, 0.92, 0.55];
const LEG_Z: f32 = 4.2;
const BODY_Z: f32 = 4.4;

/// Per-species proportions and colours.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    /// Radius and (x, y, z) scale of the cephalothorax.
    pub ceph: (f32, [f32; 3]),
    /// Radius and (x, y, z) scale of the abdomen, and its y offset.
    pub abd: (f32, [f32; 3], f32),
    /// Per rank: attachment (x, y), resting yaw, femur, tibia, tarsus.
    pub legs: [(f32, f32, f32, f32, f32, f32); 4],
    pub leg_radius: f32,
    pub spinnerets: bool,
    pub body: [f32; 4],
    pub abd_color: [f32; 4],
    pub leg: [f32; 4],
    pub band: [f32; 4],
    pub mark: [f32; 4],
}

impl Look {
    pub fn of(species: Species) -> Look {
        match species {
            Species::Araneus => Look {
                ceph: (2.5, [1.0, 1.0, 0.7]),
                abd: (4.0, [1.15, 1.3, 0.95], -5.6),
                legs: [
                    (2.2, 2.5, 0.95, 7.0, 6.4, 4.0),
                    (2.5, 1.2, 0.35, 6.2, 5.8, 3.6),
                    (2.5, -0.2, -0.35, 5.0, 4.8, 3.0),
                    (2.2, -1.6, -0.95, 6.4, 6.0, 3.8),
                ],
                leg_radius: 0.40,
                spinnerets: false,
                body: [0.42, 0.30, 0.18, 1.0],
                abd_color: [0.55, 0.40, 0.24, 1.0],
                leg: [0.38, 0.27, 0.16, 1.0],
                band: [0.70, 0.60, 0.45, 1.0],
                mark: [0.94, 0.92, 0.86, 1.0],
            },
            Species::Parasteatoda => Look {
                ceph: (2.0, [1.0, 1.0, 0.7]),
                abd: (3.5, [1.0, 1.05, 1.05], -4.6),
                legs: [
                    (1.9, 2.0, 0.85, 6.0, 5.6, 3.4),
                    (2.1, 0.9, 0.30, 5.2, 4.9, 3.1),
                    (2.1, -0.2, -0.35, 4.3, 4.1, 2.7),
                    (1.9, -1.3, -0.95, 5.6, 5.3, 3.3),
                ],
                leg_radius: 0.30,
                spinnerets: false,
                body: [0.42, 0.36, 0.32, 1.0],
                abd_color: [0.56, 0.48, 0.42, 1.0],
                leg: [0.48, 0.42, 0.36, 1.0],
                band: [0.30, 0.25, 0.22, 1.0],
                mark: [0.30, 0.24, 0.20, 1.0],
            },
            Species::Agelenopsis => Look {
                ceph: (2.6, [0.85, 1.25, 0.65]),
                abd: (3.2, [0.85, 1.45, 0.75], -6.2),
                // Long, but not so long the tarsi reach under a vivarium's
                // floor: `every_creature_fits_between_the_floor_and_the_rim`.
                legs: [
                    (2.2, 3.0, 0.95, 6.8, 6.2, 3.6),
                    (2.4, 1.6, 0.35, 6.2, 5.8, 3.4),
                    (2.4, 0.0, -0.35, 5.4, 5.0, 3.0),
                    (2.2, -1.6, -0.95, 6.4, 6.0, 3.5),
                ],
                leg_radius: 0.34,
                spinnerets: true,
                body: [0.50, 0.40, 0.28, 1.0],
                abd_color: [0.48, 0.40, 0.30, 1.0],
                leg: [0.46, 0.38, 0.27, 1.0],
                band: [0.30, 0.24, 0.16, 1.0],
                mark: [0.24, 0.19, 0.13, 1.0],
            },
        }
    }
}

pub struct WeaverMeshes {
    look: Look,
    cephalothorax: Mesh,
    abdomen: Mesh,
    abdomen_setae: Mesh,
    head_setae: Mesh,
    palp: Mesh,
    eye: Mesh,
    chelicera: Mesh,
    spinneret: Mesh,
    bug: Mesh,
    /// One capsule of unit length, scaled per thread: a web is a thousand
    /// of these a frame, and building each afresh was the frame budget.
    line: Mesh,
    leg_segments: Vec<[Mesh; 3]>,
}

impl WeaverMeshes {
    pub fn build(species: Species) -> Self {
        let look = Look::of(species);
        let r = look.leg_radius;
        let leg_segments = look
            .legs
            .iter()
            .map(|&(_, _, _, femur, tibia, tarsus)| {
                [
                    tapered_limb(r, femur, 0.20),
                    tapered_limb(r * 0.85, tibia, 0.28),
                    tapered_limb(r * 0.65, tarsus, 0.48),
                ]
            })
            .collect();
        WeaverMeshes {
            look,
            cephalothorax: mesh::sphere(look.ceph.0, 36, 56),
            abdomen: mesh::sphere(look.abd.0, 72, 96),
            abdomen_setae: setae(look.abd.0, 410, 0.27),
            head_setae: setae(look.ceph.0, 240, 0.22),
            palp: mesh::capsule(0.20, 1.6, 5, 6),
            eye: finish(mesh::sphere(0.38, 20, 28), Material::EYE),
            chelicera: mesh::capsule(0.38, 1.8, 6, 8),
            spinneret: mesh::capsule(0.22, 3.2, 5, 6),
            bug: mesh::sphere(0.85, 6, 8),
            line: mesh::capsule(0.45, 1.0, 2, 3),
            leg_segments,
        }
    }
}

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
            } else {
                v.material
            },
        });
    }
    out.indices.extend(src.indices.iter().map(|i| i + base));
}

fn root_of(pose: &WeaverPose) -> Mat4 {
    math::mul(
        math::translate(pose.pos.x, pose.pos.y, pose.z),
        math::mul(
            math::rotate_z(pose.heading - std::f32::consts::FRAC_PI_2),
            math::scale(pose.scale, pose.scale, pose.scale),
        ),
    )
}

/// Thread colour by kind and excitation. The capture spiral is the one kind
/// a viewer should be able to pick out; a vibrating thread glows.
pub fn silk_color(kind: ThreadKind, excite: f32, glass: bool) -> [f32; 4] {
    let base = if glass {
        LINE_COLOR
    } else {
        [0.78, 0.78, 0.74, 0.62]
    };
    let mut c = match kind {
        ThreadKind::Capture => [base[0], base[1] * 0.98, base[2], (base[3] * 1.35).min(1.0)],
        ThreadKind::Gumfoot => [base[0], base[1], base[2] * 0.9, (base[3] * 1.3).min(1.0)],
        ThreadKind::Auxiliary => [base[0] * 0.9, base[1] * 0.85, base[2] * 0.75, base[3] * 0.7],
        ThreadKind::Retreat => [base[0], base[1], base[2], (base[3] * 1.5).min(1.0)],
        _ => base,
    };
    let g = excite.min(1.0);
    if g > 0.01 {
        c[0] = (c[0] + 0.6 * g).min(1.6);
        c[1] = (c[1] + 0.5 * g).min(1.6);
        c[3] = (c[3] + 0.4 * g).min(1.0);
    }
    c
}

pub fn build_frame(
    out: &mut Mesh,
    meshes: &WeaverMeshes,
    w: &WeaverBody,
    pose: &WeaverPose,
    glass: bool,
) -> usize {
    out.verts.clear();
    out.indices.clear();
    let look = &meshes.look;
    let root = root_of(pose);
    let body_z = BODY_Z - 2.0 * pose.crouch;

    let (c_body, c_abd, c_leg, c_eye, c_chel) = if glass {
        (
            GlassPalette::SHELL,
            GlassPalette::SHELL,
            GlassPalette::LIMB,
            GlassPalette::SHELL_DENSE,
            GlassPalette::LIMB,
        )
    } else {
        (
            look.body,
            look.abd_color,
            look.leg,
            [0.05, 0.05, 0.06, 1.0],
            look.band,
        )
    };

    // --- cephalothorax ---
    let head = math::mul(root, math::translate(0.0, 1.6, body_z));
    let ceph_m = math::mul(
        head,
        math::scale(look.ceph.1[0], look.ceph.1[1], look.ceph.1[2]),
    );
    if glass || w.species != Species::Agelenopsis {
        emit(out, &meshes.cephalothorax, &ceph_m, c_body);
    } else {
        // Two dark longitudinal stripes: the grass spider's signature.
        let r = look.ceph.0;
        let mark = look.mark;
        emit_painted(out, &meshes.cephalothorax, &ceph_m, |p| {
            let x = p[0] / r;
            if p[2] > 0.1 && ((0.25..0.55).contains(&x.abs())) {
                mark
            } else {
                c_body
            }
        });
    }
    if !glass {
        emit(out, &meshes.head_setae, &ceph_m, look.band);
    }
    // Eight small eyes in two rows; the front-middle pair a touch larger.
    let ry = look.ceph.0 * look.ceph.1[1];
    for (i, (x, y, z, s)) in [
        (-0.55, 0.92, 0.55, 1.15),
        (0.55, 0.92, 0.55, 1.15),
        (-1.35, 0.80, 0.55, 0.9),
        (1.35, 0.80, 0.55, 0.9),
        (-0.75, 0.70, 0.95, 0.85),
        (0.75, 0.70, 0.95, 0.85),
        (-1.5, 0.55, 0.95, 0.8),
        (1.5, 0.55, 0.95, 0.8),
    ]
    .iter()
    .enumerate()
    {
        let _ = i;
        emit(
            out,
            &meshes.eye,
            &math::mul(
                head,
                math::mul(math::translate(*x, ry * *y, *z), math::scale(*s, *s, *s)),
            ),
            c_eye,
        );
    }
    for side in [-1.0f32, 1.0] {
        emit(
            out,
            &meshes.palp,
            &math::mul(
                head,
                math::trs(
                    [side * 1.0, ry * 1.05, -0.55],
                    [0.15, 0.0, -side * 0.3],
                    [1.0; 3],
                ),
            ),
            c_leg,
        );
        emit(
            out,
            &meshes.chelicera,
            &math::mul(
                head,
                math::mul(
                    math::translate(side * 0.55, ry * 0.95, -1.1),
                    math::rotate_x(0.35),
                ),
            ),
            c_chel,
        );
    }

    // --- abdomen ---
    let (ar, asc, ay) = look.abd;
    let abd_m = math::mul(
        root,
        math::trs(
            [0.0, ay, body_z - 0.2],
            [0.0; 3],
            [asc[0], asc[1], asc[2] * pose.abdomen_breathe],
        ),
    );
    if glass {
        emit(out, &meshes.abdomen, &abd_m, c_abd);
    } else {
        let mark = look.mark;
        let species = w.species;
        emit_painted(out, &meshes.abdomen, &abd_m, |p| {
            let (x, y, z) = (p[0] / ar, p[1] / ar, p[2] / ar);
            if z < 0.2 {
                return c_abd;
            }
            match species {
                // The dorsal cross: a line of pale dots down the middle and
                // a bar across the front third.
                Species::Araneus => {
                    let d = |cx: f32, cy: f32| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
                    let dots = [
                        (0.0, 0.55),
                        (0.0, 0.25),
                        (0.0, -0.05),
                        (0.0, -0.35),
                        (-0.32, 0.22),
                        (0.32, 0.22),
                        (-0.2, 0.42),
                        (0.2, 0.42),
                    ];
                    if dots
                        .iter()
                        .any(|&(cx, cy)| d(cx, cy) < 0.095 + 0.017 * (x * 73.0 + y * 51.0).sin())
                    {
                        mark
                    } else {
                        // Scalloped dark folium surrounds the cross, with warm outer flanks.
                        let edge = 0.48 + 0.10 * (y * 16.0).cos();
                        if x.abs() < edge {
                            [0.34, 0.22, 0.13, 1.0]
                        } else {
                            c_abd
                        }
                    }
                }
                // Chevrons: dark bands that bow toward the rear.
                Species::Parasteatoda => {
                    let v = y + 0.35 * x.abs();
                    let f = (v * 5.0).rem_euclid(1.0);
                    if (0.0..0.28).contains(&f) && x.abs() < 0.7 {
                        mark
                    } else {
                        c_abd
                    }
                }
                // A pale central band flanked by darker sides.
                Species::Agelenopsis => {
                    if x.abs() < 0.22 {
                        [0.66, 0.58, 0.44, 1.0]
                    } else if x.abs() < 0.5 || ((y + 0.5 * x.abs()) * 5.0).rem_euclid(1.0) < 0.18 {
                        mark
                    } else {
                        c_abd
                    }
                }
            }
        });
    }
    if !glass {
        emit(out, &meshes.abdomen_setae, &abd_m, look.band);
    }
    if look.spinnerets {
        for side in [-1.0f32, 1.0] {
            emit(
                out,
                &meshes.spinneret,
                &math::mul(
                    root,
                    math::mul(
                        math::translate(side * 0.6, ay - ar * asc[1] - 1.2, body_z - 0.6),
                        math::rotate_z(side * 0.12),
                    ),
                ),
                c_leg,
            );
        }
    }

    // --- legs ---
    let lie_along_x = math::euler(0.0, 0.0, -std::f32::consts::FRAC_PI_2);
    for (i, leg) in w.legs.iter().enumerate() {
        let (ax, ayy, yaw_off, femur, tibia, tarsus) = look.legs[leg.rank];
        let side = leg.side;
        let base_yaw = if side > 0.0 {
            yaw_off
        } else {
            std::f32::consts::PI - yaw_off
        };
        let lift = pose.legs[i].1 - 0.30 * pose.crouch;
        let angle = pose.legs[i].0;
        let leg_root = math::mul(
            root,
            math::trs(
                [side * ax, ayy, LEG_Z - 1.5 * pose.crouch],
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
            emit_banded(out, &segs[0], &femur_m, femur, c_leg, look.band);
        }
        let knee = math::mul(
            leg_root,
            math::trs(
                [femur, 0.0, 0.0],
                [0.0, 0.85 + 0.25 * pose.crouch, -0.22 * side],
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
            emit_banded(out, &segs[1], &tibia_m, tibia, c_leg, look.band);
        }
        let ankle = math::mul(
            knee,
            math::trs([tibia, 0.0, 0.0], [0.0, 0.45, -0.10 * side], [1.0; 3]),
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
    crate::webspace::place_body(out, inspection_vertices, w);

    // --- silk: every thread, and the line in progress ---
    if let Some(spider) = w.spatial_pos {
        for (a,b,kind,excite) in w.silk.spatial_segments(spider) {
            let len = ((b[0]-a[0]).powi(2)+(b[1]-a[1]).powi(2)+(b[2]-a[2]).powi(2)).sqrt();
            if len < 0.1 { continue; }
            let color = silk_color(kind,excite,glass);
            let at = |u: f32| {
                std::array::from_fn(|k| a[k]+(b[k]-a[k])*u)
            };
            crate::habitatmesh::silk_tube(out,at(0.0),at(1.0),0.16,5,false,color);
        }
        if let Some(a)=w.navigation.as_ref().and_then(|n|n.safety_anchor) {
            crate::habitatmesh::silk_tube(out,a,spider,0.12,5,false,silk_color(ThreadKind::Dragline,0.0,glass));
        }
    } else {
    for seg in w.silk.segments(pose.pos) {
        let dx = seg.b.x - seg.a.x;
        let dy = seg.b.y - seg.a.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len > 1.0 {
            // Thicker than the salticid's dragline: a web is hundreds of
            // sub-pixel lines, and at 0.22 they alias into dust. The unit
            // capsule is stretched along its axis; its end caps stretch
            // too, invisibly at this radius.
            let m = math::mul(
                math::translate((seg.a.x + seg.b.x) * 0.5, (seg.a.y + seg.b.y) * 0.5, 3.0),
                math::mul(
                    math::rotate_z(dy.atan2(dx) - std::f32::consts::FRAC_PI_2),
                    math::scale(1.0, len, 1.0),
                ),
            );
            emit(
                out,
                &meshes.line,
                &m,
                silk_color(seg.kind, seg.excite, glass),
            );
        }
    }

    }
    // --- bugs: three drifting points each; a stuck one shivers in place ---
    for b in &w.prey {
        let at = if w.spatial_pos.is_some() { w.silk.spatial_at(b.pos).unwrap_or([b.pos.x,b.pos.y,3.0]) }
            else { [b.pos.x,b.pos.y,3.0] };
        let t = b.age * 9.0;
        let spread = if b.stuck { 0.8 + 1.4 * b.struggle } else { 2.2 };
        for k in 0..3 {
            let ph = t + k as f32 * 2.1;
            let m = math::translate(
                at[0] + ph.cos() * spread,
                at[1] + (ph * 1.3).sin() * spread,
                at[2] + (ph * 0.7).sin() * 0.8,
            );
            emit(out, &meshes.bug, &m, BUG_COLOR);
        }
    }

    if pose.state == WeaverState::Sleeping {
        for v in out.verts.iter_mut().take(inspection_vertices) {
            v.pos[2] -= 0.5;
        }
    }
    inspection_vertices
}

/// The circuit inside the glass body; the authored strike node cool and
/// larger, as in the brain window.
pub fn build_neuron_field(
    out: &mut Mesh,
    sim: &LifSim,
    flash: &[f32],
    w: &WeaverBody,
    pose: &WeaverPose,
) {
    out.verts.clear();
    out.indices.clear();
    let root = root_of(pose);
    let mood = if w.state == WeaverState::Sleeping {
        0.45
    } else {
        1.0
    };
    let body_z = BODY_Z - 2.0 * pose.crouch;
    let look = Look::of(w.species);
    let (ar, asc, ay) = look.abd;
    let layout = NeuronLayout::fit_into(
        &sim.positions,
        [-2.6, ay - ar * asc[1] * 0.8, body_z - 1.0],
        [2.6, 3.6, body_z + 1.4],
    );
    for (i, p) in sim.positions.iter().enumerate() {
        let f = flash.get(i).copied().unwrap_or(0.0);
        let authored = sim.origins.get(i).copied() == Some(Origin::Authored);
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
            (f * 0.045 * mood, (0.35 + f * 0.9) * pose.scale)
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
                normal: [0.0, 0.0, 1.0],
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
    use dfcore::weaver::Idle;
    use dfcore::Vec2;

    #[test]
    fn every_species_renders_with_eight_legs_and_valid_indices() {
        for sp in Species::ALL {
            let meshes = WeaverMeshes::build(sp);
            let w = WeaverBody::new(sp, Vec2::ZERO, 1, Box::new(Idle(sp)));
            for glass in [true, false] {
                let mut m = Mesh::default();
                build_frame(&mut m, &meshes, &w, &w.pose(), glass);
                assert!(m.indices.iter().all(|&i| (i as usize) < m.verts.len()));
                let capsule_verts = mesh::capsule(0.4, 5.0, 6, 8).verts.len();
                assert!(
                    m.verts.len() > 24 * capsule_verts / 2,
                    "{sp:?} has too little leg geometry"
                );
            }
        }
    }

    #[test]
    fn silk_and_bugs_are_drawn_when_present() {
        let sp = Species::Araneus;
        let meshes = WeaverMeshes::build(sp);
        let mut w = WeaverBody::new(sp, Vec2::ZERO, 1, Box::new(Idle(sp)));
        let mut bare = Mesh::default();
        build_frame(&mut bare, &meshes, &w, &w.pose(), true);
        w.silk.pay_out(Vec2::new(-50.0, 0.0), ThreadKind::Capture);
        w.silk.attach(Vec2::new(50.0, 0.0), dfcore::Anchor::Fixed);
        w.spawn_bug(Vec2::new(20.0, 20.0), Vec2::ZERO, false);
        let mut with = Mesh::default();
        build_frame(&mut with, &meshes, &w, &w.pose(), true);
        assert!(with.verts.len() > bare.verts.len());
    }

    /// The frame budget with a whole orb on screen (WEB_PLAN.md Phase 7):
    /// build the web body-only, then time the geometry for sixty frames. A
    /// thousand capsules from one cached unit mesh have to stay well inside
    /// a 33 ms frame even in a debug build.
    #[test]
    fn a_finished_orb_builds_its_geometry_inside_the_frame_budget() {
        use dfcore::{OrbProgram, Region};
        let tank = Region::centered((720.0, 520.0));
        let mut rng = dfcore::rng::Pcg32::new(3);
        let program = OrbProgram::new(&dfcore::Anchors::enclosure(tank), &mut rng);
        let mut w = WeaverBody::new(Species::Araneus, Vec2::ZERO, 3, Box::new(program));
        let mut s = dfcore::BrainSignals::new();
        s.walk_drive = 0.6;
        let mut t = 0.0;
        while !w.web_complete() && t < 900.0 {
            w.update(1.0 / 60.0, tank, None, Some(s));
            t += 1.0 / 60.0;
        }
        assert!(
            w.silk.threads.len() > 800,
            "{} threads",
            w.silk.threads.len()
        );
        let meshes = WeaverMeshes::build(Species::Araneus);
        let mut m = Mesh::default();
        let start = std::time::Instant::now();
        for _ in 0..60 {
            build_frame(&mut m, &meshes, &w, &w.pose(), true);
        }
        let per_frame = start.elapsed().as_secs_f32() * 1000.0 / 60.0;
        eprintln!(
            "orb geometry: {} threads, {} verts, {:.2} ms per frame (debug build)",
            w.silk.threads.len(),
            m.verts.len(),
            per_frame
        );
        assert!(
            per_frame < 20.0,
            "{per_frame:.1} ms per frame is over budget"
        );
    }

    #[test]
    fn the_capture_spiral_reads_brighter_and_vibration_glows() {
        let quiet = silk_color(ThreadKind::Radius, 0.0, true);
        let sticky = silk_color(ThreadKind::Capture, 0.0, true);
        let loud = silk_color(ThreadKind::Radius, 1.0, true);
        assert!(sticky[3] > quiet[3]);
        assert!(loud[0] > quiet[0] && loud[3] > quiet[3]);
    }
}
