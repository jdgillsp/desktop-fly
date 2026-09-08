//! Glass-sided aquarium with a visible water column and natural substrate.
//! The persisted Pond identifier is retained for existing layouts.

use dfcore::{Prop, PropKind, Vec2};

use super::prims::*;
use super::{Ctx, FLOOR_Z};
use crate::mesh::Mesh;

const LINER: [f32; 3] = [0.09, 0.17, 0.19];
const WATER: [f32; 3] = [0.30, 0.56, 0.62];
const WATERLINE: [f32; 3] = [0.72, 0.91, 0.95];
const GLASS: [f32; 3] = [0.68, 0.87, 0.91];
const COBBLE: [f32; 3] = [0.56, 0.55, 0.52];
const PAD: [f32; 3] = [0.22, 0.48, 0.22];
const PETAL: [f32; 3] = [0.96, 0.62, 0.72];
const PELLET: [f32; 3] = [0.76, 0.56, 0.30];

/// How wide the coping stones are in plan, measured in from the region edge.
const COPING: f32 = 5.0;

/// Where the water surface sits: below the coping, as a pond that is actually
/// filled but not brimming looks.
pub fn water_z(c: &Ctx) -> f32 {
    FLOOR_Z + super::wall_height(c.h.kind) * 0.74
}

pub fn back(out: &mut Mesh, c: &Ctx) {
    let (lo, hi) = (c.lo, c.hi);
    let size = c.h.region.size;

    // --- the liner: dark, so the fish's colour is what you see.
    ground_face(out, lo, hi, FLOOR_Z, rgba(LINER, 0.62));

    // --- cobbles on the bottom, mostly toward the edges where they are
    // piled to hide the liner's fold.
    let stones_start = out.verts.len();
    let mut s = c.scatter();
    for i in 0..48 {
        let r = 2.2 + s.next() * 4.2;
        let edge = s.next() < 0.7;
        let (x, y) = if edge {
            // Along one of the four sides.
            let side = (s.next() * 4.0) as u32;
            let t = s.next();
            let d = COPING + r + 2.0 + s.next() * 26.0;
            match side {
                0 => (
                    lo.x + d,
                    lo.y + COPING + r + t * (size.1 - 2.0 * (COPING + r)),
                ),
                1 => (
                    hi.x - d,
                    lo.y + COPING + r + t * (size.1 - 2.0 * (COPING + r)),
                ),
                2 => (
                    lo.x + COPING + r + t * (size.0 - 2.0 * (COPING + r)),
                    lo.y + d,
                ),
                _ => (
                    lo.x + COPING + r + t * (size.0 - 2.0 * (COPING + r)),
                    hi.y - d,
                ),
            }
        } else {
            (
                lo.x + COPING + r + s.next() * (size.0 - 2.0 * (COPING + r)),
                lo.y + COPING + r + s.next() * (size.1 - 2.0 * (COPING + r)),
            )
        };
        let k = 0.75 + s.next() * 0.5;
        blob(
            out,
            Vec2::new(x, y),
            FLOOR_Z + 0.6,
            r,
            r * 0.55,
            0.15,
            i as f32 * 1.3,
            rgba(shade(COBBLE, k), 0.82),
        );
    }

    // Fine gravel between the larger river stones, fixed with the enclosure.
    for i in 0..90 {
        let r = s.range(0.5, 1.5);
        let at = c.h.region.clamp_inside(
            Vec2::new(s.range(lo.x, hi.x), s.range(lo.y, hi.y)),
            r + COPING,
        );
        blob(
            out,
            at,
            FLOOR_Z + 0.25,
            r,
            r * 0.35,
            0.12,
            i as f32,
            rgba(shade(COBBLE, s.range(0.55, 1.2)), 0.65),
        );
    }
    for v in &mut out.verts[stones_start..] {
        v.material = Material::WET;
    }
    // --- the coping: a ring of stones, each its own length and height.
    c.far_walls(out, GLASS, 0.045, 0.07);

    for prop in &c.h.props {
        if prop.kind == PropKind::Cobble {
            contact_shade(out, prop.pos, FLOOR_Z + 0.45, prop.radius);
            let k = if prop.variant == 0 { 1.0 } else { 0.78 };
            blob(
                out,
                prop.pos,
                FLOOR_Z + 1.0,
                prop.radius,
                prop.radius * 0.7,
                0.14,
                prop.radius,
                rgba(shade(COBBLE, k), 0.9),
            );
        }
    }
}

