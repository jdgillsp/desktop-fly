//! The burrowers' enclosure: a sand terrarium.
//!
//! A hognose is kept in a low glass tank with a bed of sand deep enough to
//! disappear into, a hide to rest under, a water dish, and not much else —
//! the point of the animal is that most of the time you cannot see it. So
//! the sand is a *block*, like the plate's agar, with a top surface the
//! creature sinks beneath, and the walls are lower than a vivarium's because
//! nothing in here climbs.
//!
//! The sandworm gets the same tank. The fiction would object to the scale;
//! the honest reading is a very small worm in a very large desert.

use dfcore::{Prop, PropKind, Vec2};

use super::prims::*;
use super::{Ctx, FLOOR_Z, SAND};
use crate::mesh::Mesh;

const GLASS: [f32; 3] = [0.84, 0.88, 0.86];
const SAND_C: [f32; 3] = [0.88, 0.80, 0.60];
const SAND_DEEP: [f32; 3] = [0.74, 0.64, 0.44];
const GRAIN: [f32; 3] = [0.66, 0.56, 0.38];
const CORK: [f32; 3] = [0.46, 0.32, 0.20];
const STONE: [f32; 3] = [0.58, 0.55, 0.50];
const DISH: [f32; 3] = [0.30, 0.30, 0.32];
const WATER: [f32; 3] = [0.42, 0.66, 0.74];
const SUCCULENT: [f32; 3] = [0.44, 0.58, 0.36];

pub fn back(out: &mut Mesh, c: &Ctx) {
    let (lo, hi) = (c.lo, c.hi);
    let sand_z = FLOOR_Z + SAND;
    let size = c.h.region.size;

    // --- the tank floor, then the sand as a block: darker down the sides,
    // where it is seen through the glass and packed, pale on top.
    ground_face(out, lo, hi, FLOOR_Z, rgba(shade(SAND_DEEP, 0.58), 0.45));
    let inset = 0.5;
    let (slo, shi) = (
        Vec2::new(lo.x + inset, lo.y + inset),
        Vec2::new(hi.x - inset, hi.y - inset),
    );
    let side = rgba(SAND_DEEP, 0.55);
    for (quad, n) in [
        (
            [
                [slo.x, shi.y, FLOOR_Z],
                [shi.x, shi.y, FLOOR_Z],
                [shi.x, shi.y, sand_z],
                [slo.x, shi.y, sand_z],
            ],
            [0.0, 1.0, 0.0],
        ),
        (
            [
                [slo.x, slo.y, FLOOR_Z],
                [slo.x, shi.y, FLOOR_Z],
                [slo.x, shi.y, sand_z],
                [slo.x, slo.y, sand_z],
            ],
            [-1.0, 0.0, 0.0],
        ),
        (
            [
                [shi.x, slo.y, FLOOR_Z],
                [shi.x, shi.y, FLOOR_Z],
                [shi.x, shi.y, sand_z],
                [shi.x, slo.y, sand_z],
            ],
            [1.0, 0.0, 0.0],
        ),
        (
            [
                [slo.x, slo.y, FLOOR_Z],
                [shi.x, slo.y, FLOOR_Z],
                [shi.x, slo.y, sand_z],
                [slo.x, slo.y, sand_z],
            ],
            [0.0, -1.0, 0.0],
        ),
    ] {
        face(out, quad, n, side);
    }
    ground_face(out, slo, shi, sand_z, rgba(SAND_C, 0.58));

    // --- the surface is not flat: low drifts, and a scatter of coarser
    // grains, so the sand has grain rather than reading as a beige sheet.
    let mut s = c.scatter();
    for i in 0..7 {
        let x0 = lo.x + 20.0 + s.next() * (size.0 - 40.0);
        let y0 = lo.y + 20.0 + s.next() * (size.1 - 40.0);
        let ang = s.range(0.0, std::f32::consts::TAU);
        let len = s.range(70.0, 180.0);
        let amp = s.range(4.0, 9.0);
        let (dx, dy) = (ang.cos(), ang.sin());
        let pts: Vec<Vec2> = (0..=14)
            .map(|k| {
                let t = k as f32 / 14.0 * len;
                let w = (t / len * std::f32::consts::PI).sin() * amp;
                c.h.region
                    .clamp_inside(Vec2::new(x0 + dx * t - dy * w, y0 + dy * t + dx * w), 6.0)
            })
            .collect();
        let tone = if i % 2 == 0 { 1.06 } else { 0.94 };
        ribbon(out, &pts, sand_z + 0.12, s.range(6.0, 12.0), rgba(shade(SAND_C, tone), 0.30));
    }
    for i in 0..40 {
        let x = lo.x + 8.0 + s.next() * (size.0 - 16.0);
        let y = lo.y + 8.0 + s.next() * (size.1 - 16.0);
        let r = 0.8 + s.next() * 1.6;
        blob(
            out,
            Vec2::new(x, y),
            sand_z + 0.2,
            r,
            r * 0.5,
            0.2,
            i as f32 * 0.7,
            rgba(shade(GRAIN, 0.8 + s.next() * 0.4), 0.7),
        );
    }

    // --- the water dish: a shallow black bowl sunk into the sand in a front
    // corner, with water in it. Fixed furniture, not a prop: every terrarium
    // has one and it is not something to take out.
    {
        let r = (size.0.min(size.1) * 0.09).clamp(14.0, 26.0);
        let at = Vec2::new(hi.x - r - 18.0, lo.y + r + 18.0);
        blob(out, at, sand_z + 0.3, r + 2.5, 1.8, 0.05, 3.0, rgba(DISH, 0.9));
        disc(out, [at.x, at.y, sand_z + 1.6], r, None, rgba(WATER, 0.55));
        // A bright ring where the water meets the rim.
        for k in 0..24 {
            let a = std::f32::consts::TAU * k as f32 / 24.0;
            let b = a + std::f32::consts::TAU / 24.0;
            let (sa, ca) = a.sin_cos();
            let (sb, cb) = b.sin_cos();
            tri(
                out,
                [
                    [at.x + ca * r, at.y + sa * r, sand_z + 1.7],
                    [at.x + cb * r, at.y + sb * r, sand_z + 1.7],
                    [at.x + cb * r * 0.9, at.y + sb * r * 0.9, sand_z + 1.7],
                ],
                [0.0, 0.0, 1.0],
                rgba([0.80, 0.92, 0.95], 0.5),
            );
        }
    }

    // --- the far glass.
    c.far_walls(out, GLASS, 0.10, 0.36);

    for prop in &c.h.props {
        prop_mesh(out, c, prop);
    }
}

