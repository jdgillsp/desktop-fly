//! The fly's enclosure: a framed mesh rearing cage.
//!
//! This is what a *Drosophila* population is actually kept in when it is not
//! in a vial — a light frame with fine mesh panels, a dish of cornmeal medium
//! on the floor, and whatever fruit it was baited with. The mesh is drawn as
//! what it is: nearly nothing face-on, with the weave picked out as a grid of
//! fine lines that reads as screen rather than glass. The frame is the
//! silhouette.

use dfcore::{Prop, PropKind, Vec2};

use super::prims::*;
use super::{Ctx, FLOOR_Z};
use crate::mesh::Mesh;

const FRAME: [f32; 3] = [0.93, 0.93, 0.91];
const FRAME_A: f32 = 0.92;
/// Bar thickness. Thin: the frame is aluminium extrusion, not timber.
const BAR: f32 = 3.2;
const MESH: [f32; 3] = [0.84, 0.85, 0.83];
const WEAVE: [f32; 3] = [0.58, 0.60, 0.57];
const WEAVE_PITCH: f32 = 15.0;
const PAPER: [f32; 3] = [0.91, 0.90, 0.85];
const MEDIUM: [f32; 3] = [0.80, 0.66, 0.40];

pub fn back(out: &mut Mesh, c: &Ctx) {
    let (lo, hi, top) = (c.lo, c.hi, c.top);

    // --- floor: a sheet of paper towel, as the bottom of a rearing cage is.
    ground_face(out, lo, hi, FLOOR_Z, rgba(PAPER, 0.34));
    // Faint embossed towel fibres stay under the furnishings.
    for row in 0..10 {
        let y = lo.y + 6.0 + (hi.y - lo.y - 12.0) * row as f32 / 9.0;
        ribbon(
            out,
            &[Vec2::new(lo.x + 5.0, y), Vec2::new(hi.x - 5.0, y)],
            FLOOR_Z + 0.08,
            0.65,
            rgba(shade(PAPER, 0.88), 0.14),
        );
    }
    // A few crumbs of medium and spent yeast around the floor, so it is a
    // surface that has been lived on rather than a wash of colour.
    let mut s = c.scatter();
    for _ in 0..26 {
        let gr = 1.2 + s.next() * 2.2;
        let at = Vec2::new(
            lo.x + gr + s.next() * (c.h.region.size.0 - 2.0 * gr),
            lo.y + gr + s.next() * (c.h.region.size.1 - 2.0 * gr),
        );
        let k = 0.7 + s.next() * 0.5;
        dome(
            out,
            at,
            FLOOR_Z + 0.5,
            gr,
            gr * 0.4,
            rgba(shade(MEDIUM, k), 0.6),
        );
    }

    // --- the two far panels of mesh, and their weave.
    c.far_walls(out, MESH, 0.06, 0.10);
    weave(out, c, Panel::Back, 0.30);
    weave(
        out,
        c,
        if c.near_is_lo_x() {
            Panel::Right
        } else {
            Panel::Left
        },
        0.30,
    );

    // --- the frame: bottom bars on three edges, the two back uprights and
    // the far front upright. The rest is in front of the creature and drawn
    // there.
    let frame_start = out.verts.len();
    let f = rgba(FRAME, FRAME_A);
    slab(
        out,
        [lo.x, hi.y - BAR, FLOOR_Z],
        [hi.x, hi.y, FLOOR_Z + BAR],
        f,
    );
    slab(
        out,
        [lo.x, lo.y, FLOOR_Z],
        [lo.x + BAR, hi.y, FLOOR_Z + BAR],
        f,
    );
    slab(
        out,
        [hi.x - BAR, lo.y, FLOOR_Z],
        [hi.x, hi.y, FLOOR_Z + BAR],
        f,
    );
    slab(out, [lo.x, hi.y - BAR, FLOOR_Z], [lo.x + BAR, hi.y, top], f);
    slab(out, [hi.x - BAR, hi.y - BAR, FLOOR_Z], [hi.x, hi.y, top], f);
    front_upright(out, c, !c.near_is_lo_x(), f);

    for v in &mut out.verts[frame_start..] {
        v.material = Material::METAL;
    }
    for prop in &c.h.props {
        prop_mesh(out, c, prop);
    }
}

