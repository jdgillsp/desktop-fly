//! The worm's enclosure: an agar plate.
//!
//! *C. elegans* is not kept in a box. It lives on the surface of a few
//! millimetres of nematode growth medium poured into a petri dish, grazing a
//! lawn of *E. coli* spread on top. So this is a shallow dish — walls a tenth
//! the height of the others — full of amber gel with a milky patch of
//! bacteria on it, the sinuous tracks the worm leaves behind, and a strip of
//! lab tape on the side where someone wrote the strain.
//!
//! The dish is square. Square plates exist and are used, and a round one would
//! either lie about where the region ends or leave the worm crawling on air
//! outside the glass.

use dfcore::{Prop, PropKind, Vec2};

use super::prims::*;
use super::{Ctx, AGAR, FLOOR_Z};
use crate::mesh::Mesh;

const DISH: [f32; 3] = [0.86, 0.90, 0.90];
const AGAR_C: [f32; 3] = [0.86, 0.72, 0.42];
const LAWN: [f32; 3] = [0.95, 0.94, 0.87];
const TRACK: [f32; 3] = [0.60, 0.48, 0.26];
const TAPE: [f32; 3] = [0.98, 0.95, 0.78];
const INK: [f32; 3] = [0.12, 0.12, 0.30];

pub fn back(out: &mut Mesh, c: &Ctx) {
    let (lo, hi) = (c.lo, c.hi);
    let agar_z = FLOOR_Z + AGAR;

    // --- the bottom of the dish, then the agar as a filled block: a darker
    // floor under it, sides along the walls, and its surface. Seen through
    // the low wall, the side gives the gel its depth.
    ground_face(out, lo, hi, FLOOR_Z, rgba(shade(AGAR_C, 0.75), 0.40));
    let inset = 0.6;
    let (alo, ahi) = (
        Vec2::new(lo.x + inset, lo.y + inset),
        Vec2::new(hi.x - inset, hi.y - inset),
    );
    let side = rgba(shade(AGAR_C, 0.9), 0.45);
    for (quad, n) in [
        (
            [
                [alo.x, ahi.y, FLOOR_Z],
                [ahi.x, ahi.y, FLOOR_Z],
                [ahi.x, ahi.y, agar_z],
                [alo.x, ahi.y, agar_z],
            ],
            [0.0, 1.0, 0.0],
        ),
        (
            [
                [alo.x, alo.y, FLOOR_Z],
                [alo.x, ahi.y, FLOOR_Z],
                [alo.x, ahi.y, agar_z],
                [alo.x, alo.y, agar_z],
            ],
            [-1.0, 0.0, 0.0],
        ),
        (
            [
                [ahi.x, alo.y, FLOOR_Z],
                [ahi.x, ahi.y, FLOOR_Z],
                [ahi.x, ahi.y, agar_z],
                [ahi.x, alo.y, agar_z],
            ],
            [1.0, 0.0, 0.0],
        ),
        (
            [
                [alo.x, alo.y, FLOOR_Z],
                [ahi.x, alo.y, FLOOR_Z],
                [ahi.x, alo.y, agar_z],
                [alo.x, alo.y, agar_z],
            ],
            [0.0, -1.0, 0.0],
        ),
    ] {
        face(out, quad, n, side);
    }
    ground_face(out, alo, ahi, agar_z, rgba(AGAR_C, 0.50));

    // --- tracks: the trails a worm cuts into the surface as it goes, drawn
    // as faint sinuous grooves. Fixed per plate; the live worm adds none,
    // because a trail that persisted would be state the plate does not keep.
    let mut s = c.scatter();
    let size = c.h.region.size;
    for _ in 0..5 {
        let x0 = lo.x + 12.0 + s.next() * (size.0 - 24.0);
        let y0 = lo.y + 12.0 + s.next() * (size.1 - 24.0);
        let ang = s.range(0.0, std::f32::consts::TAU);
        let len = s.range(60.0, 160.0);
        let amp = s.range(3.0, 6.0);
        let wl = s.range(14.0, 22.0);
        let (dx, dy) = (ang.cos(), ang.sin());
        let pts: Vec<Vec2> = (0..=18)
            .map(|k| {
                let t = k as f32 / 18.0 * len;
                let w = (t / wl * std::f32::consts::TAU).sin() * amp;
                c.h.region.clamp_inside(
                    Vec2::new(x0 + dx * t - dy * w, y0 + dy * t + dx * w),
                    4.0,
                )
            })
            .collect();
        ribbon(out, &pts, agar_z + 0.15, 1.6, rgba(TRACK, 0.22));
    }

    // --- the far dish walls: clear polystyrene, low.
    c.far_walls(out, DISH, 0.14, 0.40);

    // --- a strip of tape on the front, with a couple of marker strokes. No
    // text: the app draws none anywhere, and the strokes read as writing at
    // this size.
    let w = size.0;
    let (tx0, tx1) = (lo.x + w * 0.08, lo.x + w * 0.34);
    slab(
        out,
        [tx0, lo.y + 0.4, FLOOR_Z + 4.0],
        [tx1, lo.y + 1.0, FLOOR_Z + 14.0],
        rgba(TAPE, 0.92),
    );
    let ink = rgba(INK, 0.85);
    for (a, b, z) in [
        (0.06, 0.45, 10.5),
        (0.52, 0.80, 10.5),
        (0.06, 0.30, 7.2),
        (0.36, 0.86, 7.2),
    ] {
        slab(
            out,
            [tx0 + (tx1 - tx0) * a, lo.y + 1.0, FLOOR_Z + z - 0.8],
            [tx0 + (tx1 - tx0) * b, lo.y + 1.3, FLOOR_Z + z + 0.8],
            ink,
        );
    }

    for prop in &c.h.props {
        prop_mesh(out, c, prop);
    }
}

pub fn front(out: &mut Mesh, c: &Ctx, grabbed: bool) {
    c.front_pane(out, DISH, 0.06, 0.28);
    c.near_side(out, DISH, 0.06, 0.28);
    // The lip of the dish: a thicker, whiter edge than a tank's, because the
    // dish is moulded rather than cut.
    let t = if grabbed { 4.0 } else { 3.0 };
    c.rim(out, t, rgba(DISH, if grabbed { 0.92 } else { 0.70 }));
}

fn prop_mesh(out: &mut Mesh, c: &Ctx, prop: &Prop) {
    if !prop.present() {
        return;
    }
    if prop.kind == PropKind::Lawn {
        // A lawn of bacteria: a milky patch with a soft, ragged edge, denser
        // toward the middle. Three translucent layers do the softness; the
        // wobble does the raggedness.
        let z = FLOOR_Z + AGAR + 0.2;
        for (i, k) in [1.0f32, 0.8, 0.58].iter().enumerate() {
            blob(
                out,
                prop.pos,
                z + i as f32 * 0.15,
                prop.radius * k,
                0.5,
                0.10,
                prop.radius + i as f32 * 1.3,
                rgba(LAWN, 0.30),
            );
        }
        // And it is thinner where the worm has been grazing.
        let mut s = Scatter::new((prop.radius * 11.0) as u32);
        for _ in 0..4 {
            let a = s.range(0.0, std::f32::consts::TAU);
            let d = s.range(0.0, prop.radius * 0.6);
            blob(
                out,
                Vec2::new(prop.pos.x + a.cos() * d, prop.pos.y + a.sin() * d),
                z + 0.6,
                s.range(4.0, 9.0),
                0.2,
                0.2,
                a,
                rgba(AGAR_C, 0.28),
            );
        }
    }
    let _ = c;
}
