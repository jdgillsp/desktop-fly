//! The spider's enclosure: a tall arboreal vivarium.
//!
//! A jumping spider is kept in a box that is higher than it is wide, because
//! the animal lives on the walls and the furniture, not the floor: a slab of
//! cork bark leaning on the back, a twig climbing toward a top corner, and —
//! the one thing every *Phidippus* does — a silk hammock spun into the top
//! corner to sleep in. The walls are acrylic with rows of ventilation holes,
//! low at the front and high at the back, so air crosses the tank.

use dfcore::{Prop, PropKind, Vec2};

use super::prims::*;
use super::{Ctx, FLOOR_Z};
use crate::mesh::Mesh;

const ACRYLIC: [f32; 3] = [0.80, 0.87, 0.87];
const COCO: [f32; 3] = [0.30, 0.21, 0.13];
const BARK: [f32; 3] = [0.62, 0.46, 0.28];
const WOOD: [f32; 3] = [0.46, 0.32, 0.18];
const LEAF: [f32; 3] = [0.27, 0.52, 0.24];
const LITTER: [f32; 3] = [0.52, 0.36, 0.18];
const SILK: [f32; 3] = [0.97, 0.97, 0.95];
const VENT: [f32; 4] = [0.20, 0.22, 0.22, 0.55];

pub fn back(out: &mut Mesh, c: &Ctx) {
    let (lo, hi, top) = (c.lo, c.hi, c.top);
    let size = c.h.region.size;

    // --- floor: coco fibre, dark and fine, with some leaf litter on it.
    ground_face(out, lo, hi, FLOOR_Z, rgba(COCO, 0.56));
    let mut s = c.scatter();
    for _ in 0..70 {
        let gr = 1.4 + s.next() * 2.6;
        let at = Vec2::new(
            lo.x + gr + s.next() * (size.0 - 2.0 * gr),
            lo.y + gr + s.next() * (size.1 - 2.0 * gr),
        );
        let k = 0.7 + s.next() * 0.7;
        dome(out, at, FLOOR_Z + 0.5, gr, gr * 0.4, rgba(shade(COCO, k), 0.62));
    }
    for i in 0..6 {
        let r = 5.0 + s.next() * 4.0;
        let at = Vec2::new(
            lo.x + r + 4.0 + s.next() * (size.0 - 2.0 * r - 8.0),
            lo.y + r + 4.0 + s.next() * (size.1 - 2.0 * r - 8.0),
        );
        blob(
            out,
            at,
            FLOOR_Z + 0.7,
            r,
            0.6,
            0.25,
            i as f32 * 2.1,
            rgba(shade(LITTER, 0.8 + s.next() * 0.4), 0.8),
        );
    }

    // --- the far acrylic walls with their ventilation rows.
    c.far_walls(out, ACRYLIC, 0.16, 0.45);
    vent_row(out, c, Row::BackHigh);
    if c.near_is_lo_x() {
        vent_row(out, c, Row::RightHigh);
        vent_row(out, c, Row::RightLow);
    } else {
        vent_row(out, c, Row::LeftHigh);
        vent_row(out, c, Row::LeftLow);
    }

    // --- the silk retreat, spun into the top back-left corner.
    retreat(out, c);

    // The plant and the pebble first, then the tall things, so the bark is
    // drawn after the floor clutter it stands over.
    for prop in c.h.props.iter().filter(|p| !matches!(p.kind, PropKind::Bark | PropKind::Twig)) {
        prop_mesh(out, c, prop);
    }
    for prop in c.h.props.iter().filter(|p| matches!(p.kind, PropKind::Bark | PropKind::Twig)) {
        prop_mesh(out, c, prop);
    }
    let _ = top;
}

