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
        let (tan, _) = frame(n - 1);
        let tip = out.verts.len() as u32;
        let c = if glass { GlassPalette::SHELL } else { HIDE };
        let r = radius_at(1.0);
        out.verts.push(Vertex {
            pos: [tail.x - tan[0] * r * 1.2, tail.y - tan[1] * r * 1.2, z0 + r * squash],
            normal: [-tan[0], -tan[1], 0.0],
            color: c,
        });
        let last = ring_base + ((n - 1) * RING) as u32;
        for k in 0..RING {
            let k2 = (k + 1) % RING;
            out.indices
                .extend_from_slice(&[tip, last + k2 as u32, last + k as u32]);
        }
    }

    // --- the mouth: three lobes hinged on the head ring, opening outward
    // with the gape; a throat behind them; teeth around the rim.
    {
        let head = spine[0];
        let (tan, nrm) = frame(0);
        let r = radius_at(0.0);
        let zc = z0 + r * squash + rear_lift(0, worm.rear);
        // The head's own forward axis, tilted up by the rear.
        let lift = if worm.rear > 0.0 {
            (rear_lift(0, worm.rear) - rear_lift(1, worm.rear)) / 3.4
        } else {
            0.0
        };
        let fl = (1.0 + lift * lift).sqrt();
        let fwd = [tan[0] / fl, tan[1] / fl, lift / fl];
        let gape = worm.gape;
        // Throat: a dark disc just inside the head ring.
        let throat_c = if glass { GlassPalette::SHELL_DENSE } else { THROAT };
        let tb = out.verts.len() as u32;
        out.verts.push(Vertex {
            pos: [head.x + fwd[0] * 0.4, head.y + fwd[1] * 0.4, zc + fwd[2] * 0.4],
            normal: fwd,
            color: throat_c,
        });
        const LOBE_SEG: usize = 12;
        for k in 0..LOBE_SEG {
            let a = std::f32::consts::TAU * k as f32 / LOBE_SEG as f32;
            let (sa, ca) = a.sin_cos();
            // A radial basis perpendicular to fwd: nrm, and fwd x nrm.
            let upv = [-fwd[2] * nrm[1], fwd[2] * nrm[0], fwd[0] * nrm[1] - fwd[1] * nrm[0]];
            let rr = r * 0.85;
            out.verts.push(Vertex {
                pos: [
                    head.x + fwd[0] * 0.4 + nrm[0] * rr * ca + upv[0] * rr * sa,
                    head.y + fwd[1] * 0.4 + nrm[1] * rr * ca + upv[1] * rr * sa,
                    zc + fwd[2] * 0.4 + upv[2] * rr * sa,
                ],
                normal: fwd,
                color: throat_c,
            });
        }
        for k in 0..LOBE_SEG as u32 {
            let k2 = (k + 1) % LOBE_SEG as u32;
            out.indices.extend_from_slice(&[tb, tb + 1 + k, tb + 1 + k2]);
        }
        // Three lobes, each a fan of two triangles hinged on the rim, swung
        // out by the gape. Closed, they meet at a point ahead of the head.
        let lobe_c = if glass { GlassPalette::SHELL } else { HIDE };
        let reach = r * 1.5;
        for lobe in 0..3 {
            let a0 = std::f32::consts::TAU * lobe as f32 / 3.0;
            let mid = a0 + std::f32::consts::PI / 3.0;
            let upv = [-fwd[2] * nrm[1], fwd[2] * nrm[0], fwd[0] * nrm[1] - fwd[1] * nrm[0]];
            let radial = |ang: f32| -> [f32; 3] {
                let (s, c) = ang.sin_cos();
                [
                    nrm[0] * c + upv[0] * s,
                    nrm[1] * c + upv[1] * s,
                    upv[2] * s,
                ]
            };
            let hinge = |ang: f32| -> [f32; 3] {
                let d = radial(ang);
                [
                    head.x + fwd[0] * 0.6 + d[0] * r,
                    head.y + fwd[1] * 0.6 + d[1] * r,
                    zc + fwd[2] * 0.6 + d[2] * r,
                ]
            };
            // The tip swings from "closed, ahead on the axis" to "open, out
            // along the lobe's own radial".
            let d = radial(mid);
            let out_k = gape * 1.1;
            let ahead = reach * (1.0 - gape * 0.6);
            let tip = [
                head.x + fwd[0] * ahead + d[0] * r * (0.2 + out_k),
                head.y + fwd[1] * ahead + d[1] * r * (0.2 + out_k),
                zc + fwd[2] * ahead + d[2] * r * (0.2 + out_k),
            ];
            let h0 = hinge(a0);
            let h1 = hinge(a0 + std::f32::consts::TAU / 3.0);
            let hm = hinge(mid);
            let nrm3 = [d[0] * 0.5 + fwd[0], d[1] * 0.5 + fwd[1], d[2] * 0.5 + fwd[2]];
            tri(out, [h0, hm, tip], nrm3, lobe_c);
            tri(out, [hm, h1, tip], nrm3, lobe_c);
            // Teeth along each lobe's inner edge: small pale triangles.
            if !glass {
                for k in 0..4 {
                    let u = (k as f32 + 0.5) / 4.0;
                    let ang = a0 + u * std::f32::consts::TAU / 3.0;
                    let base = hinge(ang);
                    let dr = radial(ang);
                    let inward = 0.55 + gape * 0.9;
                    let t2 = [
                        base[0] + fwd[0] * 0.9 - dr[0] * inward,
                        base[1] + fwd[1] * 0.9 - dr[1] * inward,
                        base[2] + fwd[2] * 0.9 - dr[2] * inward,
                    ];
                    let side = [dr[1] * 0.3, -dr[0] * 0.3, 0.0];
                    tri(
                        out,
                        [
                            [base[0] - side[0], base[1] - side[1], base[2]],
                            [base[0] + side[0], base[1] + side[1], base[2]],
                            t2,
                        ],
                        fwd,
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
    fn the_gape_opens_the_lobes() {
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
        // Open lobes reach further *from the axis* than shut ones, which
        // reach further forward along it.
        let head = shut.spine[0];
        let (tx, ty) = (shut.heading.cos(), shut.heading.sin());
        let spread = |m: &Mesh| {
            m.verts
                .iter()
                .skip(shut.spine.len() * 12 + 1)
                .map(|v| (-(v.pos[0] - head.x) * ty + (v.pos[1] - head.y) * tx).abs())
                .fold(0.0f32, f32::max)
        };
        assert!(spread(&b) > spread(&a) * 1.15, "lobes did not open");
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