pub fn front(out: &mut Mesh, c: &Ctx, grabbed: bool) {
    let (lo, hi, top) = (c.lo, c.hi, c.top);

    // --- the front panel and the near side panel. Fainter than the far
    // ones: they are between the viewer and the animal, and a screen you can
    // see through is the point.
    c.front_pane(out, MESH, 0.04, 0.08);
    weave(out, c, Panel::Front, 0.20);
    c.near_side(out, MESH, 0.04, 0.08);
    weave(
        out,
        c,
        if c.near_is_lo_x() {
            Panel::Left
        } else {
            Panel::Right
        },
        0.20,
    );

    // --- the rest of the frame: the near front upright, the front bottom
    // bar, and all four top bars, which light up while the cage is being
    // moved.
    let frame_start = out.verts.len();
    let f = rgba(FRAME, FRAME_A);
    slab(
        out,
        [lo.x, lo.y, FLOOR_Z],
        [hi.x, lo.y + BAR, FLOOR_Z + BAR],
        f,
    );
    front_upright(out, c, c.near_is_lo_x(), f);
    let t = if grabbed {
        rgba([1.0, 1.0, 1.0], 0.95)
    } else {
        f
    };
    slab(out, [lo.x, hi.y - BAR, top - BAR], [hi.x, hi.y, top], t);
    slab(out, [lo.x, lo.y, top - BAR], [hi.x, lo.y + BAR, top], t);
    slab(out, [lo.x, lo.y, top - BAR], [lo.x + BAR, hi.y, top], t);
    slab(out, [hi.x - BAR, lo.y, top - BAR], [hi.x, hi.y, top], t);
    // Small corner fasteners give the extrusion a manufactured scale cue.
    for x in [lo.x + BAR * 0.5, hi.x - BAR * 0.5] {
        tube(
            out,
            [x, lo.y + 0.1, top - BAR * 0.5],
            [x, lo.y + 0.4, top - BAR * 0.5],
            0.85,
            8,
            true,
            rgba([0.4, 0.42, 0.42], 0.9),
        );
    }
    for v in &mut out.verts[frame_start..] {
        v.material = Material::METAL;
    }
}

/// One of the two front uprights of the frame: the −x one or the +x one.
fn front_upright(out: &mut Mesh, c: &Ctx, at_lo_x: bool, color: [f32; 4]) {
    let (lo, hi, top) = (c.lo, c.hi, c.top);
    if at_lo_x {
        slab(
            out,
            [lo.x, lo.y, FLOOR_Z],
            [lo.x + BAR, lo.y + BAR, top],
            color,
        );
    } else {
        slab(
            out,
            [hi.x - BAR, lo.y, FLOOR_Z],
            [hi.x, lo.y + BAR, top],
            color,
        );
    }
}

#[derive(Clone, Copy)]
enum Panel {
    Back,
    Front,
    Left,
    Right,
}

/// The weave of a mesh panel: a grid of fine lines in the plane of the pane,
/// a hair inside it. Sub-pixel lines alias into shimmer, so these are a full
/// unit wide and faint rather than thin and strong.
fn weave(out: &mut Mesh, c: &Ctx, panel: Panel, alpha: f32) {
    let (lo, hi, top) = (c.lo, c.hi, c.top);
    let w = 1.0;
    let col = rgba(WEAVE, alpha);
    let inset = 0.3;
    let (z0, z1) = (FLOOR_Z + BAR, top - BAR);
    match panel {
        Panel::Back | Panel::Front => {
            let (y, n) = match panel {
                Panel::Back => (hi.y - inset, [0.0, -1.0, 0.0]),
                _ => (lo.y + inset, [0.0, 1.0, 0.0]),
            };
            let mut x = lo.x + BAR + WEAVE_PITCH;
            while x < hi.x - BAR {
                face(
                    out,
                    [
                        [x - w, y, z0],
                        [x + w, y, z0],
                        [x + w, y, z1],
                        [x - w, y, z1],
                    ],
                    n,
                    col,
                );
                x += WEAVE_PITCH;
            }
            let mut z = z0 + WEAVE_PITCH;
            while z < z1 {
                face(
                    out,
                    [
                        [lo.x + BAR, y, z - w],
                        [hi.x - BAR, y, z - w],
                        [hi.x - BAR, y, z + w],
                        [lo.x + BAR, y, z + w],
                    ],
                    n,
                    col,
                );
                z += WEAVE_PITCH;
            }
        }
        Panel::Left | Panel::Right => {
            let (x, n) = match panel {
                Panel::Left => (lo.x + inset, [1.0, 0.0, 0.0]),
                _ => (hi.x - inset, [-1.0, 0.0, 0.0]),
            };
            let mut y = lo.y + BAR + WEAVE_PITCH;
            while y < hi.y - BAR {
                face(
                    out,
                    [
                        [x, y - w, z0],
                        [x, y + w, z0],
                        [x, y + w, z1],
                        [x, y - w, z1],
                    ],
                    n,
                    col,
                );
                y += WEAVE_PITCH;
            }
            let mut z = z0 + WEAVE_PITCH;
            while z < z1 {
                face(
                    out,
                    [
                        [x, lo.y + BAR, z - w],
                        [x, hi.y - BAR, z - w],
                        [x, hi.y - BAR, z + w],
                        [x, lo.y + BAR, z + w],
                    ],
                    n,
                    col,
                );
                z += WEAVE_PITCH;
            }
        }
    }
}

