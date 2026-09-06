//! The enclosure, drawn as an actual box.
//!
//! Under the tilted habitat camera the tank has real geometry: a floor with a
//! substrate bed on it, a back wall and two side walls that rise out of the
//! ground plane, and a sheet of front glass between the viewer and everything
//! else. The creature is inside that box, not on top of a picture of one.
//!
//! ## Draw order is index order
//!
//! The enclosure and the creature share one pipeline and one vertex format, so
//! they reach the GPU as a single buffer and a single draw. That means the
//! order of the indices *is* the order the triangles are drawn, which is what
//! makes translucent glass work: [`build_back`] is written first, the creature
//! next, and [`build_front`] last — so the front pane blends over the animal
//! behind it, and the back wall never blends over anything it should be behind.
//!
//! ## Fresnel is baked, not shaded
//!
//! The camera is orthographic and fixed, so the view direction is the same for
//! every vertex in the frame. That makes the grazing-angle brightening that
//! sells glass a *constant* per surface — computable here, at build time, with
//! no extra shader, no extra pipeline, and no room needed in the vertex format.
//!
//! Everything stays translucent. This is an overlay on someone's desktop, and
//! an opaque box parked over their work would be intolerable however good it
//! looked.

use dfcore::{Habitat, HabitatKind, Prop, PropKind, Vec2};

use crate::mesh::{Mesh, Vertex};

/// The tank floor, in scene z. Below the creatures' own geometry: a body's
/// nominal plane is z = 0, but the fly's legs and wings reach about 4.7 units
/// under it, and a floor drawn above that would have the fly standing
/// knee-deep in gravel. `runtime.rs` measures all four and asserts the
/// clearance rather than trusting this number.
pub const FLOOR_Z: f32 = -9.0;
/// How high the walls stand above the floor. A real tank is roughly a third as
/// tall as it is long, and at 96 against a ~600-unit footprint the box read as
/// a tray rather than as an aquarium.
pub const WALL_H: f32 = 170.0;
/// Where the water surface sits in an aquarium — below the rim, as a tank that
/// is actually filled looks.
const WATER_Z: f32 = FLOOR_Z + WALL_H * 0.86;

/// The top of the box: the cut edge of the glass. Placement asks for this
/// rather than adding `WALL_H` at four call sites.
pub fn top_z() -> f32 {
    FLOOR_Z + WALL_H
}

/// Where in the tank a creature's own origin should sit, given how high it
/// wants to be (0 = on the bottom, 1 = just under the surface).
///
/// Walkers get a small lift so their feet meet the substrate rather than
/// sinking into it — the fly's legs reach ~4.7 below its origin. A swimmer gets
/// the water column, which is what turns the koi's `depth` from a scale trick
/// into an actual position: looking straight down there was no way to show a
/// fish rising, so it was faked with size.
pub fn creature_lift(kind: HabitatKind, hint: f32) -> f32 {
    match kind {
        HabitatKind::Aquarium => FLOOR_Z + 14.0 + hint.clamp(0.0, 1.0) * WALL_H * 0.58,
        HabitatKind::Terrarium => FLOOR_Z + 5.0,
    }
}

