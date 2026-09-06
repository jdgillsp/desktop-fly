//! The procedural fly body, ported part-for-part from `buildFlyModel`
//! (FlyModel.swift:144) and `buildLeg` (FlyModel.swift:88).
//!
//! Every position, radius, scale and colour is taken verbatim from the Swift
//! source. The one structural change is that SceneKit's retained node graph
//! becomes an immediate-mode rebuild: `Fly` gives us a `Pose` plus leg angles,
//! and this module walks the hierarchy each frame writing transformed vertices
//! into one buffer.
//!
//! Rebuilding on the CPU is a deliberate v1 choice. The fly is ~28 parts and
//! ~6k vertices, so the transform cost is negligible next to the full-screen
//! clear Spike 0 measured, and it avoids a per-node bind-group scheme for a
//! scene this small. If the brain window's 23k-point cloud lands on the same
//! path it should move to instancing.

use dfcore::body::{Fly, State};
use dfcore::{LifSim, Pose};

use crate::math::{self, Mat4};
use crate::mesh::{self, Mesh, Vertex};

const BODY_BROWN: [f32; 4] = [0.50, 0.38, 0.22, 1.0];
const LEG_COLOR: [f32; 4] = [0.33, 0.24, 0.14, 1.0];
const TARSUS_COLOR: [f32; 4] = [0.2475, 0.18, 0.105, 1.0]; // legColor blended 25% black
const EYE_COLOR: [f32; 4] = [0.62, 0.10, 0.07, 1.0];
const ANTENNA_COLOR: [f32; 4] = [0.30, 0.22, 0.13, 1.0];
const PROBOSCIS_COLOR: [f32; 4] = [0.35, 0.26, 0.16, 1.0];
/// `bodyBrown.blended(withFraction: 0.15, of: .white)`
const HEAD_COLOR: [f32; 4] = [0.575, 0.473, 0.337, 1.0];
/// The abdomen's stripe texture (`abdomenTexture()`), as two banded colours.
const ABDOMEN_LIGHT: [f32; 4] = [0.72, 0.55, 0.32, 1.0];
const ABDOMEN_DARK: [f32; 4] = [0.22, 0.15, 0.09, 1.0];
const WING_COLOR: [f32; 4] = [0.82, 0.86, 0.92, 0.30];

/// "Glass anatomy" (PORT_PLAN.md §6.3 #2): a soft translucent shell with the
/// live connectome visible inside it.
///
/// This is the recommended rendering register, and it is not only a look. It
/// solves the "don't creep me out" constraint permanently for *any* creature —
/// glass and glow do not trigger a vermin reflex where chitin and compound eyes
/// do — it fuses the pet and the brain window into one object, which is the
/// project's actual thesis, and it is the only non-arbitrary rendering for a
/// synthetic creature that has no real anatomy to be faithful to.
pub struct GlassPalette;

impl GlassPalette {
    /// Translucent, but not invisible. Tuned against a white background, which
    /// is the hard case: at alpha 0.26 the creature simply could not be found
    /// on a light desktop. A pet you lose is a failed pet.
    pub const SHELL: [f32; 4] = [0.46, 0.60, 0.78, 0.46];
    pub const SHELL_DENSE: [f32; 4] = [0.38, 0.54, 0.76, 0.56];
    pub const LIMB: [f32; 4] = [0.34, 0.46, 0.64, 0.72];
    pub const WING: [f32; 4] = [0.72, 0.82, 0.94, 0.22];
}

/// Maps the circuit's positions into the body's own local space.
///
/// The ETL normalises the *whole* brain into [-10, 10], and the 668-neuron
/// circuit occupies only a fraction of that box — so using brain coordinates
/// directly crams every neuron into one spot, which additively saturates to a
/// white blob however faint each sprite is. Renormalising to the circuit's own
/// extent is what spreads it through the creature.
///
/// Anatomically loose on purpose: at ~40 px on screen a faithful brain would be
/// sub-pixel and invisible. This is a visualisation of the wiring, not a claim
/// about where each soma sits in the animal — the honest reading of a
/// diagrammatic body.
#[derive(Debug, Clone, Copy)]
pub struct NeuronLayout {
    min: [f32; 3],
    inv_span: [f32; 3],
    lo: [f32; 3],
    hi: [f32; 3],
}

