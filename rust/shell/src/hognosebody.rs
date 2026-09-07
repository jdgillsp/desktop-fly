//! The hognose's geometry: a snake, swept along the spine the burrower computes.
//!
//! `dfcore::hognose::Hognose` produces the spine, the hood, the strike, the
//! roll and the burial; nothing here decides how the animal moves. This builds
//! a round-sectioned tube around the spine, widens the neck by the hood,
//! turns the colouring over as the animal rolls onto its back, raises the
//! upturned snout, flicks the tongue, and sinks the whole thing under the
//! substrate as it burrows.
//!
//! The western hognose pattern is the literal register: a sandy ground with
//! dark saddles down the back. The belly, which is what you see when it plays
//! dead, is cream with black blotches — and it is drawn, because the death
//! act with the wrong side up is a snake lying down.
//!
//! The glass register has nothing to put inside this body — a procedural
//! creature has no connectome — and renders an empty glass snake.

use dfcore::HognoseBody;

use crate::flybody::GlassPalette;
use crate::mesh::{Mesh, Vertex};

const GROUND: [f32; 4] = [0.74, 0.60, 0.40, 1.0];
const SADDLE: [f32; 4] = [0.40, 0.26, 0.14, 1.0];
const BELLY: [f32; 4] = [0.90, 0.86, 0.72, 1.0];
const BLOTCH: [f32; 4] = [0.10, 0.08, 0.07, 1.0];
const EYE: [f32; 4] = [0.12, 0.10, 0.08, 1.0];
const TONGUE: [f32; 4] = [0.40, 0.08, 0.08, 1.0];
const MOUTH: [f32; 4] = [0.86, 0.48, 0.52, 1.0];

/// Height of the body's centreline above the substrate plane when it is out.
const Z: f32 = 2.4;
/// How far down it goes when fully buried: past the sand's own thickness,
/// so the tank's surface hides it.
const BURY_DEPTH: f32 = 9.0;

/// Half-width at body fraction `t`. A blunt head, a slightly narrower neck,
/// a stout body and a short tail — hognoses are heavy-bodied snakes.
fn radius_at(t: f32) -> f32 {
    if t < 0.06 {
        2.3 + 0.4 * (t / 0.06)
    } else if t < 0.14 {
        2.7 - 0.6 * ((t - 0.06) / 0.08)
    } else if t < 0.55 {
        2.1 + 0.9 * ((t - 0.14) / 0.41).sqrt()
    } else {
        let u = (t - 0.55) / 0.45;
        3.0 * (1.0 - u).powf(1.4) + 0.35
    }
}

/// The hood: the neck spreads sideways and flattens. Strongest a little
/// behind the head, gone by a fifth of the way back.
fn hood_at(t: f32, hood: f32) -> f32 {
    if !(0.04..0.24).contains(&t) {
        return 0.0;
    }
    let u = (t - 0.04) / 0.20;
    let bell = (u * std::f32::consts::PI).sin();
    hood * bell * 1.5
}

fn mix(a: [f32; 4], b: [f32; 4], k: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * k,
        a[1] + (b[1] - a[1]) * k,
        a[2] + (b[2] - a[2]) * k,
        a[3] + (b[3] - a[3]) * k,
    ]
}

/// Dorsal colouring at body fraction `t`: saddles every so often, on ground.
fn dorsal_color(t: f32) -> [f32; 4] {
    let period = 0.085;
    let u = (t / period).fract();
    if u < 0.42 { SADDLE } else { GROUND }
}

/// Ventral colouring: cream, with a black blotch pattern offset from the saddles.
fn ventral_color(t: f32) -> [f32; 4] {
    let period = 0.11;
    let u = ((t + 0.03) / period).fract();
    if u < 0.30 { BLOTCH } else { BELLY }
}

/// What the top-down camera sees at `t`: the back, or — rolled over — the
/// belly. `roll` is the body's `belly_up`.
fn body_color(t: f32, roll: f32, glass: bool) -> [f32; 4] {
    if glass {
        return GlassPalette::SHELL;
    }
    mix(dorsal_color(t), ventral_color(t), roll)
}

fn quad(out: &mut Mesh, p: [[f32; 3]; 4], n: [f32; 3], c: [f32; 4]) {
    let b = out.verts.len() as u32;
    for v in p {
        out.verts.push(Vertex {
            pos: v,
            normal: n,
            color: c,
        });
    }
    out.indices
        .extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
}