pub fn front(out: &mut Mesh, c: &Ctx, grabbed: bool) {
    c.front_pane(out, GLASS, 0.05, 0.30);
    c.near_side(out, GLASS, 0.05, 0.30);
    let t = if grabbed { 3.0 } else { 1.6 };
    c.rim(out, t, rgba(GLASS, if grabbed { 0.92 } else { 0.55 }));
}

pub(crate) fn prop_mesh(out: &mut Mesh, c: &Ctx, prop: &Prop) {
    if !prop.present() {
        return;
    }
    let sand_z = FLOOR_Z + SAND;
    match prop.kind {
        PropKind::Hide => {
            // A half-round of cork: a tube lying on the sand with its axis
            // along x, its centre *at* the surface so that the lower half is
            // buried and the visible part is the arch a snake goes under.
            let r = prop.radius * 0.5;
            let half = prop.radius * 1.1;
            let at = c.h.region.clamp_inside(prop.pos, half + 4.0);
            contact_shade(out, at, sand_z + 0.25, prop.radius * 1.1);
            // Centre a little above the surface: the arch stands, the
            // buried part stays inside the sand and above the tank floor.
            let zc = sand_z + r * 0.3;
            tube(
                out,
                [at.x - half, at.y, zc],
                [at.x + half, at.y, zc],
                r,
                12,
                false,
                rgba(CORK, 0.94),
            );
            // Bark texture: a few darker ridges along the top.
            for k in 0..3 {
                let y = at.y + (k as f32 - 1.0) * r * 0.45;
                let z = zc + (r * r - (y - at.y).powi(2)).max(0.0).sqrt() + 0.15;
                ribbon(
                    out,
                    &[Vec2::new(at.x - half * 0.9, y), Vec2::new(at.x + half * 0.9, y)],
                    z,
                    0.9,
                    rgba(shade(CORK, 0.7), 0.8),
                );
            }
        }
        PropKind::Cobble => {
            contact_shade(out, prop.pos, sand_z + 0.2, prop.radius);
            let k = if prop.variant == 0 { 1.0 } else { 0.8 };
            blob(
                out,
                prop.pos,
                sand_z + 0.6,
                prop.radius,
                prop.radius * 0.65,
                0.14,
                prop.radius,
                rgba(shade(STONE, k), 0.92),
            );
        }
        PropKind::Pebble => {
            blob(
                out,
                prop.pos,
                sand_z + 0.4,
                prop.radius,
                prop.radius * 0.6,
                0.18,
                prop.radius * 2.0,
                rgba(shade(STONE, 0.9), 0.9),
            );
        }
        PropKind::Plant => {
            // A succulent: a tuft, stiff, that barely stirs.
            tuft(
                out,
                prop.pos,
                sand_z + 0.3,
                prop.radius,
                prop.stir * 0.3,
                prop.phase,
                rgba(SUCCULENT, 0.9),
                c.h.region,
            );
        }
        _ => {}
    }
}