impl NeuronLayout {
    /// Body extents to fill: roughly head-to-abdomen, and inside the shell.
    const LO: [f32; 3] = [-4.2, -11.0, 4.2];
    const HI: [f32; 3] = [4.2, 11.5, 8.0];

    pub fn fit(positions: &[[f32; 3]]) -> Self {
        Self::fit_into(positions, Self::LO, Self::HI)
    }

    /// The same normalisation into a caller's box — another creature's body
    /// has other extents.
    pub fn fit_into(positions: &[[f32; 3]], lo: [f32; 3], hi: [f32; 3]) -> Self {
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for p in positions {
            for k in 0..3 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
        let mut inv_span = [1.0f32; 3];
        for k in 0..3 {
            let span = max[k] - min[k];
            inv_span[k] = if span > 1e-4 { 1.0 / span } else { 0.0 };
        }
        NeuronLayout { min, inv_span, lo, hi }
    }

    pub fn map(&self, p: [f32; 3]) -> [f32; 3] {
        let mut out = [0.0f32; 3];
        for k in 0..3 {
            let t = (p[k] - self.min[k]) * self.inv_span[k];
            out[k] = self.lo[k] + t * (self.hi[k] - self.lo[k]);
        }
        out
    }
}

/// Per-leg segment lengths from the `specs` table (FlyModel.swift:204):
/// `(attach_x, attach_y, base_yaw_offset, femur, tibia, tarsus)`.
/// `z` is 4.5 for every leg; `swing_sign` and `phase` live in `dfcore::body`.
const LEG_GEOM: [(f32, f32, f32, f32, f32, f32); 6] = [
    (3.1, 5.3, 0.95, 4.2, 4.8, 3.2),
    (-3.1, 5.3, 0.95, 4.2, 4.8, 3.2),
    (3.7, 2.0, -0.10, 4.8, 5.6, 3.8),
    (-3.7, 2.0, -0.10, 4.8, 5.6, 3.8),
    (3.3, -1.2, -0.95, 5.8, 7.0, 4.6),
    (-3.3, -1.2, -0.95, 5.8, 7.0, 4.6),
];
const LEG_Z: f32 = 4.5;

/// Meshes are generated once and reused; only transforms change per frame.
pub struct FlyMeshes {
    thorax: Mesh,
    abdomen: Mesh,
    head: Mesh,
    eye: Mesh,
    antenna: Mesh,
    proboscis: Mesh,
    wing: Mesh,
    /// Indexed as `[leg][segment]`, since segment lengths differ per leg.
    leg_segments: Vec<[Mesh; 3]>,
}

impl FlyMeshes {
    pub fn build() -> Self {
        let abdomen = {
            // Banded to stand in for `abdomenTexture()`: dark stripes across the
            // long axis, which is what reads at this size.
            let mut m = mesh::sphere(5.0, 20, 28);
            for v in m.verts.iter_mut() {
                let band = ((v.pos[1] * 0.9).sin() * 3.0).sin();
                v.color = if band > 0.15 {
                    ABDOMEN_DARK
                } else {
                    ABDOMEN_LIGHT
                };
            }
            m
        };
        let leg_segments = LEG_GEOM
            .iter()
            .map(|&(_, _, _, f, t, ta)| {
                [
                    mesh::capsule(0.48, f, 8, 10),
                    mesh::capsule(0.38, t, 8, 10),
                    mesh::capsule(0.24, ta, 8, 10),
                ]
            })
            .collect();

        FlyMeshes {
            thorax: mesh::sphere(4.6, 18, 24),
            abdomen,
            head: mesh::sphere(3.0, 16, 20),
            eye: mesh::sphere(2.0, 14, 18),
            antenna: mesh::capsule(0.16, 2.2, 6, 8),
            proboscis: mesh::cone(0.6, 0.22, 2.4, 12),
            // The Bezier oval is `NSRect(x: -2.6, y: -15.5, width: 5.2, height: 16.5)`,
            // i.e. centred 7.25 behind the hinge.
            wing: mesh::wing(5.2, 16.5, -7.25, 20),
            leg_segments,
        }
    }
}

/// Appends `src` transformed by `m` into `out`, with a flat colour.
fn emit(out: &mut Mesh, src: &Mesh, m: &Mat4, color: [f32; 4]) {
    let off = out.verts.len() as u32;
    out.verts.reserve(src.verts.len());
    for v in &src.verts {
        out.verts.push(Vertex {
            pos: math::transform_point(m, v.pos),
            normal: math::transform_dir(m, v.normal),
            color,
        });
    }
    out.indices.extend(src.indices.iter().map(|i| i + off));
}

/// Same, but keeping each vertex's own colour (for the banded abdomen).
fn emit_vertex_colored(out: &mut Mesh, src: &Mesh, m: &Mat4) {
    let off = out.verts.len() as u32;
    for v in &src.verts {
        out.verts.push(Vertex {
            pos: math::transform_point(m, v.pos),
            normal: math::transform_dir(m, v.normal),
            color: v.color,
        });
    }
    out.indices.extend(src.indices.iter().map(|i| i + off));
}

/// Screen-facing quads for the circuit's neurons, inside the body.
///
/// The camera is orthographic looking down -z, so a screen-facing billboard is
/// just an xy quad — no view-matrix maths needed.
pub fn build_neuron_field(
    out: &mut Mesh,
    sim: &LifSim,
    flash: &[f32],
    fly: &Fly,
    pose: &Pose,
) {
    out.verts.clear();
    out.indices.clear();

    let root = math::mul(
        math::translate(pose.pos.x, pose.pos.y, pose.z),
        math::mul(
            math::euler(pose.pitch, 0.0, pose.heading - std::f32::consts::FRAC_PI_2),
            math::scale(pose.scale, pose.scale, pose.scale),
        ),
    );
    // Sleeping creatures dim rather than go dark: the network is still running.
    let mood = if fly.state == State::Sleeping { 0.45 } else { 1.0 };
    let layout = NeuronLayout::fit(&sim.positions);

    for (i, p) in sim.positions.iter().enumerate() {
        let f = flash.get(i).copied().unwrap_or(0.0);
        let base = sim.manifest.color_for(&sim.roles[i]);
        // Only *activity* is drawn. A constant per-neuron glow means 668
        // additive sprites overlapping inside a ~40 px body, which saturates to
        // a white blob no matter how small each one is — and it says the wrong
        // thing anyway. The resting look belongs to the glass shell; these
        // points are neurons firing. You see it think, rather than see a lamp.
        // Low threshold so the spontaneous crackle of a resting network is
        // still faintly visible — the creature is never actually "off".
        if f < 0.012 {
            continue;
        }
        // Even spread out, hundreds of sprites overlap; the per-neuron
        // contribution has to stay small enough that a whole population firing
        // reads as a bright region rather than clipping to white.
        let a = f * 0.055 * mood;
        let radius = (0.35 + f * 1.0) * pose.scale;
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
                // The corner rides in the normal: `fs_neuron` uses it for a
                // radial falloff, and nothing lights these points.
                normal: [dx, dy, 0.0],
                color: col,
            });
        }
        out.indices
            .extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
}