fn fan(out: &mut Mesh, root: [f32; 3], rim: &[[f32; 3]], c: [f32; 4]) {
    if rim.len() < 2 {
        return;
    }
    let base = out.verts.len() as u32;
    out.verts.push(Vertex {
        pos: root,
        normal: [0.0, 0.0, 1.0],
        color: c,
    });
    for p in rim {
        out.verts.push(Vertex {
            pos: *p,
            normal: [0.0, 0.0, 1.0],
            color: c,
        });
    }
    for k in 0..rim.len() as u32 - 1 {
        out.indices
            .extend_from_slice(&[base, base + 1 + k, base + 2 + k]);
    }
}

/// Build one frame of the snake from its spine.
pub fn build_frame(out: &mut Mesh, snake: &HognoseBody, glass: bool) {
    out.verts.clear();
    out.indices.clear();

    let spine = &snake.spine;
    let n = spine.len();
    if n < 3 {
        return;
    }
    let roll = snake.belly_up;
    let sink = snake.buried * BURY_DEPTH;
    let z0 = Z - sink;
    // Free roam has no sand to hide under, so the animal also fades as it
    // digs in; in a tank the surface hides it before the fade matters.
    let fade = if glass { 1.0 } else { 1.0 - snake.buried * 0.85 };

    let frame = |i: usize| -> ([f32; 2], [f32; 2]) {
        let a = spine[i.saturating_sub(1)];
        let b = spine[(i + 1).min(n - 1)];
        let (mut tx, mut ty) = (b.x - a.x, b.y - a.y);
        let l = (tx * tx + ty * ty).sqrt();
        if l < 1e-4 {
            tx = snake.heading.cos();
            ty = snake.heading.sin();
        } else {
            tx /= l;
            ty /= l;
        }
        ([tx, ty], [-ty, tx])
    };

    // --- body: a round tube, flattened and widened at the neck by the hood.
    const RING: usize = 10;
    let ring_base = out.verts.len() as u32;
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let (_, nrm) = frame(i);
        let p = spine[i];
        let r = radius_at(t);
        let spread = hood_at(t, snake.hood);
        let w = r + spread;
        let h = r * (1.0 - 0.45 * (spread / 1.5).min(1.0));
        let c = body_color(t, roll, glass);
        for k in 0..RING {
            let a = std::f32::consts::TAU * k as f32 / RING as f32;
            let (sa, ca) = a.sin_cos();
            let pos = [
                p.x + nrm[0] * w * ca,
                p.y + nrm[1] * w * ca,
                z0 + h * sa,
            ];
            let nx = nrm[0] * ca / w.max(1e-3);
            let ny = nrm[1] * ca / w.max(1e-3);
            let nz = sa / h.max(1e-3);
            let l = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-4);
            let shade = if glass { 1.0 } else { 0.78 + 0.22 * sa.max(0.0) };
            out.verts.push(Vertex {
                pos,
                normal: [nx / l, ny / l, nz / l],
                color: [c[0] * shade, c[1] * shade, c[2] * shade, c[3] * fade],
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

    // --- the snout: closed with a fan whose tip is *raised* — the upturned
    // rostral scale that names the animal. Rolled over, it points down.
    let head = spine[0];
    let (tan, nrm) = frame(0);
    let snout = [
        head.x + tan[0] * 2.4,
        head.y + tan[1] * 2.4,
        z0 + 1.3 * (1.0 - 2.0 * roll),
    ];
    {
        let tip = out.verts.len() as u32;
        let c = body_color(0.0, roll, glass);
        out.verts.push(Vertex {
            pos: snout,
            normal: [tan[0], tan[1], 0.4],
            color: [c[0], c[1], c[2], c[3] * fade],
        });
        for k in 0..RING {
            let k2 = (k + 1) % RING;
            out.indices.extend_from_slice(&[
                tip,
                ring_base + k as u32,
                ring_base + k2 as u32,
            ]);
        }
    }

    // --- the tongue: a forked ribbon flicked out of the snout.
    if snake.tongue > 0.05 {
        let reach = 1.0 + snake.tongue * 4.0;
        let c = if glass { GlassPalette::LIMB } else { [TONGUE[0], TONGUE[1], TONGUE[2], fade] };
        let root = [snout[0], snout[1], snout[2] - 0.3];
        let stem = [
            snout[0] + tan[0] * reach * 0.6,
            snout[1] + tan[1] * reach * 0.6,
            snout[2] - 0.3,
        ];
        for side in [-1.0f32, 1.0] {
            let tip = [
                snout[0] + tan[0] * reach + nrm[0] * side * reach * 0.35,
                snout[1] + tan[1] * reach + nrm[1] * side * reach * 0.35,
                snout[2] - 0.3,
            ];
            let w = 0.28;
            quad(
                out,
                [
                    [root[0] - nrm[0] * w, root[1] - nrm[1] * w, root[2]],
                    [root[0] + nrm[0] * w, root[1] + nrm[1] * w, root[2]],
                    [stem[0] + nrm[0] * w, stem[1] + nrm[1] * w, stem[2]],
                    [stem[0] - nrm[0] * w, stem[1] - nrm[1] * w, stem[2]],
                ],
                [0.0, 0.0, 1.0],
                c,
            );
            quad(
                out,
                [
                    [stem[0] - nrm[0] * w, stem[1] - nrm[1] * w, stem[2]],
                    [stem[0] + nrm[0] * w, stem[1] + nrm[1] * w, stem[2]],
                    [tip[0] + nrm[0] * w * 0.5, tip[1] + nrm[1] * w * 0.5, tip[2]],
                    [tip[0] - nrm[0] * w * 0.5, tip[1] - nrm[1] * w * 0.5, tip[2]],
                ],
                [0.0, 0.0, 1.0],
                c,
            );
        }
    }

    // --- playing dead: the mouth hangs open. A pink wedge at the snout.
    if !glass && roll > 0.3 {
        let open = (roll - 0.3) / 0.7;
        let w = 2.0 * open;
        let root = [head.x + tan[0] * 1.0, head.y + tan[1] * 1.0, z0 + 0.5];
        let a = [
            snout[0] + tan[0] * 1.4 * open + nrm[0] * w,
            snout[1] + tan[1] * 1.4 * open + nrm[1] * w,
            z0 + 0.6,
        ];
        let b = [
            snout[0] + tan[0] * 1.4 * open - nrm[0] * w,
            snout[1] + tan[1] * 1.4 * open - nrm[1] * w,
            z0 + 0.6,
        ];
        fan(out, root, &[a, b], [MOUTH[0], MOUTH[1], MOUTH[2], fade]);
    }

    // --- eyes, on the sides of the head just behind the snout.
    if !glass && roll < 0.6 {
        for side in [-1.0f32, 1.0] {
            let e = [
                head.x + nrm[0] * side * 2.0 + tan[0] * 0.6,
                head.y + nrm[1] * side * 2.0 + tan[1] * 0.6,
                z0 + radius_at(0.0) * 0.7,
            ];
            let r = 0.55;
            quad(
                out,
                [
                    [e[0] - r, e[1] - r, e[2]],
                    [e[0] + r, e[1] - r, e[2]],
                    [e[0] + r, e[1] + r, e[2]],
                    [e[0] - r, e[1] + r, e[2]],
                ],
                [0.0, 0.0, 1.0],
                [EYE[0], EYE[1], EYE[2], fade],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dfcore::Vec2;

    fn snake() -> HognoseBody {
        HognoseBody::new(Vec2::ZERO, 1)
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

    /// Across the body, perpendicular to the head-tail axis.
    fn max_across(m: &Mesh, s: &HognoseBody, verts: usize) -> f32 {
        let head = s.spine[0];
        let tail = s.spine[s.spine.len() - 1];
        let (mut ax, mut ay) = (head.x - tail.x, head.y - tail.y);
        let l = (ax * ax + ay * ay).sqrt().max(1e-4);
        ax /= l;
        ay /= l;
        m.verts
            .iter()
            .take(verts)
            .map(|v| (-v.pos[0] * ay + v.pos[1] * ax).abs())
            .fold(0.0f32, f32::max)
    }

    #[test]
    fn a_frame_produces_a_closed_mesh() {
        let mut m = Mesh::default();
        build_frame(&mut m, &snake(), false);
        assert!(m.verts.len() > 200, "only {} verts", m.verts.len());
        assert_eq!(m.indices.len() % 3, 0);
        assert!(m.indices.iter().all(|&i| (i as usize) < m.verts.len()));
    }

    /// A snake is long and thin, and the hognose is the stout end of thin.
    #[test]
    fn the_silhouette_is_snake_shaped_from_above() {
        let s = snake();
        let mut m = Mesh::default();
        build_frame(&mut m, &s, false);
        let head = s.spine[0];
        let tail = s.spine[s.spine.len() - 1];
        let length = head.dist(tail);
        let width = 2.0 * max_across(&m, &s, s.spine.len() * 10);
        let ratio = length / width.max(1e-3);
        assert!((5.0..14.0).contains(&ratio), "length:width {ratio:.1}:1");
    }

    /// The hood has to show from above, or the bluff is a snake standing still.
    #[test]
    fn the_hood_widens_the_neck() {
        let mut calm = snake();
        calm.hood = 0.0;
        let mut hooded = snake();
        hooded.hood = 1.0;
        let mut a = Mesh::default();
        let mut b = Mesh::default();
        build_frame(&mut a, &calm, false);
        build_frame(&mut b, &hooded, false);
        // Measure the neck rings only, each against its own spine point, so
        // the undulation the spine already carries does not count.
        let neck = 2..6;
        let width = |m: &Mesh, s: &HognoseBody| {
            neck.clone()
                .flat_map(|i| {
                    let c = s.spine[i];
                    (i * 10..i * 10 + 10)
                        .map(move |k| ((m.verts[k].pos[0] - c.x).powi(2) + (m.verts[k].pos[1] - c.y).powi(2)).sqrt())
                })
                .fold(0.0f32, f32::max)
        };
        assert!(width(&b, &hooded) > width(&a, &calm) * 1.3, "no hood");
    }

    /// Playing dead has to read as *the other side of the snake*: the visible
    /// colouring changes, and the mouth opens.
    #[test]
    fn rolling_over_shows_the_belly_and_opens_the_mouth() {
        let mut up = snake();
        up.belly_up = 0.0;
        let mut over = snake();
        over.belly_up = 1.0;
        let mut a = Mesh::default();
        let mut b = Mesh::default();
        build_frame(&mut a, &up, false);
        build_frame(&mut b, &over, false);
        // Top-of-body vertex colour at mid-body: sa = 1 is index k = 2 or 3
        // of the ring (TAU * 2.5/10); take the brightest channel sum.
        let top_sum = |m: &Mesh| {
            (0..10)
                .map(|k| m.verts[10 * 10 + k].color)
                .map(|c| c[0] + c[1] + c[2])
                .fold(0.0f32, f32::max)
        };
        assert_ne!(top_sum(&a), top_sum(&b), "the colouring did not change");
        let mouth = |m: &Mesh| m.verts.iter().any(|v| v.color == MOUTH);
        assert!(!mouth(&a), "mouth open while alive");
        assert!(mouth(&b), "no mouth opened");
    }

    #[test]
    fn burrowing_sinks_it_and_fades_it() {
        let mut out = snake();
        out.buried = 0.0;
        let mut under = snake();
        under.buried = 1.0;
        let mut a = Mesh::default();
        let mut b = Mesh::default();
        build_frame(&mut a, &out, false);
        build_frame(&mut b, &under, false);
        let (_, hi_a) = bounds(&a);
        let (_, hi_b) = bounds(&b);
        assert!(hi_b[2] < hi_a[2] - 6.0, "did not sink: {} vs {}", hi_b[2], hi_a[2]);
        assert!(b.verts[0].color[3] < 0.3, "still opaque: {}", b.verts[0].color[3]);
        // In glass it sinks but does not fade, because the tank hides it.
        let mut g = Mesh::default();
        build_frame(&mut g, &under, true);
        assert!(g.verts[0].color[3] > 0.3);
    }

    #[test]
    fn the_tongue_is_drawn_when_flicked() {
        let mut inn = snake();
        inn.tongue = 0.0;
        let mut flick = snake();
        flick.tongue = 1.0;
        let mut a = Mesh::default();
        let mut b = Mesh::default();
        build_frame(&mut a, &inn, false);
        build_frame(&mut b, &flick, false);
        assert!(b.verts.len() > a.verts.len());
    }

    #[test]
    fn the_snout_is_upturned() {
        let s = snake();
        let mut m = Mesh::default();
        build_frame(&mut m, &s, false);
        let ring = s.spine.len() * 10;
        let snout = m.verts[ring].pos;
        assert!(snout[2] > Z + 0.5, "the snout is not raised: {}", snout[2]);
    }

    #[test]
    fn the_glass_register_is_translucent_and_empty() {
        let mut m = Mesh::default();
        build_frame(&mut m, &snake(), true);
        assert!(!m.verts.is_empty());
        for v in &m.verts {
            assert!(v.color[3] < 1.0, "opaque glass: {}", v.color[3]);
        }
    }

    #[test]
    fn the_literal_register_has_saddles_on_a_sandy_ground() {
        let mut saw_saddle = false;
        let mut saw_ground = false;
        for i in 0..40 {
            let c = body_color(i as f32 / 40.0, 0.0, false);
            saw_saddle |= c == SADDLE;
            saw_ground |= c == GROUND;
        }
        assert!(saw_saddle && saw_ground);
    }
}
