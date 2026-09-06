//! The enclosure, drawn.
//!
//! Everything here is seen from directly above, because that is the only camera
//! this app has — the overlay looks down at the desktop the way you look down
//! into a tank on a table. So an "aquarium" is not a box in perspective: it is a
//! water plane, a gravel bed along the near edge, and a bright rim where the
//! glass catches the light. That reads as a container at a glance and costs a
//! few hundred triangles.
//!
//! The whole mesh sits **below** the creature in z (see [`TOP_Z`]), so the
//! animal is always inside its enclosure rather than being occluded by it, and
//! it is drawn translucent throughout: this is an overlay on someone's desktop,
//! and an opaque rectangle parked over their work would be intolerable however
//! pretty it was.

use dfcore::{Habitat, HabitatKind, Prop, PropKind, Region};

use crate::mesh::{Mesh, Vertex};

/// The floor of the tank. Everything else stacks up from here.
const FLOOR_Z: f32 = -26.0;
/// The highest point of any habitat geometry, and the reason the whole tank
/// sits so far back: a creature's *nominal* plane is z = 0, but its geometry is
/// not. The fly's legs and wings reach about 4.7 units below that, so an
/// enclosure drawn just under zero would have the fly standing knee-deep in its
/// own gravel. `runtime.rs` measures every creature's true floor and asserts
/// this clears the lowest of them, rather than trusting the nominal plane.
pub const TOP_Z: f32 = -8.0;