pub fn front(out: &mut Mesh, c: &Ctx, grabbed: bool) {
    let (lo, hi) = (c.lo, c.hi);
    let wz = water_z(c);
    let (wlo, whi) = (
        Vec2::new(lo.x + COPING, lo.y + COPING),
        Vec2::new(hi.x - COPING, hi.y - COPING),
    );

    // --- the surface, and the bright line where it meets the stone.
    surface(out, Material::WET, |out| {
        ground_face(out, wlo, whi, wz, rgba(WATER, 0.075))
    });
    let band = 2.5;
    for (a, b) in [
        (Vec2::new(wlo.x, whi.y - band), Vec2::new(whi.x, whi.y)),
        (Vec2::new(wlo.x, wlo.y), Vec2::new(whi.x, wlo.y + band)),
        (Vec2::new(wlo.x, wlo.y), Vec2::new(wlo.x + band, whi.y)),
        (Vec2::new(whi.x - band, wlo.y), Vec2::new(whi.x, whi.y)),
    ] {
        surface(out, Material::WET, |out| {
            ground_face(out, a, b, wz + 0.1, rgba(WATERLINE, 0.42))
        });
    }

    // --- what floats.
    for prop in &c.h.props {
        if !prop.present() {
            continue;
        }
        match prop.kind {
            PropKind::LilyPad => surface(out, Material::SKIN, |out| lily_pad(out, c, prop, wz)),
            PropKind::Food => {
                dome(
                    out,
                    prop.pos,
                    wz + 0.3,
                    prop.radius,
                    prop.radius * 0.7,
                    rgba(PELLET, 0.92),
                );
            }
            _ => {}
        }
    }

    // Near glass must follow the fish and water in draw order.
    c.near_side(out, GLASS, 0.035, 0.065);
    c.front_pane(out, GLASS, 0.03, 0.055);
    c.rim(out, 1.6, rgba(WATERLINE, 0.52));
    for x in [lo.x, hi.x] {
        for y in [lo.y, hi.y] {
            slab(
                out,
                [
                    x.max(lo.x).min(hi.x - 1.2),
                    y.max(lo.y).min(hi.y - 1.2),
                    FLOOR_Z,
                ],
                [(x + 1.2).min(hi.x), (y + 1.2).min(hi.y), c.top],
                rgba(WATERLINE, 0.32),
            );
        }
    }
    for &(at, age) in &c.h.ripples {
        let r = 3.0 + age * 13.0;
        let points: Vec<Vec2> = (0..=40)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 40.0;
                c.h.region
                    .clamp_inside(Vec2::new(at.x + a.cos() * r, at.y + a.sin() * r), COPING)
            })
            .collect();
        ribbon(
            out,
            &points,
            wz + 0.18,
            0.65,
            rgba(WATERLINE, (1.0 - age / 2.0).max(0.0) * 0.35),
        );
    }

    // --- while being moved, the top of the coping lights up; there is no
    // rim otherwise, the stones are the rim.
    if grabbed {
        c.rim(out, COPING, rgba([1.0, 1.0, 1.0], 0.55));
    }
}

/// A lily pad: a disc with a notch, floating on the surface, carrying a flower
/// if it is the flowering one. Its notch faces a fixed way per pad so it does
/// not spin as the phase advances; the pad itself drifts with the prop.
fn lily_pad(out: &mut Mesh, c: &Ctx, prop: &Prop, wz: f32) {
    let at = c.h.region.clamp_inside(prop.pos, prop.radius + COPING);
    let notch = prop.radius * 0.9;
    let r = prop.radius;
    // The pad dips a little as it is nosed.
    let z = wz + 0.4 - prop.stir * 0.6;
    disc(
        out,
        [at.x, at.y, z],
        r,
        Some((notch - 0.32, notch + 0.32)),
        rgba(PAD, 0.9),
    );
    // A lighter vein from the notch to the centre.
    ribbon(
        out,
        &[
            Vec2::new(at.x + notch.cos() * r * 0.55, at.y + notch.sin() * r * 0.55),
            Vec2::new(at.x, at.y),
        ],
        z + 0.1,
        1.0,
        rgba(shade(PAD, 1.35), 0.7),
    );
    // Radial secondary veins follow the notch orientation and stop short of the edge.
    for k in 1..8 {
        let a = notch + k as f32 * std::f32::consts::TAU / 8.0;
        ribbon(
            out,
            &[
                at,
                Vec2::new(at.x + a.cos() * r * 0.86, at.y + a.sin() * r * 0.86),
            ],
            z + 0.12,
            0.38,
            rgba(shade(PAD, 1.25), 0.48),
        );
    }
    if prop.variant == 1 {
        flower(out, at, z + 0.3, r * 0.42);
    }
}

/// A water lily: two rings of petals opening from a yellow centre.
fn flower(out: &mut Mesh, at: Vec2, z: f32, r: f32) {
    let petal = rgba(PETAL, 0.92);
    let inner = rgba(shade(PETAL, 1.05), 0.92);
    for (ring, n, spread, lift, col) in [(0, 8, 1.0, 0.45, petal), (1, 6, 0.6, 0.9, inner)] {
        for k in 0..n {
            let a = std::f32::consts::TAU * (k as f32 + ring as f32 * 0.5) / n as f32;
            let (s, c) = a.sin_cos();
            let (px, py) = (-s, c);
            let base = [at.x + c * r * 0.15, at.y + s * r * 0.15, z];
            let tip = [at.x + c * r * spread, at.y + s * r * spread, z + r * lift];
            let w = r * 0.22;
            let normal = [c * 0.3, s * 0.3, 0.9];
            tri(
                out,
                [
                    [base[0] - px * w, base[1] - py * w, base[2]],
                    [base[0] + px * w, base[1] + py * w, base[2]],
                    tip,
                ],
                normal,
                col,
            );
        }
    }
    dome(
        out,
        at,
        z,
        r * 0.2,
        r * 0.25,
        rgba([0.95, 0.80, 0.25], 0.92),
    );
}