pub fn front(out: &mut Mesh, c: &Ctx, grabbed: bool) {
    c.front_pane(out, ACRYLIC, 0.07, 0.30);
    c.near_side(out, ACRYLIC, 0.07, 0.30);
    // The low front vent row, and the near side's rows.
    vent_row(out, c, Row::FrontLow);
    if c.near_is_lo_x() {
        vent_row(out, c, Row::LeftHigh);
        vent_row(out, c, Row::LeftLow);
    } else {
        vent_row(out, c, Row::RightHigh);
        vent_row(out, c, Row::RightLow);
    }
    let t = if grabbed { 6.0 } else { 4.0 };
    c.rim(out, t, rgba(ACRYLIC, if grabbed { 0.92 } else { 0.62 }));
}

#[cfg(test)]
pub(super) fn prop(out: &mut Mesh, c: &Ctx, p: &Prop) {
    prop_mesh(out, c, p);
}

#[derive(Clone, Copy)]
enum Row {
    BackHigh,
    LeftHigh,
    RightHigh,
    LeftLow,
    RightLow,
    FrontLow,
}

/// A row of small holes drilled through a pane, drawn as dark squares a hair
/// inside it.
fn vent_row(out: &mut Mesh, c: &Ctx, row: Row) {
    let (lo, hi, top) = (c.lo, c.hi, c.top);
    let hole = 1.4;
    let pitch = 11.0;
    let inset = 0.3;
    let z = match row {
        Row::BackHigh | Row::LeftHigh | Row::RightHigh => top - 16.0,
        _ => FLOOR_Z + 14.0,
    };
    match row {
        Row::BackHigh | Row::FrontLow => {
            let (y, n) = if matches!(row, Row::BackHigh) {
                (hi.y - inset, [0.0, -1.0, 0.0])
            } else {
                (lo.y + inset, [0.0, 1.0, 0.0])
            };
            let mut x = lo.x + 14.0;
            while x < hi.x - 12.0 {
                face(
                    out,
                    [
                        [x - hole, y, z - hole],
                        [x + hole, y, z - hole],
                        [x + hole, y, z + hole],
                        [x - hole, y, z + hole],
                    ],
                    n,
                    VENT,
                );
                x += pitch;
            }
        }
        _ => {
            let (x, n) = if matches!(row, Row::LeftHigh | Row::LeftLow) {
                (lo.x + inset, [1.0, 0.0, 0.0])
            } else {
                (hi.x - inset, [-1.0, 0.0, 0.0])
            };
            let mut y = lo.y + 14.0;
            while y < hi.y - 12.0 {
                face(
                    out,
                    [
                        [x, y - hole, z - hole],
                        [x, y + hole, z - hole],
                        [x, y + hole, z + hole],
                        [x, y - hole, z + hole],
                    ],
                    n,
                    VENT,
                );
                y += pitch;
            }
        }
    }
}

/// The silk hammock: a sagging sheet spun across the top back-left corner,
/// with the guy lines that hold it. Translucent, as silk is.
fn retreat(out: &mut Mesh, c: &Ctx) {
    let (lo, hi, top) = (c.lo, c.hi, c.top);
    let reach = (c.h.region.size.0.min(c.h.region.size.1) * 0.22).clamp(40.0, 90.0);
    let corner = [lo.x + 0.5, hi.y - 0.5, top - 6.0];
    let sag = top - 6.0 - reach * 0.32;
    let silk = rgba(SILK, 0.55);
    // A fan from the corner to an arc between the two walls, bellied down in
    // the middle.
    const SEG: usize = 8;
    let arc = |i: usize| {
        let t = i as f32 / SEG as f32;
        let a = t * std::f32::consts::FRAC_PI_2;
        // From the back wall (t = 0) round to the side wall (t = 1).
        let belly = (t * std::f32::consts::PI).sin();
        [
            lo.x + 0.5 + a.sin() * reach * (1.0 - 0.1 * belly),
            hi.y - 0.5 - a.cos() * reach * (1.0 - 0.1 * belly),
            top - 6.0 - (sag - (top - 6.0)).abs() * belly * 0.0 - reach * 0.32 * belly,
        ]
    };
    let hub = [lo.x + reach * 0.35, hi.y - reach * 0.35, sag];
    for i in 0..SEG {
        let (a, b) = (arc(i), arc(i + 1));
        tri(out, [hub, a, b], [0.3, -0.3, 0.9], silk);
        tri(out, [corner, b, a], [0.3, -0.3, 0.9], silk);
    }
    // Guy lines down the two walls and along the rim.
    let line = rgba(SILK, 0.6);
    for (a, b, n) in [
        (arc(0), [lo.x + 0.5, hi.y - 0.5, top - 6.0 - reach * 0.8], [0.0, -1.0, 0.0]),
        (arc(SEG), [lo.x + 0.5, hi.y - 0.5, top - 6.0 - reach * 0.8], [1.0, 0.0, 0.0]),
        (hub, [lo.x + reach * 0.9, hi.y - 0.5, top - 20.0], [0.0, -1.0, 0.0]),
    ] {
        let w = 0.45;
        face(
            out,
            [
                [a[0] - w, a[1], a[2]],
                [a[0] + w, a[1], a[2]],
                [b[0] + w, b[1], b[2]],
                [b[0] - w, b[1], b[2]],
            ],
            n,
            line,
        );
    }
}

