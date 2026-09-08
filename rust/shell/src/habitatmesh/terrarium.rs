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

use dfcore::{Prop, PropKind, Region, Vec2};

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
const HEAT: [f32; 3] = [0.95, 0.42, 0.18];

/// Where the water dish sits and how big it is: a front corner at the *cool*
/// end, away from the heat mat — which is also where a keeper puts it, so
/// it does not evaporate. Public because the sandworm's runtime has to know
/// where the water is; the body avoids it.
pub(crate) fn water_dish(region: &Region) -> (Vec2, f32) {
    let lo = region.min();
    let size = region.size;
    let r = (size.0.min(size.1) * 0.09).clamp(14.0, 26.0);
    (Vec2::new(lo.x + r + 18.0, lo.y + r + 18.0), r)
}

/// The heat mat: under the sand at the +x third of the tank.
pub(crate) fn heat_mat(region: &Region) -> (Vec2, Vec2) {
    let (lo, hi) = (region.min(), region.max());
    (
        Vec2::new(lo.x + region.size.0 * 0.64, lo.y + 6.0),
        Vec2::new(hi.x - 6.0, hi.y - 6.0),
    )
}

pub fn back(out: &mut Mesh, c: &Ctx) {
    let (lo, hi) = (c.lo, c.hi);
    let sand_z = FLOOR_Z + SAND;
    let size = c.h.region.size;

    // --- the tank floor, then the sand as a block: darker down the sides,
    // where it is seen through the glass and packed, pale on top.
    ground_face(out, lo, hi, FLOOR_Z, rgba(shade(SAND_DEEP, 0.58), 0.45));
    // The heat mat under one end: a warm glow through the floor, and a
    // fainter one through the sand above it, so the tank reads as having a
    // warm end and a cool end.
    let (mlo, mhi) = heat_mat(&c.h.region);
    ground_face(out, mlo, mhi, FLOOR_Z + 0.2, rgba(HEAT, 0.30));
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
    substrate(
        out,
        slo,
        shi,
        sand_z,
        0.10,
        0.12,
        (size.0 * 31.0 + size.1 * 17.0) as u32,
        rgba(SAND_C, 0.58),
    );
    ground_face(out, mlo, mhi, sand_z + 0.11, rgba(HEAT, 0.07));

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
        ribbon(
            out,
            &pts,
            sand_z + 0.12,
            s.range(6.0, 12.0),
            rgba(shade(SAND_C, tone), 0.30),
        );
    }
    for i in 0..100 {
        let x = lo.x + 8.0 + s.next() * (size.0 - 16.0);
        let y = lo.y + 8.0 + s.next() * (size.1 - 16.0);
        let r = 0.45 + s.next() * 1.25;
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
    // corner at the cool end, with water in it. Fixed furniture, not a prop:
    // every terrarium has one and it is not something to take out.
    {
        let (at, r) = water_dish(&c.h.region);
        blob(
            out,
            at,
            sand_z + 0.3,
            r + 2.5,
            1.8,
            0.05,
            3.0,
            rgba(DISH, 0.9),
        );
        let water_start = out.verts.len();
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
        for v in &mut out.verts[water_start..] {
            v.material = Material::WET;
        }
    }

    for pair in c.h.tracks.windows(2) {
        if pair[0].0.dist(pair[1].0) < 20.0 {
            ribbon(
                out,
                &[pair[0].0, pair[1].0],
                FLOOR_Z + SAND + 0.15,
                1.8,
                rgba([0.48, 0.35, 0.20], (1.0 - pair[0].1 / 25.0).max(0.0) * 0.24),
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
            for k in 0..7 {
                let y = at.y + (k as f32 - 3.0) * r * 0.25;
                let z = zc + (r * r - (y - at.y).powi(2)).max(0.0).sqrt() + 0.15;
                ribbon(
                    out,
                    &[
                        Vec2::new(at.x - half * 0.9, y),
                        Vec2::new(at.x - half * 0.15, y + 0.4),
                        Vec2::new(at.x + half * 0.9, y),
                    ],
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