fn rgba(c: [f32; 3], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

/// Grazing-angle brightening. A surface facing the camera shows its own colour;
/// one seen edge-on catches the light and goes opaque, which is what makes a
/// pane of glass visible at all.
fn fresnel(normal: [f32; 3], view: [f32; 3]) -> f32 {
    let d = (normal[0] * view[0] + normal[1] * view[1] + normal[2] * view[2]).abs();
    (1.0 - d).clamp(0.0, 1.0).powf(2.2)
}

/// One flat face, given as four corners in order around its perimeter.
fn face(out: &mut Mesh, quad: [[f32; 3]; 4], normal: [f32; 3], color: [f32; 4]) {
    let base = out.verts.len() as u32;
    for p in quad {
        out.verts.push(Vertex {
            pos: p,
            normal,
            color,
        });
    }
    out.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// A face lying flat on the ground plane (normal +z), from two corners.
fn ground_face(out: &mut Mesh, lo: Vec2, hi: Vec2, z: f32, color: [f32; 4]) {
    face(
        out,
        [
            [lo.x, lo.y, z],
            [hi.x, lo.y, z],
            [hi.x, hi.y, z],
            [lo.x, hi.y, z],
        ],
        [0.0, 0.0, 1.0],
        color,
    );
}

/// A dome sitting on the floor: pebbles, grit, the ball. Rim normals tip
/// outward so it shades like a rounded thing from any camera angle.
fn dome(out: &mut Mesh, at: Vec2, z: f32, r: f32, rise: f32, color: [f32; 4]) {
    const SEG: usize = 16;
    let base = out.verts.len() as u32;
    out.verts.push(Vertex {
        pos: [at.x, at.y, z + rise],
        normal: [0.0, 0.0, 1.0],
        color,
    });
    for k in 0..SEG {
        let a = std::f32::consts::TAU * k as f32 / SEG as f32;
        let (s, c) = a.sin_cos();
        // Slope of the dome's flank, so a tall dome shades as a tall dome.
        let up = (r / rise.max(0.1)) * 0.45;
        let l = (1.0 + up * up).sqrt();
        out.verts.push(Vertex {
            pos: [at.x + c * r, at.y + s * r, z],
            normal: [c / l, s / l, up / l],
            color,
        });
    }
    for k in 0..SEG {
        let a = base + 1 + k as u32;
        let b = base + 1 + ((k + 1) % SEG) as u32;
        out.indices.extend_from_slice(&[base, a, b]);
    }
}

/// An upright frond: a tapered strip standing out of the substrate, leaning
/// with the prop's sway. Now that height is visible, plants grow *up* rather
/// than being splayed across the floor.
fn frond(
    out: &mut Mesh,
    root: Vec2,
    lean: f32,
    height: f32,
    half_width: f32,
    color: [f32; 4],
    region: dfcore::Region,
) {
    const JOINTS: usize = 5;
    let base = out.verts.len() as u32;
    for j in 0..=JOINTS {
        let t = j as f32 / JOINTS as f32;
        // Bend accumulates toward the tip, so the frond curves rather than
        // hinging at the base.
        let bend = lean * t * t;
        // Clamped into the tank: a frond that leans out through the glass is a
        // small thing to see and a hard thing to unsee, and the mesh promising
        // to stay inside the region is what makes the walls mean anything.
        let tip = region.clamp_inside(
            Vec2::new(
                root.x + bend * height * 0.55,
                root.y + bend * height * 0.18,
            ),
            half_width + 2.0,
        );
        let (x, y) = (tip.x, tip.y);
        let z = FLOOR_Z + 1.0 + height * t;
        let w = half_width * (1.0 - t * 0.85);
        let shade = 0.72 + 0.28 * t;
        let c = [color[0] * shade, color[1] * shade, color[2] * shade, color[3]];
        for side in [-1.0f32, 1.0] {
            out.verts.push(Vertex {
                pos: [x + side * w, y, z],
                normal: [bend.sin() * 0.4, -0.5, 0.76],
                color: c,
            });
        }
    }
    for j in 0..JOINTS as u32 {
        let a = base + j * 2;
        out.indices
            .extend_from_slice(&[a, a + 1, a + 2, a + 1, a + 3, a + 2]);
    }
}

struct Palette {
    /// Water, or the colour of the ground.
    fill: [f32; 3],
    /// Gravel or soil on the floor.
    bed: [f32; 3],
    /// The glass itself.
    glass: [f32; 3],
    plant: [f32; 3],
}

fn palette(kind: HabitatKind) -> Palette {
    match kind {
        HabitatKind::Aquarium => Palette {
            fill: [0.13, 0.38, 0.49],
            bed: [0.31, 0.32, 0.35],
            glass: [0.66, 0.86, 0.94],
            plant: [0.18, 0.55, 0.31],
        },
        HabitatKind::Terrarium => Palette {
            fill: [0.27, 0.22, 0.16],
            bed: [0.39, 0.31, 0.22],
            glass: [0.82, 0.80, 0.74],
            plant: [0.31, 0.47, 0.23],
        },
    }
}

/// Everything behind the creature: floor, substrate, back and side walls, and
/// the props standing on the floor. Write this into the buffer first.
pub fn build_back(out: &mut Mesh, h: &Habitat, view: [f32; 3]) {
    out.verts.clear();
    out.indices.clear();

    let r = h.region;
    let p = palette(h.kind);
    let (lo, hi) = (r.min(), r.max());
    let top = FLOOR_Z + WALL_H;

    // --- floor
    ground_face(out, lo, hi, FLOOR_Z, rgba(p.fill, 0.40));

    // --- substrate: a bed over the whole floor, and grit on top of it. Under a
    // tilted camera the floor is the largest surface in view, so a flat wash of
    // colour there is what would make the tank look like a decal.
    ground_face(out, lo, hi, FLOOR_Z + 0.4, rgba(p.bed, 0.36));
    let mut n = (r.size.0 as i32 as u32)
        .wrapping_mul(2654435761)
        .wrapping_add(1);
    let mut next = move || {
        n ^= n << 13;
        n ^= n >> 17;
        n ^= n << 5;
        (n >> 8) as f32 / 16777216.0
    };
    for _ in 0..70 {
        let gr = 1.8 + next() * 3.4;
        // Inset by its own radius: a grain centred exactly on the edge would
        // otherwise poke out through the wall.
        let at = Vec2::new(
            lo.x + gr + next() * (r.size.0 - 2.0 * gr),
            lo.y + gr + next() * (r.size.1 - 2.0 * gr),
        );
        let shade = 0.74 + next() * 0.52;
        dome(
            out,
            at,
            FLOOR_Z + 0.8,
            gr,
            gr * 0.4,
            rgba([p.bed[0] * shade, p.bed[1] * shade, p.bed[2] * shade], 0.62),
        );
    }

    // --- back wall, seen from the inside (normal pointing toward the viewer),
    // and the two side walls.
    let wall_a = 0.22;
    for (quad, normal) in [
        (
            [
                [lo.x, hi.y, FLOOR_Z],
                [hi.x, hi.y, FLOOR_Z],
                [hi.x, hi.y, top],
                [lo.x, hi.y, top],
            ],
            [0.0, -1.0, 0.0],
        ),
        (
            [
                [lo.x, lo.y, FLOOR_Z],
                [lo.x, hi.y, FLOOR_Z],
                [lo.x, hi.y, top],
                [lo.x, lo.y, top],
            ],
            [1.0, 0.0, 0.0],
        ),
        (
            [
                [hi.x, lo.y, FLOOR_Z],
                [hi.x, hi.y, FLOOR_Z],
                [hi.x, hi.y, top],
                [hi.x, lo.y, top],
            ],
            [-1.0, 0.0, 0.0],
        ),
    ] {
        let f = fresnel(normal, view);
        face(out, quad, normal, rgba(p.glass, wall_a + 0.5 * f));
    }

    for prop in &h.props {
        build_prop(out, prop, &p, r);
    }
}

/// Everything in front of the creature: the water surface, the front pane, and
/// the bright top rim. Write this into the buffer *after* the creature so it
/// blends over it.
pub fn build_front(out: &mut Mesh, h: &Habitat, view: [f32; 3], grabbed: bool) {
    out.verts.clear();
    out.indices.clear();

    let r = h.region;
    let p = palette(h.kind);
    let (lo, hi) = (r.min(), r.max());
    let top = FLOOR_Z + WALL_H;

    // --- the water's surface, an aquarium's most legible single feature: it
    // says "this box is full of something" at a glance.
    if h.kind == HabitatKind::Aquarium {
        // The **waterline only**, not the surface. A full translucent plane
        // across the top is between the camera and everything in the tank, so
        // it washes the gravel, the plants and the fish out into one milky
        // sheet — it made the box look lidded. A bright band where the surface
        // meets the glass says how full the tank is and hides nothing.
        let band = 3.5;
        for (a, b) in [
            (Vec2::new(lo.x, hi.y - band), Vec2::new(hi.x, hi.y)),
            (Vec2::new(lo.x, lo.y), Vec2::new(hi.x, lo.y + band)),
            (Vec2::new(lo.x, lo.y), Vec2::new(lo.x + band, hi.y)),
            (Vec2::new(hi.x - band, lo.y), Vec2::new(hi.x, hi.y)),
        ] {
            ground_face(out, a, b, WATER_Z, rgba([0.74, 0.93, 0.99], 0.50));
        }
    }

    // --- front pane. Barely there face-on, catching the light at its edges.
    let normal = [0.0, 1.0, 0.0];
    let f = fresnel(normal, view);
    face(
        out,
        [
            [lo.x, lo.y, FLOOR_Z],
            [hi.x, lo.y, FLOOR_Z],
            [hi.x, lo.y, top],
            [lo.x, lo.y, top],
        ],
        normal,
        rgba(p.glass, 0.07 + 0.30 * f),
    );

    // --- the rim: the cut top edge of all four panes, which is where a real
    // tank is brightest and what makes the silhouette read.
    // While the user is repositioning it, the rim lights up — the only
    // feedback available, since the tank is being dragged by a held chord
    // rather than by a click it could highlight on press.
    let t = if grabbed { 6.0 } else { 4.0 };
    let rim = rgba(p.glass, if grabbed { 0.92 } else { 0.62 });
    for (a, b) in [
        (Vec2::new(lo.x, hi.y - t), Vec2::new(hi.x, hi.y)),
        (Vec2::new(lo.x, lo.y), Vec2::new(hi.x, lo.y + t)),
        (Vec2::new(lo.x, lo.y), Vec2::new(lo.x + t, hi.y)),
        (Vec2::new(hi.x - t, lo.y), Vec2::new(hi.x, hi.y)),
    ] {
        ground_face(out, a, b, top, rim);
    }
}

/// A soft dark patch on the substrate directly under a prop. Without it, props
/// read as hovering a little above the floor: the renderer's one shadow pass is
/// spent on the creature, and nothing else in the scene touches the ground
/// visibly. Cheap, and it is most of what makes the tank look inhabited.
fn contact_shade(out: &mut Mesh, at: Vec2, r: f32) {
    dome(out, at, FLOOR_Z + 0.45, r * 1.35, 0.0, [0.0, 0.0, 0.0, 0.22]);
}

fn build_prop(out: &mut Mesh, prop: &Prop, p: &Palette, region: dfcore::Region) {
    if !prop.present() {
        return;
    }
    if prop.kind != PropKind::Food {
        contact_shade(out, prop.pos, prop.radius);
    }
    match prop.kind {
        PropKind::Pebble => {
            let g = 0.44 + prop.radius * 0.006;
            dome(
                out,
                prop.pos,
                FLOOR_Z + 1.0,
                prop.radius,
                prop.radius * 0.62,
                rgba([g, g * 0.97, g * 0.93], 0.80),
            );
        }
        PropKind::Ball => {
            // Sits on the floor and brightens while it is being knocked about,
            // so it is obvious the creature is the one moving it.
            let glow = 0.12 * prop.stir;
            dome(
                out,
                prop.pos,
                FLOOR_Z + 1.2,
                prop.radius,
                prop.radius * 1.15,
                rgba([0.86 + glow, 0.40 + glow, 0.28 + glow], 0.90),
            );
        }
        PropKind::Food => {
            // A flake in the water column. `respawn` handles its life and
            // `pos.y` its drift; the height here is what makes it read as
            // suspended rather than sliding along the gravel.
            dome(
                out,
                prop.pos,
                FLOOR_Z + 2.0 + WALL_H * 0.45,
                prop.radius,
                prop.radius,
                rgba([0.92, 0.78, 0.40], 0.92),
            );
        }
        PropKind::Plant => {
            dome(
                out,
                prop.pos,
                FLOOR_Z + 0.9,
                prop.radius * 0.55,
                prop.radius * 0.4,
                rgba([p.bed[0] * 0.75, p.bed[1] * 0.75, p.bed[2] * 0.75], 0.8),
            );
            let blades = 6;
            let amp = 0.14 + 0.36 * prop.stir;
            for i in 0..blades {
                let f = i as f32 / (blades - 1) as f32;
                let lean = (f - 0.5) * 0.7 + (prop.phase + f * 2.4).sin() * amp;
                let height = prop.radius * (2.6 + ((i % 3) as f32) * 0.8);
                frond(
                    out,
                    region.clamp_inside(
                        Vec2::new(
                            prop.pos.x + (f - 0.5) * prop.radius * 0.8,
                            prop.pos.y + ((i % 2) as f32 - 0.5) * prop.radius * 0.5,
                        ),
                        prop.radius * 0.24 + 2.0,
                    ),
                    lean,
                    height,
                    prop.radius * 0.22,
                    rgba(p.plant, 0.88),
                    region,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;
    use dfcore::Region;

    fn cam() -> Camera {
        crate::camera::habitat()
    }

    fn view() -> [f32; 3] {
        cam().view_dir()
    }

    fn tank(kind: HabitatKind) -> Habitat {
        Habitat::new(
            kind,
            Region::new(Vec2::new(320.0, -180.0), (620.0, 420.0)),
            11,
        )
    }

    fn whole(h: &Habitat) -> Mesh {
        let mut m = Mesh::default();
        let mut f = Mesh::default();
        build_back(&mut m, h, view());
        build_front(&mut f, h, view(), false);
        let base = m.verts.len() as u32;
        m.verts.extend_from_slice(&f.verts);
        m.indices.extend(f.indices.iter().map(|i| i + base));
        m
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
    fn both_kinds_produce_a_closed_mesh() {
        for kind in [HabitatKind::Aquarium, HabitatKind::Terrarium] {
            let m = whole(&tank(kind));
            assert!(m.verts.len() > 400, "{kind:?}: only {} verts", m.verts.len());
            assert_eq!(m.indices.len() % 3, 0);
            assert!(
                m.indices.iter().all(|&i| (i as usize) < m.verts.len()),
                "{kind:?} produced out-of-range indices"
            );
        }
    }

    /// The tank must not spill over its own walls in the ground plane — a box
    /// drawn wider than the region the creature is confined to would be a lie
    /// about where the boundary is.
    #[test]
    fn the_drawing_stays_within_the_region_footprint() {
        for kind in [HabitatKind::Aquarium, HabitatKind::Terrarium] {
            let h = tank(kind);
            let m = whole(&h);
            let (lo, hi) = bounds(&m);
            let (rlo, rhi) = (h.region.min(), h.region.max());
            assert!(
                lo[0] >= rlo.x - 0.01 && lo[1] >= rlo.y - 0.01,
                "{kind:?} spills past the near corner"
            );
            assert!(
                hi[0] <= rhi.x + 0.01 && hi[1] <= rhi.y + 0.01,
                "{kind:?} spills past the far corner: mesh ({}, {}) vs region ({}, {})",
                hi[0], hi[1], rhi.x, rhi.y
            );
        }
    }

    /// The whole point of the tilt: the tank has height now.
    #[test]
    fn the_tank_is_a_box_of_the_declared_height() {
        let m = whole(&tank(HabitatKind::Aquarium));
        let (lo, hi) = bounds(&m);
        assert!(
            (lo[2] - FLOOR_Z).abs() < 0.01,
            "the floor is at {} not {FLOOR_Z}",
            lo[2]
        );
        assert!(
            (hi[2] - (FLOOR_Z + WALL_H)).abs() < 0.01,
            "the rim is at {} not {}",
            hi[2],
            FLOOR_Z + WALL_H
        );
        // And the walls occupy screen height the ground rectangle knows
        // nothing about — which is exactly what placement has to account for.
        let c = cam();
        let (flat_lo, flat_hi) = c.screen_offsets((310.0, 210.0), FLOOR_Z, FLOOR_Z);
        let (box_lo, box_hi) = c.screen_offsets((310.0, 210.0), FLOOR_Z, top_z());
        assert!(
            (box_hi.y - box_lo.y) > (flat_hi.y - flat_lo.y) + 20.0,
            "walls add no screen height"
        );
    }

    /// Front glass has to be written *after* the creature, so it is a separate
    /// mesh — and it must be nearer the camera than the space the creature
    /// occupies, or it blends the wrong way round and the animal looks painted
    /// onto the pane.
    ///
    /// Comparing whole-mesh extremes does not test this: the *side* walls
    /// legitimately reach the tank's near edge too, so the back mesh's nearest
    /// vertex ties with the front pane's. What matters is the front pane
    /// against the middle of the tank, which is where the animal is.
    #[test]
    fn the_front_pane_is_nearer_than_the_space_the_creature_swims_in() {
        let h = tank(HabitatKind::Aquarium);
        let mut back = Mesh::default();
        let mut front = Mesh::default();
        build_back(&mut back, &h, view());
        build_front(&mut front, &h, view(), false);
        assert!(!front.indices.is_empty(), "there is no front glass");

        let v = cam().view();
        let depth = |p: [f32; 3]| crate::math::transform_point(&v, p)[2];
        // The pane sits on the near wall, at y = region.min(). Scanning the
        // whole front mesh for a minimum would find the *rim* instead, which
        // spans the entire footprint including the back.
        let mid_z = FLOOR_Z + WALL_H / 2.0;
        let pane = depth([h.region.center.x, h.region.min().y, mid_z]);
        assert!(
            front
                .verts
                .iter()
                .any(|p| (p.pos[1] - h.region.min().y).abs() < 0.01),
            "the front mesh has nothing on the near wall"
        );
        // Versus the creature in the middle of the tank at the top of its water
        // column — the nearest to the glass it ever gets.
        let creature = depth([
            h.region.center.x,
            h.region.center.y,
            creature_lift(h.kind, 1.0),
        ]);
        assert!(
            pane > creature,
            "the creature is in front of the glass: pane {pane:.1}, creature {creature:.1}"
        );

        // And the back wall must be behind that same point.
        let wall = depth([h.region.center.x, h.region.max().y, FLOOR_Z + WALL_H]);
        assert!(
            wall < creature,
            "the back wall is in front of the creature: wall {wall:.1}, creature {creature:.1}"
        );
    }

    /// Glass seen edge-on has to brighten, or the panes are invisible and the
    /// tank has no silhouette.
    ///
    /// Stated as the property, not as a number off one particular wall: with
    /// yaw the side pane is *not* edge-on any more — turning it toward the
    /// viewer is the whole reason yaw is there — so a threshold tuned to it
    /// measures the camera angle rather than the shading.
    #[test]
    fn glass_catches_the_light_at_grazing_angles() {
        let v = view();
        // Straight at the camera: no boost at all.
        assert!(fresnel(v, v) < 0.01, "a face-on surface is being brightened");
        // Exactly edge-on: full boost.
        let edge = [v[1], -v[0], 0.0];
        assert!(
            fresnel(edge, v) > 0.95,
            "an edge-on surface is not brightening: {}",
            fresnel(edge, v)
        );
        // And it has to be monotonic in between, or the falloff is not a
        // falloff.
        let mut last = 0.0;
        for k in 0..=8 {
            let t = k as f32 / 8.0;
            let n = [
                v[0] * (1.0 - t) + edge[0] * t,
                v[1] * (1.0 - t) + edge[1] * t,
                v[2] * (1.0 - t) + edge[2] * t,
            ];
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            let f = fresnel([n[0] / l, n[1] / l, n[2] / l], v);
            assert!(f >= last - 1e-4, "fresnel dipped at t={t}: {f} after {last}");
            last = f;
        }

        // The panes of the actual tank must span a real range, so the box has
        // edges rather than reading as one flat wash.
        let walls: Vec<f32> = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
            .iter()
            .map(|n| fresnel(*n, v))
            .collect();
        let spread = walls.iter().cloned().fold(f32::MIN, f32::max)
            - walls.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread > 0.1, "every pane shades the same: {walls:?}");
    }

    #[test]
    fn nothing_is_fully_opaque() {
        for kind in [HabitatKind::Aquarium, HabitatKind::Terrarium] {
            for v in &whole(&tank(kind)).verts {
                assert!(v.color[3] <= 0.95, "{kind:?} has an opaque surface");
            }
        }
    }

    /// Plants have to stand up now that height is visible. Splaying them flat
    /// on the floor was the right compromise looking straight down and is
    /// simply wrong here.
    #[test]
    fn plants_stand_up_out_of_the_substrate() {
        let h = tank(HabitatKind::Aquarium);
        let i = h
            .props
            .iter()
            .position(|p| p.kind == PropKind::Plant)
            .unwrap();
        let mut only = h.clone();
        only.props = vec![h.props[i]];
        let mut m = Mesh::default();
        // Walls would dominate the bounds, so measure the props alone.
        build_prop(&mut m, &only.props[0], &palette(only.kind), only.region);
        let (_, hi) = bounds(&m);
        let height = hi[2] - FLOOR_Z;
        assert!(
            height > h.props[i].radius * 2.0,
            "the plant is only {height:.1} tall"
        );
        assert!(height < WALL_H, "the plant grows out of the tank");
    }

    /// A stirred plant must actually move, or the cursor's only way of touching
    /// the tank produces no visible result.
    #[test]
    fn a_stirred_plant_moves() {
        let mut h = tank(HabitatKind::Terrarium);
        let i = h
            .props
            .iter()
            .position(|p| p.kind == PropKind::Plant)
            .unwrap();
        let calm = whole(&h);
        h.props[i].stir = 1.0;
        let stirred = whole(&h);

        assert_eq!(calm.verts.len(), stirred.verts.len(), "topology changed");
        let moved = calm
            .verts
            .iter()
            .zip(&stirred.verts)
            .map(|(a, b)| ((a.pos[0] - b.pos[0]).powi(2) + (a.pos[2] - b.pos[2]).powi(2)).sqrt())
            .fold(0.0f32, f32::max);
        assert!(moved > 1.0, "the plant did not react: {moved}");
    }

    /// An eaten flake stops being drawn — otherwise the fish appears to swim
    /// through food it has already taken.
    #[test]
    fn eaten_food_is_not_drawn() {
        let mut h = tank(HabitatKind::Aquarium);
        let i = h
            .props
            .iter()
            .position(|p| p.kind == PropKind::Food)
            .unwrap();
        let before = whole(&h).verts.len();
        h.props[i].respawn = 4.0;
        assert!(
            whole(&h).verts.len() < before,
            "the flake is still on screen after being eaten"
        );
    }

    /// Aquarium and terrarium have to look different, or "matched to the
    /// creature type" is a claim with no rendering behind it.
    #[test]
    fn the_two_kinds_look_different() {
        let a = whole(&tank(HabitatKind::Aquarium));
        let t = whole(&tank(HabitatKind::Terrarium));
        let aq = a.verts[0].color;
        let te = t.verts[0].color;
        assert!(aq[2] > aq[0], "the aquarium floor is not blue: {aq:?}");
        assert!(te[0] > te[2], "the terrarium floor is not earthy: {te:?}");
    }
}