fn prop_mesh(out: &mut Mesh, c: &Ctx, prop: &Prop) {
    if !prop.present() {
        return;
    }
    let region = c.h.region;
    let (lo, hi) = (c.lo, c.hi);
    match prop.kind {
        PropKind::Pebble => {
            contact_shade(out, prop.pos, FLOOR_Z + 0.45, prop.radius);
            let g = 0.44 + prop.radius * 0.006;
            blob(
                out,
                prop.pos,
                FLOOR_Z + 1.0,
                prop.radius,
                prop.radius * 0.62,
                0.12,
                prop.radius,
                rgba([g, g * 0.97, g * 0.93], 0.85),
            );
        }
        PropKind::Plant => {
            contact_shade(out, prop.pos, FLOOR_Z + 0.45, prop.radius);
            dome(
                out,
                prop.pos,
                FLOOR_Z + 0.9,
                prop.radius * 0.55,
                prop.radius * 0.4,
                rgba(shade(COCO, 0.75), 0.8),
            );
            tuft(
                out,
                prop.pos,
                FLOOR_Z,
                prop.radius,
                prop.stir,
                prop.phase,
                rgba(LEAF, 0.88),
                region,
            );
        }
        PropKind::Bark => bark(out, c, prop),
        PropKind::Twig => {
            let root = region.clamp_inside(prop.pos, 8.0);
            contact_shade(out, root, FLOOR_Z + 0.45, prop.radius * 0.5);
            // Leans toward whichever top back corner is nearer, and stops
            // short of the wall by its own leaf.
            let corner_x = if root.x < region.center.x { lo.x } else { hi.x };
            let tip = region.clamp_inside(
                Vec2::new(root.x + (corner_x - root.x) * 0.55, hi.y - 12.0),
                10.0,
            );
            let h = super::wall_height(c.h.kind) * 0.72;
            // Starts a radius up, so the end cap's ring — perpendicular to a
            // leaning axis — does not dip below the floor.
            let a = [root.x, root.y, FLOOR_Z + 3.6];
            let b = [tip.x, tip.y, FLOOR_Z + h];
            tube(out, a, b, 3.4, 10, true, rgba(WOOD, 0.92));
            // Two side twigs part-way up, each ending in a leaf, and a leaf at
            // the tip.
            let side = |t: f32, dir: f32| {
                let p = [
                    a[0] + (b[0] - a[0]) * t,
                    a[1] + (b[1] - a[1]) * t,
                    a[2] + (b[2] - a[2]) * t,
                ];
                let e = region.clamp_inside(
                    Vec2::new(p[0] + dir * 22.0, p[1] - 6.0 * dir.abs()),
                    8.0,
                );
                (p, [e.x, e.y, p[2] + 10.0])
            };
            for (t, dir) in [(0.45, -1.0), (0.7, 1.0)] {
                let (p, e) = side(t, dir);
                tube(out, p, e, 1.9, 8, true, rgba(WOOD, 0.9));
                disc(out, [e[0], e[1], e[2] + 0.5], 7.0, None, rgba(LEAF, 0.88));
            }
            disc(out, [b[0], b[1], b[2] + 0.5], 8.0, None, rgba(shade(LEAF, 1.1), 0.88));
        }
        _ => {}
    }
}

