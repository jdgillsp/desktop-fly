//! The sandworm's geometry: a ringed tube, mostly under the sand.
//!
//! `dfcore::sandworm::Sandworm` produces the spine, how much of the body is
//! up, how far the head is reared and how far the mouth is open. This draws
//! three things from that:
//!
//! * **The ripple.** Under the sand the body itself is below the surface and
//!   hidden by it, so what is drawn is a raised, translucent ridge of sand
//!   along the spine — the wake the fiction is famous for. It is the only
//!   thing visible most of the time, and in free roam, where there is no
//!   sand, it is what says "something is moving under the desktop".
//! * **The body.** A tube whose radius is modulated ring by ring, with the
//!   ridges travelling along it at the body's peristaltic phase, and the
//!   front segments lifted into the air by the rear.
//! * **The mouth.** Three jaw lobes that open away from the axis, a dark
//!   throat, and a rim of crystal teeth.
//!
//! The glass register is an empty glass worm: there is nothing inside a
//! creature with no connectome, let alone one with no existence.

use dfcore::SandwormBody;

use crate::flybody::GlassPalette;
use crate::mesh::{Mesh, Vertex};

const HIDE: [f32; 4] = [0.60, 0.54, 0.44, 1.0];
const GROOVE: [f32; 4] = [0.38, 0.33, 0.26, 1.0];
const THROAT: [f32; 4] = [0.16, 0.08, 0.06, 1.0];
const TOOTH: [f32; 4] = [0.94, 0.93, 0.86, 1.0];
const RIPPLE: [f32; 4] = [0.82, 0.72, 0.50, 0.40];
const RIPPLE_CREST: [f32; 4] = [0.92, 0.84, 0.62, 0.55];

/// Body radius at fraction `t`: thick at the head, tapering a little.
fn radius_at(t: f32) -> f32 {
    4.8 - 1.6 * t * t
}

/// How far below the surface the body's *underside* sits when it is fully
/// submerged. Inside a terrarium's bed of sand, not through its floor: the
/// body also flattens as it goes under (`SQUASH`) so that it fits.
const SUBMERGE: f32 = 8.5;
/// Vertical scale of the body when fully submerged. A tunnelling body is
/// pressed into its tunnel; and the tank's sand is shallower than the worm
/// is round.
const SQUASH: f32 = 0.6;

/// How high the head rears, at the tip, in scene units.
const REAR_HEIGHT: f32 = 22.0;
/// How many of the front segments take part in the rear.
const REAR_SEGMENTS: usize = 9;

fn tri(out: &mut Mesh, p: [[f32; 3]; 3], n: [f32; 3], c: [f32; 4]) {
    let b = out.verts.len() as u32;
    for v in p {
        out.verts.push(Vertex {
            pos: v,
            normal: n,
            color: c,
        });
    }
    out.indices.extend_from_slice(&[b, b + 1, b + 2]);
}

/// The lift of spine point `i` due to the rear: a curve that is highest at
/// the head and reaches the ground by `REAR_SEGMENTS`.
fn rear_lift(i: usize, rear: f32) -> f32 {
    if i >= REAR_SEGMENTS {
        return 0.0;
    }
    let u = 1.0 - i as f32 / REAR_SEGMENTS as f32;
    REAR_HEIGHT * rear * u * u
}

