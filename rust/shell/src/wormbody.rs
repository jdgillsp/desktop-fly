//! The worm's geometry: a tapered tube along the body the crawler computes.
//!
//! `dfcore::worm::Worm` already produces the body as a list of points sampled
//! along the head's recorded path, so all this does is sweep a capsule down
//! that polyline. Nothing here decides how the animal moves.
//!
//! *C. elegans* is transparent in life, so the glass register is the honest
//! rendering rather than a stylistic choice (PORT_PLAN.md §6.1); the literal
//! register is a pale, faintly opaque version of the same tube — there is no
//! chitin to draw.

use dfcore::{GradedSim, Worm};

use crate::flybody::GlassPalette;
use crate::math;
use crate::mesh::{self, Mesh, Vertex};

/// Literal register: a translucent, slightly warm cuticle.
const CUTICLE: [f32; 4] = [0.86, 0.82, 0.70, 0.80];
const CUTICLE_HEAD: [f32; 4] = [0.90, 0.86, 0.74, 0.88];

/// Radius at the head and at the tail. The animal tapers at both ends; the
/// tail more sharply.
const R_HEAD: f32 = 2.4;
const R_MID: f32 = 2.9;
const R_TAIL: f32 = 0.7;
/// Height of the body's centreline above the desktop plane, so the tube sits
/// on the surface like the fly's feet rather than half through it.
const Z: f32 = 3.0;

fn radius_at(t: f32) -> f32 {
    // Widest a third of the way back, then a long taper to the tail.
    if t < 0.33 {
        R_HEAD + (R_MID - R_HEAD) * (t / 0.33)
    } else {
        let u = (t - 0.33) / 0.67;
        R_MID + (R_TAIL - R_MID) * u * u
    }
}

/// Sweep a capsule down each body segment.
pub fn build_frame(out: &mut Mesh, worm: &Worm, glass: bool) {
    out.verts.clear();
    out.indices.clear();
    let n = worm.body.len();
    if n < 2 {
        return;
    }
    for i in 0..n - 1 {
        let a = worm.body[i];
        let b = worm.body[i + 1];
        let t = i as f32 / (n - 1) as f32;
        let r = radius_at(t);
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-3 {
            continue;
        }
        // Capsules generate along +y; rotate to the segment's direction. Total
        // height includes the caps, so overlapping caps between consecutive
        // segments smooth the joints for free.
        let seg = mesh::capsule(r, len + 2.0 * r, 6, 10);
        let m = math::mul(
            math::translate((a.x + b.x) * 0.5, (a.y + b.y) * 0.5, Z),
            math::rotate_z(dy.atan2(dx) - std::f32::consts::FRAC_PI_2),
        );
        let color = if glass {
            if t < 0.12 {
                GlassPalette::SHELL_DENSE
            } else {
                GlassPalette::SHELL
            }
        } else if t < 0.12 {
            CUTICLE_HEAD
        } else {
            CUTICLE
        };
        append(out, &seg, &m, color);
    }
}