/// A slab of cork bark leaning on the back wall: a tilted board with its top
/// edge against the pane, and the fissures that make it cork.
fn bark(out: &mut Mesh, c: &Ctx, prop: &Prop) {
    let region = c.h.region;
    let hw = prop.radius * 1.5;
    let x0 = (prop.pos.x - hw).max(region.min().x + 2.0);
    let x1 = (prop.pos.x + hw).min(region.max().x - 2.0);
    let y_foot = prop.pos.y.min(c.hi.y - 6.0);
    let y_head = c.hi.y - 1.5;
    let h = super::wall_height(c.h.kind) * 0.62;
    let (z0, z1) = (FLOOR_Z + 1.0, FLOOR_Z + h);
    let th = 5.0;
    ribbon(
        out,
        &[Vec2::new(x0, y_foot), Vec2::new(x1, y_foot)],
        FLOOR_Z + 0.45,
        th * 2.6,
        [0.0, 0.0, 0.0, 0.22],
    );
    let front = rgba(BARK, 0.92);
    // The face's normal is tipped further up than the lean alone would give
    // it. The key light comes from behind the tank, so a true normal leaves
    // the whole face in ambient and the bark reads as a flat dark door; cork
    // is rough enough to catch light from anywhere, and this is that.
    let dy = y_head - y_foot;
    let l = (dy * dy + h * h).sqrt().max(1e-3);
    let n_front = {
        let n = [0.0, -h / l * 0.55, dy / l * 0.55 + 0.8];
        let k = (n[1] * n[1] + n[2] * n[2]).sqrt();
        [0.0, n[1] / k, n[2] / k]
    };
    face(
        out,
        [
            [x0, y_foot, z0],
            [x1, y_foot, z0],
            [x1, y_head, z1],
            [x0, y_head, z1],
        ],
        n_front,
        front,
    );
    // Top edge and the two ends, so it has thickness.
    let top = rgba(shade(BARK, 1.1), 0.92);
    face(
        out,
        [
            [x0, y_head, z1],
            [x1, y_head, z1],
            [x1, y_head, z1 - th],
            [x0, y_head, z1 - th],
        ],
        [0.0, 0.0, 1.0],
        top,
    );
    for (x, n) in [(x0, [-1.0, 0.0, 0.0]), (x1, [1.0, 0.0, 0.0])] {
        face(
            out,
            [
                [x, y_foot, z0],
                [x, y_head, z1],
                [x, y_head, z1 - th],
                [x, y_foot + th, z0],
            ],
            n,
            rgba(shade(BARK, 0.8), 0.92),
        );
    }
    // Fissures: dark strips running up the face.
    let mut s = Scatter::new((prop.radius * 53.0) as u32);
    let groove = rgba(shade(BARK, 0.45), 0.7);
    for _ in 0..6 {
        let fx = x0 + 4.0 + s.next() * (x1 - x0 - 8.0);
        let w = 0.8 + s.next() * 1.4;
        let (t0, t1) = {
            let a = s.next() * 0.6;
            (a, (a + 0.25 + s.next() * 0.5).min(1.0))
        };
        let wander = (s.next() - 0.5) * 8.0;
        let at = |t: f32| {
            [
                fx + wander * t,
                y_foot + dy * t - 0.3,
                z0 + (z1 - z0) * t + 0.3,
            ]
        };
        let (a, b) = (at(t0), at(t1));
        face(
            out,
            [
                [a[0] - w, a[1], a[2]],
                [a[0] + w, a[1], a[2]],
                [b[0] + w, b[1], b[2]],
                [b[0] - w, b[1], b[2]],
            ],
            n_front,
            groove,
        );
    }
}