/// Walk the fly's hierarchy and write one frame's worth of world-space geometry.
pub fn build_frame(out: &mut Mesh, meshes: &FlyMeshes, fly: &Fly, pose: &Pose, glass: bool) {
    out.verts.clear();
    out.indices.clear();

    // Root: scene position, then heading (SceneKit uses `heading - pi/2` because
    // the model faces +y), then pitch, then the altitude scale.
    let root = math::mul(
        math::translate(pose.pos.x, pose.pos.y, pose.z),
        math::mul(
            math::euler(
                pose.pitch,
                0.0,
                pose.heading - std::f32::consts::FRAC_PI_2,
            ),
            math::scale(pose.scale, pose.scale, pose.scale),
        ),
    );

    let (c_thorax, c_head, c_eye, c_ant, c_prob, c_leg, c_tarsus, c_wing) = if glass {
        (
            GlassPalette::SHELL,
            GlassPalette::SHELL,
            GlassPalette::SHELL_DENSE,
            GlassPalette::LIMB,
            GlassPalette::LIMB,
            GlassPalette::LIMB,
            GlassPalette::LIMB,
            GlassPalette::WING,
        )
    } else {
        (
            BODY_BROWN,
            HEAD_COLOR,
            EYE_COLOR,
            ANTENNA_COLOR,
            PROBOSCIS_COLOR,
            LEG_COLOR,
            TARSUS_COLOR,
            WING_COLOR,
        )
    };

    // --- torso ---
    emit(
        out,
        &meshes.thorax,
        &math::mul(root, math::trs([0.0, 2.5, 6.2], [0.0; 3], [0.95, 1.15, 0.85])),
        c_thorax,
    );
    if glass {
        emit(
            out,
            &meshes.abdomen,
            &math::mul(
                root,
                math::trs(
                    [0.0, -6.5, 5.6],
                    [0.0; 3],
                    [0.9, 1.5, 0.75 * pose.abdomen_breathe],
                ),
            ),
            GlassPalette::SHELL,
        );
    } else {
    emit_vertex_colored(
        out,
        &meshes.abdomen,
        &math::mul(
            root,
            math::trs(
                [0.0, -6.5, 5.6],
                [0.0; 3],
                [0.9, 1.5, 0.75 * pose.abdomen_breathe],
            ),
        ),
    );
    }
    emit(
        out,
        &meshes.head,
        &math::mul(root, math::trs([0.0, 9.0, 6.0], [0.0; 3], [1.0, 0.85, 0.9])),
        c_head,
    );

    // --- head furniture ---
    for side in [-1.0f32, 1.0] {
        emit(
            out,
            &meshes.eye,
            &math::mul(
                root,
                math::trs([side * 2.1, 9.7, 6.4], [0.0; 3], [0.8, 1.0, 1.15]),
            ),
            c_eye,
        );
        emit(
            out,
            &meshes.antenna,
            &math::mul(
                root,
                math::trs([side * 0.9, 11.6, 6.3], [-1.15, 0.0, side * 0.35], [1.0; 3]),
            ),
            c_ant,
        );
    }
    emit(
        out,
        &meshes.proboscis,
        &math::mul(root, math::trs([0.0, 10.4, 4.6], [-0.5, 0.0, 0.0], [1.0; 3])),
        c_prob,
    );

    // --- legs: root -> femur, knee -> tibia, ankle -> tarsus ---
    for (i, &(ax, ay, yaw_off, femur, tibia, tarsus)) in LEG_GEOM.iter().enumerate() {
        let leg = &fly.legs[i];
        let base_yaw = if leg.swing_sign > 0.0 {
            yaw_off
        } else {
            std::f32::consts::PI - yaw_off
        };
        // Leg.apply(): eulerAngles = (0, -lift, baseYaw + swingSign * angle)
        let leg_root = math::mul(
            root,
            math::trs(
                [ax, ay, LEG_Z],
                [0.0, -leg.lift, base_yaw + leg.swing_sign * leg.angle],
                [1.0; 3],
            ),
        );
        let segs = &meshes.leg_segments[i];

        // Each segment capsule is rotated -pi/2 about z so its y-axis lies along
        // the limb's local +x, and shifted to its midpoint.
        let lie_along_x = math::euler(0.0, 0.0, -std::f32::consts::FRAC_PI_2);
        emit(
            out,
            &segs[0],
            &math::mul(
                leg_root,
                math::mul(math::translate(femur / 2.0, 0.0, 0.0), lie_along_x),
            ),
            c_leg,
        );

        let knee = math::mul(
            leg_root,
            math::trs(
                [femur, 0.0, 0.0],
                [0.0, 0.75, -0.30 * leg.swing_sign],
                [1.0; 3],
            ),
        );
        emit(
            out,
            &segs[1],
            &math::mul(
                knee,
                math::mul(math::translate(tibia / 2.0, 0.0, 0.0), lie_along_x),
            ),
            c_leg,
        );

        let ankle = math::mul(
            knee,
            math::trs(
                [tibia, 0.0, 0.0],
                [0.0, 0.35, -0.15 * leg.swing_sign],
                [1.0; 3],
            ),
        );
        emit(
            out,
            &segs[2],
            &math::mul(
                ankle,
                math::mul(math::translate(tarsus / 2.0, 0.0, 0.0), lie_along_x),
            ),
            c_tarsus,
        );
    }

    // --- wings ---
    for (i, side) in [-1.0f32, 1.0].into_iter().enumerate() {
        let w = pose.wings[i];
        emit(
            out,
            &meshes.wing,
            &math::mul(
                root,
                math::trs(
                    [side * 1.6, 0.5, if side > 0.0 { 7.7 } else { 7.55 }],
                    [w[0], w[1], w[2]],
                    [1.0; 3],
                ),
            ),
            c_wing,
        );
    }

    // Sleeping flies tuck down slightly; a small cue that reads at fly scale.
    if fly.state == State::Sleeping {
        for v in out.verts.iter_mut() {
            v.pos[2] -= 0.6;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dfcore::Vec2;

    fn a_fly() -> Fly {
        Fly::new(Vec2::new(0.0, 0.0), dfcore::DEFAULT_SEED)
    }

    #[test]
    fn a_frame_produces_geometry_for_every_part() {
        let meshes = FlyMeshes::build();
        let fly = a_fly();
        let mut out = Mesh::default();
        build_frame(&mut out, &meshes, &fly, &fly.pose(), false);
        // 3 body + 2 eyes + 2 antennae + 1 proboscis + 18 leg segments + 2 wings.
        assert!(out.verts.len() > 3000, "only {} verts", out.verts.len());
        assert!(out.indices.len() % 3 == 0);
        assert!(out.indices.iter().all(|&i| (i as usize) < out.verts.len()));
    }

    #[test]
    fn the_fly_is_roughly_fly_sized_and_centred_on_its_position() {
        let meshes = FlyMeshes::build();
        let mut fly = a_fly();
        fly.pos = Vec2::new(120.0, -60.0);
        let mut out = Mesh::default();
        build_frame(&mut out, &meshes, &fly, &fly.pose(), false);

        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for v in &out.verts {
            for i in 0..2 {
                lo[i] = lo[i].min(v.pos[i]);
                hi[i] = hi[i].max(v.pos[i]);
            }
        }
        // Body spans roughly 40 scene units at FLY_SCALE 1.15 — a small bug on a
        // 2560px desktop, not a dog.
        let span_x = hi[0] - lo[0];
        let span_y = hi[1] - lo[1];
        assert!((10.0..80.0).contains(&span_x), "x span {span_x}");
        assert!((10.0..80.0).contains(&span_y), "y span {span_y}");
        // And it is drawn where the fly actually is.
        assert!((lo[0]..hi[0]).contains(&120.0), "x {lo:?}..{hi:?}");
        assert!((lo[1]..hi[1]).contains(&-60.0));
    }

    /// Altitude must scale the rendered body — that is how height reads on a
    /// flat desktop, and a behaviour test asserts the same relationship.
    #[test]
    fn altitude_makes_the_fly_bigger() {
        let meshes = FlyMeshes::build();
        let mut fly = a_fly();
        let mut out = Mesh::default();

        let span = |m: &Mesh| {
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for v in &m.verts {
                lo = lo.min(v.pos[0]);
                hi = hi.max(v.pos[0]);
            }
            hi - lo
        };
        build_frame(&mut out, &meshes, &fly, &fly.pose(), false);
        let grounded = span(&out);

        fly.alt = 1.0;
        build_frame(&mut out, &meshes, &fly, &fly.pose(), false);
        let airborne = span(&out);

        assert!(
            airborne > grounded * 1.5,
            "grounded {grounded} -> airborne {airborne}"
        );
    }

    #[test]
    fn legs_move_when_the_gait_advances() {
        let meshes = FlyMeshes::build();
        let mut fly = a_fly();
        fly.state = State::Walking;
        fly.speed = 60.0;
        let mut a = Mesh::default();
        let mut b = Mesh::default();
        build_frame(&mut a, &meshes, &fly, &fly.pose(), false);
        for _ in 0..12 {
            fly.update(
                1.0 / 60.0,
                dfcore::Region::centered((1512.0, 982.0)),
                None,
                None,
            );
        }
        build_frame(&mut b, &meshes, &fly, &fly.pose(), false);
        let moved = a
            .verts
            .iter()
            .zip(b.verts.iter())
            .any(|(x, y)| (x.pos[0] - y.pos[0]).abs() > 0.05);
        assert!(moved, "gait produced no vertex motion");
    }
}