fn prop_mesh(out: &mut Mesh, c: &Ctx, prop: &Prop) {
    if !prop.present() {
        return;
    }
    let region = c.h.region;
    match prop.kind {
        PropKind::Dish => {
            // A shallow dish of medium: an open cylinder with the tan surface
            // of the medium a little below its lip, and a few dark specks of
            // yeast on it.
            contact_shade(out, prop.pos, FLOOR_Z + 0.45, prop.radius);
            let r = prop.radius;
            let bottom = [prop.pos.x, prop.pos.y, FLOOR_Z + 0.8];
            let lip = [prop.pos.x, prop.pos.y, FLOOR_Z + 10.0];
            surface(out, Material::GLASS, |out| {
                tube(
                    out,
                    bottom,
                    lip,
                    r,
                    32,
                    false,
                    rgba([0.93, 0.93, 0.91], 0.55),
                )
            });
            disc(out, bottom, r, None, rgba([0.93, 0.93, 0.91], 0.7));
            disc(
                out,
                [prop.pos.x, prop.pos.y, FLOOR_Z + 7.5],
                r * 0.93,
                None,
                rgba(MEDIUM, 0.92),
            );
            let mut s = Scatter::new((prop.radius * 37.0) as u32);
            for _ in 0..7 {
                let a = s.range(0.0, std::f32::consts::TAU);
                let d = s.range(0.0, r * 0.7);
                dome(
                    out,
                    Vec2::new(prop.pos.x + a.cos() * d, prop.pos.y + a.sin() * d),
                    FLOOR_Z + 7.6,
                    1.2,
                    0.5,
                    rgba([0.45, 0.32, 0.16], 0.8),
                );
            }
        }
        PropKind::Fruit => {
            contact_shade(out, prop.pos, FLOOR_Z + 0.45, prop.radius);
            if prop.variant == 0 {
                surface(out, Material::SKIN, |out| banana(out, c, prop));
            } else {
                surface(out, Material::WET, |out| apple_wedge(out, prop));
            }
        }
        PropKind::Vial => {
            // A spent culture vial on its side: a glass tube with a plug of
            // cotton at the mouth and a layer of medium at the base.
            let half = prop.radius * 2.2;
            let r = prop.radius * 0.6;
            let z = FLOOR_Z + r + 0.6;
            let x0 = (prop.pos.x - half).max(region.min().x + 4.0);
            let x1 = (prop.pos.x + half).min(region.max().x - 4.0);
            let (y, _) = (prop.pos.y, 0.0);
            ribbon(
                out,
                &[Vec2::new(x0, y), Vec2::new(x1, y)],
                FLOOR_Z + 0.45,
                r * 2.4,
                [0.0, 0.0, 0.0, 0.2],
            );
            // Medium at the base end, then the glass over everything, then the
            // plug which sits proud of the mouth.
            let len = x1 - x0;
            tube(
                out,
                [x0 + 0.5, y, z],
                [x0 + len * 0.3, y, z],
                r * 0.9,
                14,
                true,
                rgba(MEDIUM, 0.9),
            );
            let glass_start = out.verts.len();
            tube(
                out,
                [x0, y, z],
                [x1 - r * 1.2, y, z],
                r,
                16,
                true,
                rgba([0.80, 0.88, 0.90], 0.35),
            );
            for v in &mut out.verts[glass_start..] {
                v.material = Material::GLASS;
            }
            tube(
                out,
                [x1 - r * 1.8, y, z],
                [x1, y, z],
                r * 0.94,
                14,
                true,
                rgba([0.97, 0.97, 0.95], 0.9),
            );
        }
        _ => {}
    }
}