fn rgba(c: [f32; 3], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

/// A flat rectangle facing the camera.
fn quad(out: &mut Mesh, x0: f32, y0: f32, x1: f32, y1: f32, z: f32, color: [f32; 4]) {
    let base = out.verts.len() as u32;
    for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] {
        out.verts.push(Vertex {
            pos: [x, y, z],
            normal: [0.0, 0.0, 1.0],
            color,
        });
    }
    out.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// A shallow dome: a disc whose rim normals tip outward, so the flat top-down
/// view still shades it like a rounded thing. Pebbles and the ball are this.
fn dome(out: &mut Mesh, cx: f32, cy: f32, z: f32, r: f32, rise: f32, color: [f32; 4]) {
    const SEG: usize = 14;
    let base = out.verts.len() as u32;
    out.verts.push(Vertex {
        pos: [cx, cy, z + rise],
        normal: [0.0, 0.0, 1.0],
        color,
    });
    for k in 0..SEG {
        let a = std::f32::consts::TAU * k as f32 / SEG as f32;
        let (s, c) = a.sin_cos();
        let n = {
            let v = [c, s, 0.62];
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            [v[0] / l, v[1] / l, v[2] / l]
        };
        out.verts.push(Vertex {
            pos: [cx + c * r, cy + s * r, z],
            normal: n,
            color,
        });
    }
    for k in 0..SEG {
        let a = base + 1 + k as u32;
        let b = base + 1 + ((k + 1) % SEG) as u32;
        out.indices.extend_from_slice(&[base, a, b]);
    }
}

/// A tapered blade, from a wide root to a point. Plant fronds.
fn blade(
    out: &mut Mesh,
    root: (f32, f32),
    tip: (f32, f32),
    half_width: f32,
    z: f32,
    color: [f32; 4],
) {
    let (dx, dy) = (tip.0 - root.0, tip.1 - root.1);
    let l = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (nx, ny) = (-dy / l * half_width, dx / l * half_width);
    let base = out.verts.len() as u32;
    let mut push = |x: f32, y: f32, n: [f32; 3]| {
        out.verts.push(Vertex {
            pos: [x, y, z],
            normal: n,
            color,
        });
    };
    push(root.0 - nx, root.1 - ny, [0.0, 0.0, 1.0]);
    push(root.0 + nx, root.1 + ny, [0.0, 0.0, 1.0]);
    push(tip.0, tip.1, [0.0, 0.0, 1.0]);
    out.indices.extend_from_slice(&[base, base + 1, base + 2]);
}

struct Palette {
    /// The body of water, or the floor of the terrarium.
    fill: [f32; 3],
    fill_alpha: f32,
    /// Gravel or soil along the near edge.
    bed: [f32; 3],
    /// The glass rim.
    rim: [f32; 3],
    plant: [f32; 3],
}

fn palette(kind: HabitatKind) -> Palette {
    match kind {
        HabitatKind::Aquarium => Palette {
            fill: [0.16, 0.42, 0.52],
            fill_alpha: 0.30,
            bed: [0.30, 0.31, 0.34],
            rim: [0.62, 0.82, 0.90],
            plant: [0.20, 0.56, 0.32],
        },
        HabitatKind::Terrarium => Palette {
            fill: [0.30, 0.24, 0.17],
            fill_alpha: 0.34,
            bed: [0.38, 0.30, 0.21],
            rim: [0.78, 0.76, 0.70],
            plant: [0.32, 0.48, 0.24],
        },
    }
}

/// Build the whole enclosure for this frame.
pub fn build(out: &mut Mesh, h: &Habitat) {
    out.verts.clear();
    out.indices.clear();

    let r = h.region;
    let p = palette(h.kind);
    let (lo, hi) = (r.min(), r.max());

    // The water, or the ground.
    quad(out, lo.x, lo.y, hi.x, hi.y, FLOOR_Z, rgba(p.fill, p.fill_alpha));

    // A bed along the near (down-screen) edge, where gravel would settle.
    let bed_h = (r.size.1 * 0.16).clamp(18.0, 70.0);
    quad(
        out,
        lo.x,
        lo.y,
        hi.x,
        lo.y + bed_h,
        FLOOR_Z + 0.5,
        rgba(p.bed, 0.42),
    );
    // Grit, so the bed is not a flat block of colour. Deterministic from the
    // region so it does not crawl between frames.
    let mut n = (r.size.0 as i32 as u32).wrapping_mul(2654435761).wrapping_add(1);
    let mut next = move || {
        n ^= n << 13;
        n ^= n >> 17;
        n ^= n << 5;
        (n >> 8) as f32 / 16777216.0
    };
    for _ in 0..26 {
        let gx = lo.x + next() * r.size.0;
        let gy = lo.y + 3.0 + next() * (bed_h - 6.0);
        let gr = 1.6 + next() * 3.0;
        let shade = 0.75 + next() * 0.5;
        dome(
            out,
            gx,
            gy,
            FLOOR_Z + 0.8,
            gr,
            0.6,
            rgba([p.bed[0] * shade, p.bed[1] * shade, p.bed[2] * shade], 0.55),
        );
    }

    // The rim: four bands of glass around the edge, brighter than the fill.
    let t = 7.0;
    let rim = rgba(p.rim, 0.34);
    quad(out, lo.x, hi.y - t, hi.x, hi.y, TOP_Z, rim);
    quad(out, lo.x, lo.y, hi.x, lo.y + t, TOP_Z, rim);
    quad(out, lo.x, lo.y + t, lo.x + t, hi.y - t, TOP_Z, rim);
    quad(out, hi.x - t, lo.y + t, hi.x, hi.y - t, TOP_Z, rim);
    // Corner posts, a touch stronger — the frame of the tank.
    let post = rgba(p.rim, 0.55);
    let q = 14.0;
    for (cx, cy) in [
        (lo.x, lo.y),
        (hi.x - q, lo.y),
        (lo.x, hi.y - q),
        (hi.x - q, hi.y - q),
    ] {
        quad(out, cx, cy, cx + q, cy + q, TOP_Z, post);
    }

    for prop in &h.props {
        build_prop(out, prop, &p, r);
    }
}

fn build_prop(out: &mut Mesh, prop: &Prop, p: &Palette, region: Region) {
    if !prop.present() {
        return;
    }
    let z = FLOOR_Z + 3.0;
    match prop.kind {
        PropKind::Pebble => {
            let g = 0.42 + prop.radius * 0.006;
            dome(
                out,
                prop.pos.x,
                prop.pos.y,
                z,
                prop.radius,
                prop.radius * 0.35,
                rgba([g, g * 0.98, g * 0.95], 0.72),
            );
        }
        PropKind::Ball => {
            // Warm, and it brightens while it is being knocked about, so it is
            // obvious that the creature is the one moving it.
            let glow = 0.12 * prop.stir;
            dome(
                out,
                prop.pos.x,
                prop.pos.y,
                z + 0.6,
                prop.radius,
                prop.radius * 0.8,
                rgba([0.86 + glow, 0.42 + glow, 0.30 + glow], 0.85),
            );
        }
        PropKind::Food => {
            dome(
                out,
                prop.pos.x,
                prop.pos.y,
                z + 0.6,
                prop.radius,
                prop.radius * 0.5,
                rgba([0.90, 0.76, 0.38], 0.88),
            );
        }
        PropKind::Plant => {
            // Anchored at its base, fronds fanning up-screen. Sway comes from
            // the prop's own phase, and doubles when something disturbs it —
            // which is the whole of the cursor's interaction with the tank.
            dome(
                out,
                prop.pos.x,
                prop.pos.y,
                z,
                prop.radius * 0.45,
                1.2,
                rgba([p.bed[0] * 0.8, p.bed[1] * 0.8, p.bed[2] * 0.8], 0.7),
            );
            let blades = 5;
            let amp = 0.16 + 0.34 * prop.stir;
            for i in 0..blades {
                let f = i as f32 / (blades - 1) as f32;
                let lean = (f - 0.5) * 1.1 + (prop.phase + f * 2.2).sin() * amp;
                let len = prop.radius * (2.1 + (i % 2) as f32 * 0.9);
                let tip = (
                    prop.pos.x + lean.sin() * len,
                    prop.pos.y + lean.cos() * len,
                );
                let tip = region.clamp_inside(dfcore::Vec2::new(tip.0, tip.1), 8.0);
                let shade = 0.82 + 0.18 * f;
                blade(
                    out,
                    (prop.pos.x, prop.pos.y),
                    (tip.x, tip.y),
                    prop.radius * 0.24,
                    z + 0.4 + i as f32 * 0.05,
                    rgba(
                        [p.plant[0] * shade, p.plant[1] * shade, p.plant[2] * shade],
                        0.80,
                    ),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dfcore::{Habitat, HabitatKind, Region, Vec2};

    fn tank(kind: HabitatKind) -> Habitat {
        Habitat::new(
            kind,
            Region::new(Vec2::new(320.0, -180.0), (620.0, 420.0)),
            11,
        )
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
            let mut m = Mesh::default();
            build(&mut m, &tank(kind));
            assert!(m.verts.len() > 100, "{kind:?}: only {} verts", m.verts.len());
            assert_eq!(m.indices.len() % 3, 0);
            assert!(
                m.indices.iter().all(|&i| (i as usize) < m.verts.len()),
                "{kind:?} produced out-of-range indices"
            );
        }
    }

    /// The enclosure must not spill over its own walls — a tank drawn wider
    /// than the region the creature is confined to would be a lie about where
    /// the boundary is.
    #[test]
    fn the_drawing_stays_inside_the_region() {
        for kind in [HabitatKind::Aquarium, HabitatKind::Terrarium] {
            let h = tank(kind);
            let mut m = Mesh::default();
            build(&mut m, &h);
            let (lo, hi) = bounds(&m);
            let (rlo, rhi) = (h.region.min(), h.region.max());
            assert!(
                lo[0] >= rlo.x - 0.01 && lo[1] >= rlo.y - 0.01,
                "{kind:?} spills past the near corner"
            );
            assert!(
                hi[0] <= rhi.x + 0.01 && hi[1] <= rhi.y + 0.01,
                "{kind:?} spills past the far corner"
            );
        }
    }

    /// The creature lives at z >= 0. If any of the furniture reached that high
    /// the animal would be drawn *inside* its own gravel.
    #[test]
    fn the_enclosure_stays_behind_the_creature() {
        for kind in [HabitatKind::Aquarium, HabitatKind::Terrarium] {
            let mut m = Mesh::default();
            build(&mut m, &tank(kind));
            let (_, hi) = bounds(&m);
            assert!(hi[2] <= TOP_Z + 1e-4, "{kind:?} reaches z = {}", hi[2]);
        }
    }

    /// Aquarium and terrarium have to actually look different, or "matched to
    /// the creature type" is a claim with no rendering behind it.
    #[test]
    fn the_two_kinds_look_different() {
        let mut a = Mesh::default();
        let mut t = Mesh::default();
        build(&mut a, &tank(HabitatKind::Aquarium));
        build(&mut t, &tank(HabitatKind::Terrarium));
        // The water is blue: more blue than red. The soil is not.
        let aq = a.verts[0].color;
        let te = t.verts[0].color;
        assert!(aq[2] > aq[0], "the aquarium fill is not blue: {aq:?}");
        assert!(te[0] > te[2], "the terrarium fill is not earthy: {te:?}");
    }

    /// Everything is drawn translucent. An opaque slab over the user's desktop
    /// would be unusable however good it looked.
    #[test]
    fn nothing_is_fully_opaque() {
        for kind in [HabitatKind::Aquarium, HabitatKind::Terrarium] {
            let mut m = Mesh::default();
            build(&mut m, &tank(kind));
            for v in &m.verts {
                assert!(v.color[3] <= 0.9, "{kind:?} has an opaque surface");
            }
        }
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
        let mut calm = Mesh::default();
        build(&mut calm, &h);

        h.props[i].stir = 1.0;
        let mut stirred = Mesh::default();
        build(&mut stirred, &h);

        assert_eq!(calm.verts.len(), stirred.verts.len(), "topology changed");
        let moved = calm
            .verts
            .iter()
            .zip(&stirred.verts)
            .map(|(a, b)| {
                ((a.pos[0] - b.pos[0]).powi(2) + (a.pos[1] - b.pos[1]).powi(2)).sqrt()
            })
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
        let mut before = Mesh::default();
        build(&mut before, &h);
        h.props[i].respawn = 4.0;
        let mut after = Mesh::default();
        build(&mut after, &h);
        assert!(
            after.verts.len() < before.verts.len(),
            "the flake is still on screen after being eaten"
        );
    }
}
