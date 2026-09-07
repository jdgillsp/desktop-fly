//! The koi's enclosure: a raised stone pond.
//!
//! Koi are pond fish. An aquarium was the wrong container the moment there
//! was a camera that could show a container: the fish belongs in dark water
//! under lily pads with cobbles on the bottom and a rim of stone to lean on.
//! So this is a formal raised pond: coping stones round the edge, a dark
//! liner, a scatter of river cobbles, pads on the surface with a flower on
//! one of them, and pellets floating where they were thrown in.
//!
//! The water is a *surface*, drawn in front of the fish, faint enough that
//! the fish stays visible through it and present enough that a koi rising
//! toward it visibly approaches something. A full translucent sheet across a
//! glass tank washed everything out (HABITAT_PLAN.md); here the walls are low
//! and there is no glass, and the sheet is what says "water".

use dfcore::{Prop, PropKind, Vec2};

use super::prims::*;
use super::{Ctx, FLOOR_Z};
use crate::mesh::Mesh;

const LINER: [f32; 3] = [0.09, 0.17, 0.19];
const WATER: [f32; 3] = [0.30, 0.56, 0.62];
const WATERLINE: [f32; 3] = [0.72, 0.91, 0.95];
const STONE: [f32; 3] = [0.60, 0.57, 0.51];
const COBBLE: [f32; 3] = [0.56, 0.55, 0.52];
const PAD: [f32; 3] = [0.22, 0.48, 0.22];
const PETAL: [f32; 3] = [0.96, 0.62, 0.72];
const PELLET: [f32; 3] = [0.76, 0.56, 0.30];

/// How wide the coping stones are in plan, measured in from the region edge.
const COPING: f32 = 16.0;

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
                0 => (lo.x + d, lo.y + COPING + r + t * (size.1 - 2.0 * (COPING + r))),
                1 => (hi.x - d, lo.y + COPING + r + t * (size.1 - 2.0 * (COPING + r))),
                2 => (lo.x + COPING + r + t * (size.0 - 2.0 * (COPING + r)), lo.y + d),
                _ => (lo.x + COPING + r + t * (size.0 - 2.0 * (COPING + r)), hi.y - d),
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

    // --- the coping: a ring of stones, each its own length and height.
    coping(out, c);

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
    ground_face(out, wlo, whi, wz, rgba(WATER, 0.16));
    let band = 2.5;
    for (a, b) in [
        (Vec2::new(wlo.x, whi.y - band), Vec2::new(whi.x, whi.y)),
        (Vec2::new(wlo.x, wlo.y), Vec2::new(whi.x, wlo.y + band)),
        (Vec2::new(wlo.x, wlo.y), Vec2::new(wlo.x + band, whi.y)),
        (Vec2::new(whi.x - band, wlo.y), Vec2::new(whi.x, whi.y)),
    ] {
        ground_face(out, a, b, wz + 0.1, rgba(WATERLINE, 0.42));
    }

    // --- what floats.
    for prop in &c.h.props {
        if !prop.present() {
            continue;
        }
        match prop.kind {
            PropKind::LilyPad => lily_pad(out, c, prop, wz),
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

    // --- while being moved, the top of the coping lights up; there is no
    // rim otherwise, the stones are the rim.
    if grabbed {
        c.rim(out, COPING, rgba([1.0, 1.0, 1.0], 0.55));
    }
}

/// The ring of coping stones. Each side is cut into stones of uneven length
/// with a hair of mortar between them; each stone has its own height and its
/// own shade. The first stone stands at exactly the declared wall height, so
/// the top of the pond is where placement thinks it is.
fn coping(out: &mut Mesh, c: &Ctx) {
    let (lo, hi, top) = (c.lo, c.hi, c.top);
    let mut s = Scatter::new(c.h.region.size.1 as i32 as u32 ^ 0x5bd1);
    let gap = 1.2;
    let mut first = true;
    let mut stone = |out: &mut Mesh, s: &mut Scatter, a: [f32; 2], b: [f32; 2]| {
        let h = if first { top } else { top - s.range(0.0, 9.0) };
        first = false;
        let k = 0.82 + s.next() * 0.34;
        slab(
            out,
            [a[0], a[1], FLOOR_Z],
            [b[0], b[1], h],
            rgba(shade(STONE, k), 0.93),
        );
    };
    // Front and back runs span the full width; the sides fit between them.
    for y in [lo.y, hi.y - COPING] {
        let mut x = lo.x;
        while x < hi.x - 1.0 {
            let len = s.range(34.0, 64.0).min(hi.x - x);
            stone(out, &mut s, [x + gap * 0.5, y], [x + len - gap * 0.5, y + COPING]);
            x += len;
        }
    }
    for x in [lo.x, hi.x - COPING] {
        let mut y = lo.y + COPING;
        while y < hi.y - COPING - 1.0 {
            let len = s.range(34.0, 64.0).min(hi.y - COPING - y);
            stone(out, &mut s, [x, y + gap * 0.5], [x + COPING, y + len - gap * 0.5]);
            y += len;
        }
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
    dome(out, at, z, r * 0.2, r * 0.25, rgba([0.95, 0.80, 0.25], 0.92));
}