fn append(out: &mut Mesh, src: &Mesh, m: &math::Mat4, color: [f32; 4]) {
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

/// The connectome inside the body.
///
/// The worm's neurons are laid out by the ETL with the anterior–posterior axis
/// first, so each soma is placed by its normalised AP coordinate as an arc
/// length along the body polyline, and its lateral coordinate as an offset
/// across it. That is the honest diagrammatic body: the nerve ring bunches at
/// the head, the ventral cord runs the length, because that is where they are.
///
/// Graded neurons do not spike, so there is no flash to draw: `activity` is
/// normalised depolarisation, and a neuron is drawn as bright as it is
/// depolarised above rest.
pub fn build_neuron_field(out: &mut Mesh, sim: &GradedSim, activity: &[f32], worm: &Worm) {
    out.verts.clear();
    out.indices.clear();
    let n = worm.body.len();
    if n < 2 || sim.positions.is_empty() {
        return;
    }
    // Normalise the AP and lateral axes over the circuit's own extent.
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in &sim.positions {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let span = |k: usize| (hi[k] - lo[k]).max(1e-4);

    for (i, p) in sim.positions.iter().enumerate() {
        let a = activity.get(i).copied().unwrap_or(0.0);
        if a < 0.05 {
            continue;
        }
        let ap = (p[0] - lo[0]) / span(0);
        let lat = (p[1] - lo[1]) / span(1) * 2.0 - 1.0;
        // Arc-length position along the body, head first.
        let s = ap * (n - 1) as f32;
        let i0 = (s.floor() as usize).min(n - 2);
        let f = s - i0 as f32;
        let (b0, b1) = (worm.body[i0], worm.body[i0 + 1]);
        let cx = b0.x + (b1.x - b0.x) * f;
        let cy = b0.y + (b1.y - b0.y) * f;
        let dx = b1.x - b0.x;
        let dy = b1.y - b0.y;
        let len = (dx * dx + dy * dy).sqrt().max(1e-3);
        let r = radius_at(ap) * 0.6;
        // Perpendicular offset across the body.
        let (nx, ny) = (-dy / len, dx / len);
        let centre = [cx + nx * lat * r, cy + ny * lat * r, Z + radius_at(ap) + 0.5];
        let base = sim.manifest.color_for(&sim.roles[i]);
        let alpha = a * 0.16;
        let radius = 0.45 + a * 1.1;
        let col = [
            (base[0] * (0.6 + a * 0.9)).min(1.6),
            (base[1] * (0.6 + a * 0.9)).min(1.6),
            (base[2] * (0.6 + a * 0.9)).min(1.6),
            alpha,
        ];
        let b = out.verts.len() as u32;
        for (ox, oy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            out.verts.push(Vertex {
                pos: [centre[0] + ox * radius, centre[1] + oy * radius, centre[2]],
                // The corner rides in the normal for the radial falloff.
                normal: [ox, oy, 0.0],
                color: col,
            });
        }
        out.indices
            .extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dfcore::creature::{Body, World};
        use dfcore::Region;
    use dfcore::{BrainSignals, Vec2};

    fn crawled_worm() -> Worm {
        let mut w = Worm::new(Vec2::ZERO, 7);
        let mut d = BrainSignals::new();
        d.walk_drive = 0.8;
        let world = World {
            region: Region::centered((1000.0, 800.0)),
            ledges: Vec::new(),
            cursor: None,
            attractor: None,
        };
        for _ in 0..120 {
            w.step(1.0 / 60.0, &d, &world);
        }
        w
    }

    #[test]
    fn the_body_is_a_closed_tube_that_follows_every_segment() {
        let w = crawled_worm();
        let mut m = Mesh::default();
        build_frame(&mut m, &w, true);
        assert!(!m.indices.is_empty());
        assert_eq!(m.indices.len() % 3, 0);
        let max = *m.indices.iter().max().unwrap() as usize;
        assert!(max < m.verts.len(), "index out of range");
        // Every body point should be inside the swept geometry's xy extent.
        let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
        for v in &m.verts {
            lo = (lo.0.min(v.pos[0]), lo.1.min(v.pos[1]));
            hi = (hi.0.max(v.pos[0]), hi.1.max(v.pos[1]));
        }
        for p in &w.body {
            assert!(p.x >= lo.0 && p.x <= hi.0 && p.y >= lo.1 && p.y <= hi.1);
        }
    }

    #[test]
    fn the_tube_tapers_from_head_to_tail() {
        assert!(radius_at(0.0) < radius_at(0.33));
        assert!(radius_at(1.0) < radius_at(0.0));
        assert!(radius_at(1.0) > 0.0);
    }

    #[test]
    fn the_literal_register_is_still_translucent_because_the_animal_is() {
        let w = crawled_worm();
        let mut m = Mesh::default();
        build_frame(&mut m, &w, false);
        assert!(m.verts.iter().all(|v| v.color[3] < 1.0));
    }
}