/// A banana: a gentle arc of three tubes, dark at both tips. Its direction is
/// fixed per prop so it does not spin as the phase advances.
fn banana(out: &mut Mesh, c: &Ctx, prop: &Prop) {
    let region = c.h.region;
    let ang = prop.radius * 1.3;
    let len = prop.radius * 2.4;
    let r = prop.radius * 0.33;
    let (dx, dy) = (ang.cos(), ang.sin());
    let (px, py) = (-dy, dx);
    let z = FLOOR_Z + r + 0.6;
    let pt = |t: f32| {
        // The arc bows sideways by a fraction of its length.
        let bow = (1.0 - (2.0 * t - 1.0).powi(2)) * len * 0.18;
        let p = region.clamp_inside(
            Vec2::new(
                prop.pos.x + dx * (t - 0.5) * len + px * bow,
                prop.pos.y + dy * (t - 0.5) * len + py * bow,
            ),
            r + 3.0,
        );
        [p.x, p.y, z]
    };
    let skin = rgba([0.93, 0.80, 0.26], 0.93);
    for k in 0..9 {
        let a = pt(k as f32 / 9.0);
        let b = pt((k + 1) as f32 / 9.0);
        tube(out, a, b, r, 12, true, skin);
    }
    for t in [0.0, 1.0] {
        let p = pt(t);
        tube(
            out,
            p,
            pt(if t == 0.0 { 0.08 } else { 0.92 }),
            r * 0.6,
            10,
            true,
            rgba([0.33, 0.22, 0.10], 0.9),
        );
    }
}

/// A wedge of apple lying on its side: cream flesh, a strip of red skin along
/// the curved edge.
fn apple_wedge(out: &mut Mesh, prop: &Prop) {
    let r = prop.radius * 1.4;
    let h = 6.0;
    let ang0 = prop.radius * 0.9;
    let span = 0.85;
    let apex = Vec2::new(prop.pos.x, prop.pos.y);
    let flesh = rgba([0.96, 0.92, 0.78], 0.93);
    let skin = rgba([0.76, 0.16, 0.16], 0.93);
    let z0 = FLOOR_Z + 0.8;
    let z1 = z0 + h;
    let segs = 6;
    let arc = |i: usize| {
        let a = ang0 + span * (i as f32 / segs as f32 - 0.5);
        Vec2::new(apex.x + a.cos() * r, apex.y + a.sin() * r)
    };
    for i in 0..segs {
        let (a, b) = (arc(i), arc(i + 1));
        // top and bottom faces
        tri(
            out,
            [[apex.x, apex.y, z1], [a.x, a.y, z1], [b.x, b.y, z1]],
            [0.0, 0.0, 1.0],
            flesh,
        );
        tri(
            out,
            [[apex.x, apex.y, z0], [b.x, b.y, z0], [a.x, a.y, z0]],
            [0.0, 0.0, -1.0],
            flesh,
        );
        // the skin along the arc
        let mid = ang0 + span * ((i as f32 + 0.5) / segs as f32 - 0.5);
        face(
            out,
            [
                [a.x, a.y, z0],
                [b.x, b.y, z0],
                [b.x, b.y, z1],
                [a.x, a.y, z1],
            ],
            [mid.cos(), mid.sin(), 0.0],
            skin,
        );
    }
    // the two cut faces
    for (p, n) in [
        (arc(0), ang0 - span * 0.5 - 1.57),
        (arc(segs), ang0 + span * 0.5 + 1.57),
    ] {
        face(
            out,
            [
                [apex.x, apex.y, z0],
                [p.x, p.y, z0],
                [p.x, p.y, z1],
                [apex.x, apex.y, z1],
            ],
            [n.cos(), n.sin(), 0.0],
            flesh,
        );
    }
}