/// Build one frame of the worm from its spine.
pub fn build_frame(out: &mut Mesh, worm: &SandwormBody, glass: bool) {
    out.verts.clear();
    out.indices.clear();

    let spine = &worm.spine;
    let n = spine.len();
    if n < 3 {
        return;
    }
    let up = worm.surface;
    let z0 = -SUBMERGE * (1.0 - up);
    let squash = SQUASH + (1.0 - SQUASH) * up;

    let frame = |i: usize| -> ([f32; 2], [f32; 2]) {
        let a = spine[i.saturating_sub(1)];
        let b = spine[(i + 1).min(n - 1)];
        let (mut tx, mut ty) = (b.x - a.x, b.y - a.y);
        let l = (tx * tx + ty * ty).sqrt();
        if l < 1e-4 {
            tx = worm.heading.cos();
            ty = worm.heading.sin();
        } else {
            tx /= l;
            ty /= l;
        }
        ([tx, ty], [-ty, tx])
    };

    // --- the ripple: a raised ridge of sand along the spine, present in
    // proportion to how much of the animal is under. Two strips, a broad
    // faint one and a narrow bright crest, both flat on the surface plane.
    if up < 0.98 {
        let k = 1.0 - up;
        for (width, z, c) in [
            (2.2, 0.25, RIPPLE),
            (1.0, 0.4, RIPPLE_CREST),
        ] {
            let col = if glass {
                [GlassPalette::WING[0], GlassPalette::WING[1], GlassPalette::WING[2], 0.30 * k]
            } else {
                [c[0], c[1], c[2], c[3] * k]
            };
            let base = out.verts.len() as u32;
            for i in 0..n {
                let t = i as f32 / (n - 1) as f32;
                let (_, nrm) = frame(i);
                let p = spine[i];
                let w = radius_at(t) * width;
                for side in [-1.0f32, 1.0] {
                    out.verts.push(Vertex {
                        pos: [p.x + nrm[0] * w * side, p.y + nrm[1] * w * side, z],
                        normal: [0.0, 0.0, 1.0],
                        color: col,
                    });
                }
            }
            for i in 0..n as u32 - 1 {
                let a = base + i * 2;
                out.indices
                    .extend_from_slice(&[a, a + 2, a + 1, a + 1, a + 2, a + 3]);
            }
        }
    }

    // --- body: a tube with travelling ridges. Every ring is a segment; the
    // radius swells and shrinks along the peristaltic wave, and the grooves
    // between ridges are darker.
    const RING: usize = 12;
    let ring_base = out.verts.len() as u32;
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let (_, nrm) = frame(i);
        let p = spine[i];
        let wave = (std::f32::consts::TAU * (i as f32 * 0.3 - worm.ring_phase)).sin();
        let r = radius_at(t) * (1.0 + 0.12 * wave);
        let rv = r * squash;
        let z = z0 + rv + rear_lift(i, worm.rear);
        let c = if glass {
            GlassPalette::SHELL
        } else {
            let g = (0.5 - 0.5 * wave).clamp(0.0, 1.0);
            [
                HIDE[0] + (GROOVE[0] - HIDE[0]) * g,
                HIDE[1] + (GROOVE[1] - HIDE[1]) * g,
                HIDE[2] + (GROOVE[2] - HIDE[2]) * g,
                1.0,
            ]
        };
        for k in 0..RING {
            let a = std::f32::consts::TAU * k as f32 / RING as f32;
            let (sa, ca) = a.sin_cos();
            let pos = [p.x + nrm[0] * r * ca, p.y + nrm[1] * r * ca, z + rv * sa];
            let nx = nrm[0] * ca / r.max(1e-3);
            let ny = nrm[1] * ca / r.max(1e-3);
            let nz = sa / rv.max(1e-3);
            let nl = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-4);
            let (nx, ny, nz) = (nx / nl, ny / nl, nz / nl);
            let shade = if glass { 1.0 } else { 0.72 + 0.28 * sa.max(0.0) };
            out.verts.push(Vertex {
                pos,
                normal: [nx, ny, nz],
                color: [c[0] * shade, c[1] * shade, c[2] * shade, c[3]],
            });
        }
    }
    for i in 0..n - 1 {
        for k in 0..RING {
            let k2 = (k + 1) % RING;
            let a = ring_base + (i * RING + k) as u32;
            let b = ring_base + (i * RING + k2) as u32;
            let c = ring_base + ((i + 1) * RING + k) as u32;
            let d = ring_base + ((i + 1) * RING + k2) as u32;
            out.indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    // Close the tail.
    {
        let tail = spine[n - 1];
        // `frame`'s tangent runs head-to-tail, so this is behind the tail.
        let (tan, _) = frame(n - 1);
        let tip = out.verts.len() as u32;
        let c = if glass { GlassPalette::SHELL } else { HIDE };
        let r = radius_at(1.0);
        out.verts.push(Vertex {
            pos: [tail.x + tan[0] * r * 1.2, tail.y + tan[1] * r * 1.2, z0 + r * squash],
            normal: [tan[0], tan[1], 0.0],
            color: c,
        });
        let last = ring_base + ((n - 1) * RING) as u32;
        for k in 0..RING {
            let k2 = (k + 1) % RING;
            out.indices
                .extend_from_slice(&[tip, last + k2 as u32, last + k as u32]);
        }
    }

    // --- the maw. In the books the mouth is a vast round opening ringed
    // with crystal teeth — not the films' lobes. The head end flares into a
    // rim that widens with the gape, the throat behind it is dark and
    // recessed, and the teeth stand around the rim, leaning inward when the
    // mouth is shut and standing out from it when it is open.
    {
        let head = spine[0];
        // `frame`'s tangent runs head-to-tail; the maw faces the other way.
        let (tan, nrm) = frame(0);
        let tan = [-tan[0], -tan[1]];
        let r = radius_at(0.0);
        let zc = z0 + r * squash + rear_lift(0, worm.rear);
        let lift = if worm.rear > 0.0 {
            (rear_lift(0, worm.rear) - rear_lift(1, worm.rear)) / 3.4
        } else {
            0.0
        };
        let fl = (1.0 + lift * lift).sqrt();
        let fwd = [tan[0] / fl, tan[1] / fl, lift / fl];
        let upv = [-fwd[2] * nrm[1], fwd[2] * nrm[0], fwd[0] * nrm[1] - fwd[1] * nrm[0]];
        let radial = |ang: f32| -> [f32; 3] {
            let (sn, cs) = ang.sin_cos();
            [
                nrm[0] * cs + upv[0] * sn,
                nrm[1] * cs + upv[1] * sn,
                upv[2] * sn,
            ]
        };
        let gape = worm.gape;
        let rim_r = r * (0.92 + 0.35 * gape);
        // Vertical radius follows the squash so a submerged head is flat.
        let vr = squash;
        let at = |d: [f32; 3], rad: f32, along: f32| -> [f32; 3] {
            [
                head.x + fwd[0] * along + d[0] * rad,
                head.y + fwd[1] * along + d[1] * rad,
                zc + fwd[2] * along + d[2] * rad * vr,
            ]
        };
        const MAW: usize = 18;
        // The flared rim: a short cone from the head ring out to the rim.
        let rim_c = if glass { GlassPalette::SHELL } else { HIDE };
        let rb = out.verts.len() as u32;
        for k in 0..MAW {
            let ang = std::f32::consts::TAU * k as f32 / MAW as f32;
            let d = radial(ang);
            out.verts.push(Vertex {
                pos: at(d, r, 0.0),
                normal: d,
                color: rim_c,
            });
            out.verts.push(Vertex {
                pos: at(d, rim_r, 1.2 + 0.8 * gape),
                normal: d,
                color: rim_c,
            });
        }
        for k in 0..MAW as u32 {
            let k2 = (k + 1) % MAW as u32;
            let (a, b) = (rb + k * 2, rb + k2 * 2);
            out.indices.extend_from_slice(&[a, a + 1, b, b, a + 1, b + 1]);
        }
        // The throat: a dark disc recessed into the head, so the mouth reads
        // as a hole and not a lid. Recesses further as it opens.
        let throat_c = if glass { GlassPalette::SHELL_DENSE } else { THROAT };
        let depth = -(0.6 + 2.2 * gape);
        let tb = out.verts.len() as u32;
        out.verts.push(Vertex {
            pos: at([0.0, 0.0, 0.0], 0.0, depth),
            normal: fwd,
            color: throat_c,
        });
        for k in 0..MAW {
            let ang = std::f32::consts::TAU * k as f32 / MAW as f32;
            let d = radial(ang);
            out.verts.push(Vertex {
                pos: at(d, rim_r * 0.92, depth + 0.3),
                normal: fwd,
                color: throat_c,
            });
        }
        for k in 0..MAW as u32 {
            let k2 = (k + 1) % MAW as u32;
            out.indices.extend_from_slice(&[tb, tb + 1 + k, tb + 1 + k2]);
        }
        // Crystal teeth: long, pale, in two staggered rows around the rim,
        // pointing inward and forward; the gape swings them outward.
        if !glass {
            for row in 0..2 {
                let count = MAW;
                let base_along = 1.0 + 0.8 * gape - row as f32 * 0.9;
                let len = if row == 0 { 2.6 } else { 1.8 };
                for k in 0..count {
                    let ang = std::f32::consts::TAU * (k as f32 + row as f32 * 0.5) / count as f32;
                    let d = radial(ang);
                    // Root on the rim, tip leaning toward the axis (shut) or
                    // standing forward (open).
                    let root_rad = rim_r * (0.98 - row as f32 * 0.12);
                    let inward = len * (0.85 - 0.75 * gape);
                    let forward = len * (0.35 + 0.65 * gape);
                    let root = at(d, root_rad, base_along);
                    let tip = at(d, root_rad - inward, base_along + forward);
                    let side = [
                        -d[1] * 0.28 + fwd[0] * 0.0,
                        d[0] * 0.28,
                        0.0,
                    ];
                    let n3 = [d[0] * 0.6 + fwd[0], d[1] * 0.6 + fwd[1], d[2] * 0.6 + fwd[2]];
                    tri(
                        out,
                        [
                            [root[0] - side[0], root[1] - side[1], root[2]],
                            [root[0] + side[0], root[1] + side[1], root[2]],
                            tip,
                        ],
                        n3,
                        TOOTH,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dfcore::Vec2;

    fn worm() -> SandwormBody {
        SandwormBody::new(Vec2::ZERO, 1)
    }

    fn bounds(m: &Mesh) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for v in &m.verts {
            for i in 0..3 {
                lo[i] = lo[i].min(v.pos[i]);
                hi[i] = hi[i].max(v.pos[i]);
            }
        }
        (lo, hi)
    }

    #[test]
    fn a_frame_produces_a_closed_mesh() {
        let mut m = Mesh::default();
        build_frame(&mut m, &worm(), false);
        assert!(m.verts.len() > 300, "only {} verts", m.verts.len());
        assert_eq!(m.indices.len() % 3, 0);
        assert!(m.indices.iter().all(|&i| (i as usize) < m.verts.len()));
    }

    /// Submerged, the body is below the surface plane and only the ripple
    /// is at it; surfaced, the body is above and there is no ripple.
    #[test]
    fn under_the_sand_only_the_ripple_shows() {
        let mut under = worm();
        under.surface = 0.0;
        let mut up = worm();
        up.surface = 1.0;
        let mut a = Mesh::default();
        let mut b = Mesh::default();
        build_frame(&mut a, &under, false);
        build_frame(&mut b, &up, false);
        // Every body vertex is below the surface when submerged.
        let above = a.verts.iter().filter(|v| v.pos[2] > 1.0).count();
        assert_eq!(above, 0, "{above} body vertices poke through the sand");
        let ripple = a.verts.iter().filter(|v| (v.pos[2] - 0.25).abs() < 0.2).count();
        assert!(ripple > 40, "no ripple: {ripple}");
        // Surfaced: the body is up, and there is no ripple.
        let (lo, _) = bounds(&b);
        assert!(lo[2] > -0.5, "surfaced body is underground: {}", lo[2]);
        let ripple_up = b.verts.iter().filter(|v| v.color[3] < 0.95 && v.color[3] > 0.05).count();
        assert_eq!(ripple_up, 0, "ripple drawn on a surfaced worm");
    }

    #[test]
    fn rearing_lifts_the_head_and_not_the_tail() {
        let mut flat = worm();
        flat.surface = 1.0;
        flat.rear = 0.0;
        let mut reared = worm();
        reared.surface = 1.0;
        reared.rear = 1.0;
        let mut a = Mesh::default();
        let mut b = Mesh::default();
        build_frame(&mut a, &flat, false);
        build_frame(&mut b, &reared, false);
        let (_, hi_a) = bounds(&a);
        let (_, hi_b) = bounds(&b);
        assert!(hi_b[2] > hi_a[2] + 12.0, "did not rear: {} vs {}", hi_b[2], hi_a[2]);
        // The tail ring is unchanged.
        let n = flat.spine.len();
        let ring0 = 0; // no ripple when surfaced, so rings start at 0
        let tail_a = a.verts[ring0 + (n - 1) * 12].pos[2];
        let tail_b = b.verts[ring0 + (n - 1) * 12].pos[2];
        assert!((tail_a - tail_b).abs() < 0.01, "the tail moved");
    }

    #[test]
    fn the_gape_opens_a_round_maw_ringed_with_teeth() {
        let mut shut = worm();
        shut.surface = 1.0;
        shut.gape = 0.0;
        let mut open = worm();
        open.surface = 1.0;
        open.gape = 1.0;
        let mut a = Mesh::default();
        let mut b = Mesh::default();
        build_frame(&mut a, &shut, false);
        build_frame(&mut b, &open, false);
        assert_eq!(a.verts.len(), b.verts.len());
        let body = shut.spine.len() * 12 + 1;
        // The rim flares wider as it opens, measured across the axis.
        let head = shut.spine[0];
        let (tx, ty) = (shut.heading.cos(), shut.heading.sin());
        let spread = |m: &Mesh| {
            m.verts
                .iter()
                .skip(body)
                .map(|v| (-(v.pos[0] - head.x) * ty + (v.pos[1] - head.y) * tx).abs())
                .fold(0.0f32, f32::max)
        };
        assert!(spread(&b) > spread(&a) * 1.2, "the maw did not flare");
        // Teeth: a ring of them, pale, and many more than three lobes.
        let teeth = |m: &Mesh| m.verts.iter().skip(body).filter(|v| v.color == TOOTH).count() / 3;
        assert!(teeth(&a) >= 30, "only {} teeth", teeth(&a));
        // The throat is a dark recess behind the rim: further back along the
        // axis than any rim vertex when open.
        let along = |v: &Vertex| (v.pos[0] - head.x) * tx + (v.pos[1] - head.y) * ty;
        let throat_min = b.verts.iter().skip(body).filter(|v| v.color == THROAT).map(along).fold(f32::MAX, f32::min);
        let rim_min = b.verts.iter().skip(body).filter(|v| v.color != THROAT && v.color != TOOTH).map(along).fold(f32::MAX, f32::min);
        assert!(throat_min < rim_min - 1.0, "the throat is not recessed: {throat_min} vs {rim_min}");
    }

    #[test]
    fn the_rings_are_visible_as_a_radius_modulation() {
        let mut w = worm();
        w.surface = 1.0;
        let mut m = Mesh::default();
        build_frame(&mut m, &w, false);
        // Ring k's top vertex z minus its centre gives the radius; consecutive
        // rings must differ.
        let n = w.spine.len();
        let radii: Vec<f32> = (0..n)
            .map(|i| {
                let ring = &m.verts[i * 12..(i + 1) * 12];
                let c = w.spine[i];
                ring.iter()
                    .map(|v| ((v.pos[0] - c.x).powi(2) + (v.pos[1] - c.y).powi(2)).sqrt())
                    .fold(0.0f32, f32::max)
            })
            .collect();
        let mut changes = 0;
        for i in 1..n {
            if (radii[i] - radii[i - 1]).abs() > 0.15 {
                changes += 1;
            }
        }
        assert!(changes > n / 2, "the body is a smooth pipe: {changes} ridges");
    }

    #[test]
    fn the_glass_register_is_translucent_and_empty() {
        let mut w = worm();
        w.surface = 1.0;
        let mut m = Mesh::default();
        build_frame(&mut m, &w, true);
        assert!(!m.verts.is_empty());
        for v in &m.verts {
            assert!(v.color[3] < 1.0, "opaque glass: {}", v.color[3]);
        }
    }
}
