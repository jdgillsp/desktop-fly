//! The shapes the enclosures are built from. Everything here is a few dozen
//! triangles at most, writes straight into a [`Mesh`], and knows nothing about
//! which enclosure it is part of.

use dfcore::{Region, Vec2};

pub use crate::mesh::Material;
use crate::mesh::{Mesh, Vertex};

/// Assign a surface response explicitly to a generated piece of furniture.
pub fn surface(out: &mut Mesh, material: [f32; 4], build: impl FnOnce(&mut Mesh)) {
    let start = out.verts.len();
    build(out);
    for vertex in &mut out.verts[start..] {
        vertex.material = material;
    }
}

pub fn rgba(c: [f32; 3], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

pub fn shade(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}

/// Grazing-angle brightening. A surface facing the camera shows its own colour;
/// one seen edge-on catches the light and goes opaque, which is what makes a
/// pane of glass visible at all.
///
/// The camera is orthographic and fixed, so the view direction is the same for
/// every vertex in the frame. That makes this a *constant* per surface —
/// computable at build time, with no extra shader and no room needed in the
/// vertex format.
pub fn fresnel(normal: [f32; 3], view: [f32; 3]) -> f32 {
    let d = (normal[0] * view[0] + normal[1] * view[1] + normal[2] * view[2]).abs();
    (1.0 - d).clamp(0.0, 1.0).powf(2.2)
}

/// A tiny deterministic generator for scatter — grit, stone lengths, blob
/// edges. Seeded from the tank's size so the same tank always scatters the
/// same way, and so a resize re-rolls it.
pub struct Scatter(u32);

impl Scatter {
    pub fn new(seed: u32) -> Self {
        Scatter(seed.wrapping_mul(2654435761).wrapping_add(1) | 1)
    }
    pub fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / 16777216.0
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

/// One flat face, given as four corners in order around its perimeter.
pub fn face(out: &mut Mesh, quad: [[f32; 3]; 4], normal: [f32; 3], color: [f32; 4]) {
    let base = out.verts.len() as u32;
    for p in quad {
        out.verts.push(Vertex {
            texcoord: [0.0; 4],
            pos: p,
            normal,
            color,
            material: Material::MATTE,
        });
    }
    out.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// One triangle, with its own normal.
pub fn tri(out: &mut Mesh, pts: [[f32; 3]; 3], normal: [f32; 3], color: [f32; 4]) {
    let base = out.verts.len() as u32;
    for p in pts {
        out.verts.push(Vertex {
            texcoord: [0.0; 4],
            pos: p,
            normal,
            color,
            material: Material::MATTE,
        });
    }
    out.indices.extend_from_slice(&[base, base + 1, base + 2]);
}

/// A face lying flat on the ground plane (normal +z), from two corners.
pub fn ground_face(out: &mut Mesh, lo: Vec2, hi: Vec2, z: f32, color: [f32; 4]) {
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

/// An axis-aligned box between two corners, all six faces, outward normals.
/// Frame bars, coping stones, a strip of tape.
pub fn slab(out: &mut Mesh, lo: [f32; 3], hi: [f32; 3], color: [f32; 4]) {
    let [x0, y0, z0] = lo;
    let [x1, y1, z1] = hi;
    // top / bottom
    face(
        out,
        [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
        [0.0, 0.0, 1.0],
        color,
    );
    face(
        out,
        [[x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0]],
        [0.0, 0.0, -1.0],
        color,
    );
    // near (−y) / far (+y)
    face(
        out,
        [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
        [0.0, -1.0, 0.0],
        color,
    );
    face(
        out,
        [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
        [0.0, 1.0, 0.0],
        color,
    );
    // left (−x) / right (+x)
    face(
        out,
        [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
        [-1.0, 0.0, 0.0],
        color,
    );
    face(
        out,
        [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]],
        [1.0, 0.0, 0.0],
        color,
    );
}

/// A dome sitting on the floor: pebbles, grit, a pellet. Rim normals tip
/// outward so it shades like a rounded thing from any camera angle.
pub fn dome(out: &mut Mesh, at: Vec2, z: f32, r: f32, rise: f32, color: [f32; 4]) {
    blob(out, at, z, r, rise, 0.0, 0.0, color);
}

/// A dome whose outline wobbles — `wobble` is the fraction of the radius the
/// edge wanders by, `seed` picks which way. A bacterial lawn, a river cobble,
/// a coping stone seen from above: nothing natural is a perfect circle.
#[allow(clippy::too_many_arguments)]
pub fn blob(
    out: &mut Mesh,
    at: Vec2,
    z: f32,
    r: f32,
    rise: f32,
    wobble: f32,
    seed: f32,
    color: [f32; 4],
) {
    // Spend silhouette detail on furniture, not subpixel substrate grains.
    let (segs, rings): (usize, usize) = if r < 2.0 {
        (8, 2)
    } else if r < 7.0 {
        (12, 3)
    } else {
        (20, 4)
    };
    let rings = if rise < 0.01 { 1 } else { rings };
    let base = out.verts.len() as u32;
    for ring in 0..=rings {
        let t = ring as f32 / rings as f32 * std::f32::consts::FRAC_PI_2;
        let radial = t.sin();
        let elevation = t.cos();
        for k in 0..segs {
            let a = std::f32::consts::TAU * k as f32 / segs as f32;
            let (sn, cs) = a.sin_cos();
            let noise = (3.0 * a + seed).sin() * 0.6 + (7.0 * a + seed * 1.7).sin() * 0.4;
            let rr = r * (1.0 + wobble * noise);
            let nx = cs * radial * rise.max(0.01);
            let ny = sn * radial * rise.max(0.01);
            let nz = elevation * rr;
            let norm = (nx * nx + ny * ny + nz * nz).sqrt().max(0.001);
            let tone = 1.0 + wobble * 0.20 * noise * radial;
            out.verts.push(Vertex {
                texcoord: [0.0; 4],
                pos: [
                    at.x + cs * rr * radial,
                    at.y + sn * rr * radial,
                    z + rise * elevation,
                ],
                normal: if rise < 0.01 {
                    [0.0, 0.0, 1.0]
                } else {
                    [nx / norm, ny / norm, nz / norm]
                },
                color: [color[0] * tone, color[1] * tone, color[2] * tone, color[3]],
                material: Material::MATTE,
            });
        }
    }
    for ring in 0..rings {
        for k in 0..segs {
            let a = base + (ring * segs + k) as u32;
            let b = base + (ring * segs + (k + 1) % segs) as u32;
            let c = a + segs as u32;
            let d = b + segs as u32;
            // The first ring collapses to the apex: omit its zero-area half.
            out.indices.extend_from_slice(&[a, c, d]);
            if ring > 0 {
                out.indices.extend_from_slice(&[a, d, b]);
            }
        }
    }
}

/// A flat disc facing up, optionally with a wedge missing between two angles
/// (a lily pad's notch).
pub fn disc(out: &mut Mesh, at: [f32; 3], r: f32, gap: Option<(f32, f32)>, color: [f32; 4]) {
    const SEG: usize = 24;
    let (from, to) = match gap {
        Some((a, b)) => (b, a + std::f32::consts::TAU),
        None => (0.0, std::f32::consts::TAU),
    };
    let base = out.verts.len() as u32;
    out.verts.push(Vertex {
        texcoord: [0.0; 4],
        pos: at,
        normal: [0.0, 0.0, 1.0],
        color,
        material: Material::MATTE,
    });
    for k in 0..=SEG {
        let a = from + (to - from) * k as f32 / SEG as f32;
        let (s, c) = a.sin_cos();
        out.verts.push(Vertex {
            texcoord: [0.0; 4],
            pos: [at[0] + c * r, at[1] + s * r, at[2]],
            normal: [0.0, 0.0, 1.0],
            color,
            material: Material::MATTE,
        });
    }
    for k in 0..SEG as u32 {
        out.indices
            .extend_from_slice(&[base, base + 1 + k, base + 2 + k]);
    }
}

/// A capped cylinder between two points in space. A vial, a twig, the wall of
/// a dish. `caps` closes the ends.
pub fn tube(
    out: &mut Mesh,
    a: [f32; 3],
    b: [f32; 3],
    r: f32,
    segs: usize,
    caps: bool,
    color: [f32; 4],
) {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-4);
    let axis = [d[0] / len, d[1] / len, d[2] / len];
    // Any vector not parallel to the axis gives a basis.
    let helper = if axis[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let u = {
        let c = [
            axis[1] * helper[2] - axis[2] * helper[1],
            axis[2] * helper[0] - axis[0] * helper[2],
            axis[0] * helper[1] - axis[1] * helper[0],
        ];
        let l = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt().max(1e-6);
        [c[0] / l, c[1] / l, c[2] / l]
    };
    let v = [
        axis[1] * u[2] - axis[2] * u[1],
        axis[2] * u[0] - axis[0] * u[2],
        axis[0] * u[1] - axis[1] * u[0],
    ];
    let base = out.verts.len() as u32;
    for k in 0..segs {
        let t = std::f32::consts::TAU * k as f32 / segs as f32;
        let (s, c) = t.sin_cos();
        let n = [
            u[0] * c + v[0] * s,
            u[1] * c + v[1] * s,
            u[2] * c + v[2] * s,
        ];
        for p in [a, b] {
            out.verts.push(Vertex {
                texcoord: [0.0; 4],
                pos: [p[0] + n[0] * r, p[1] + n[1] * r, p[2] + n[2] * r],
                normal: n,
                color,
                material: Material::MATTE,
            });
        }
    }
    for k in 0..segs as u32 {
        let i0 = base + 2 * k;
        let i1 = base + 2 * ((k + 1) % segs as u32);
        out.indices
            .extend_from_slice(&[i0, i0 + 1, i1 + 1, i0, i1 + 1, i1]);
    }
    if caps {
        for (p, sign) in [(a, -1.0f32), (b, 1.0)] {
            let n = [axis[0] * sign, axis[1] * sign, axis[2] * sign];
            let c0 = out.verts.len() as u32;
            out.verts.push(Vertex {
                texcoord: [0.0; 4],
                pos: p,
                normal: n,
                color,
                material: Material::MATTE,
            });
            for k in 0..segs {
                let t = std::f32::consts::TAU * k as f32 / segs as f32;
                let (s, c) = t.sin_cos();
                let m = [
                    u[0] * c + v[0] * s,
                    u[1] * c + v[1] * s,
                    u[2] * c + v[2] * s,
                ];
                out.verts.push(Vertex {
                    texcoord: [0.0; 4],
                    pos: [p[0] + m[0] * r, p[1] + m[1] * r, p[2] + m[2] * r],
                    normal: n,
                    color,
                    material: Material::MATTE,
                });
            }
            for k in 0..segs as u32 {
                let i = c0 + 1 + k;
                let j = c0 + 1 + (k + 1) % segs as u32;
                out.indices.extend_from_slice(&[c0, i, j]);
            }
        }
    }
}

/// A ribbon lying on the ground along a polyline: a worm's track in agar, the
/// shadow under something long.
pub fn ribbon(out: &mut Mesh, pts: &[Vec2], z: f32, width: f32, color: [f32; 4]) {
    if pts.len() < 2 {
        return;
    }
    let base = out.verts.len() as u32;
    for i in 0..pts.len() {
        let (a, b) = if i + 1 < pts.len() {
            (pts[i], pts[i + 1])
        } else {
            (pts[i - 1], pts[i])
        };
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l = (dx * dx + dy * dy).sqrt().max(1e-4);
        let (nx, ny) = (-dy / l * width * 0.5, dx / l * width * 0.5);
        for side in [-1.0f32, 1.0] {
            out.verts.push(Vertex {
                texcoord: [0.0; 4],
                pos: [pts[i].x + nx * side, pts[i].y + ny * side, z],
                normal: [0.0, 0.0, 1.0],
                color,
                material: Material::MATTE,
            });
        }
    }
    for i in 0..(pts.len() as u32 - 1) {
        let a = base + i * 2;
        out.indices
            .extend_from_slice(&[a, a + 1, a + 2, a + 1, a + 3, a + 2]);
    }
}

/// An upright frond: a tapered strip standing out of the substrate, leaning
/// with the prop's sway.
#[allow(clippy::too_many_arguments)]
pub fn frond(
    out: &mut Mesh,
    root: Vec2,
    floor_z: f32,
    lean: f32,
    height: f32,
    half_width: f32,
    color: [f32; 4],
    region: Region,
) {
    const JOINTS: usize = 8;
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
            Vec2::new(root.x + bend * height * 0.55, root.y + bend * height * 0.18),
            half_width + 2.0,
        );
        let (x, y) = (tip.x, tip.y);
        let z = floor_z + 1.0 + height * t;
        let w = half_width * (0.08 + 0.92 * (std::f32::consts::PI * t).sin().powf(0.7));
        let k = 0.72 + 0.28 * t;
        let c = [color[0] * k, color[1] * k, color[2] * k, color[3]];
        for side in [-1.0f32, 0.0, 1.0] {
            out.verts.push(Vertex {
                texcoord: [0.0; 4],
                pos: [x + side * w, y - (1.0 - side.abs()) * w * 0.22, z],
                normal: [side * 0.35, -0.65, 0.76],
                color: c,
                material: Material::SKIN,
            });
        }
    }
    for j in 0..JOINTS as u32 {
        let a = base + j * 3;
        for side in 0..2 {
            let k = a + side;
            out.indices
                .extend_from_slice(&[k, k + 1, k + 3, k + 1, k + 4, k + 3]);
        }
    }
}

/// A tuft of fronds: the terrarium plant. Sways with `stir`.
#[allow(clippy::too_many_arguments)]
pub fn tuft(
    out: &mut Mesh,
    at: Vec2,
    floor_z: f32,
    radius: f32,
    stir: f32,
    phase: f32,
    color: [f32; 4],
    region: Region,
) {
    let blades = 6;
    let amp = 0.14 + 0.36 * stir;
    for i in 0..blades {
        let f = i as f32 / (blades - 1) as f32;
        let lean = (f - 0.5) * 0.7 + (phase + f * 2.4).sin() * amp;
        let height = radius * (2.6 + ((i % 3) as f32) * 0.8);
        frond(
            out,
            region.clamp_inside(
                Vec2::new(
                    at.x + (f - 0.5) * radius * 0.8,
                    at.y + ((i % 2) as f32 - 0.5) * radius * 0.5,
                ),
                radius * 0.24 + 2.0,
            ),
            floor_z,
            lean,
            height,
            radius * 0.22,
            color,
            region,
        );
    }
}

/// A soft dark patch on the floor directly under something. Without it, props
/// read as hovering a little above the floor: the renderer's one shadow pass
/// is spent on the creature, and nothing else in the scene touches the ground
/// visibly. Cheap, and it is most of what makes an enclosure look inhabited.
pub fn contact_shade(out: &mut Mesh, at: Vec2, z: f32, r: f32) {
    dome(out, at, z, r * 1.35, 0.0, [0.0, 0.0, 0.0, 0.22]);
}

/// A pointed, folded broad leaf, bounded by its supplied radius.
pub fn leaf(out: &mut Mesh, at: [f32; 3], radius: f32, angle: f32, color: [f32; 4]) {
    let (sn, cs) = angle.sin_cos();
    let p = |x: f32, y: f32, z: f32| [at[0] + cs * x - sn * y, at[1] + sn * x + cs * y, at[2] + z];
    let root = p(-radius, 0.0, 0.0);
    let tip = p(radius, 0.0, radius * 0.08);
    let mid = p(0.0, 0.0, radius * 0.17);
    for side in [-1.0f32, 1.0] {
        let edge = p(-radius * 0.05, side * radius * 0.48, 0.0);
        let tone = if side < 0.0 { 0.88 } else { 1.05 };
        let col = [color[0] * tone, color[1] * tone, color[2] * tone, color[3]];
        tri(
            out,
            [root, edge, mid],
            [-sn * side * 0.2, cs * side * 0.2, 0.98],
            col,
        );
        tri(
            out,
            [mid, edge, tip],
            [-sn * side * 0.2, cs * side * 0.2, 0.98],
            col,
        );
    }
    tube(
        out,
        root,
        tip,
        radius * 0.022,
        5,
        false,
        [color[0] * 1.2, color[1] * 1.2, color[2] * 1.2, color[3]],
    );
}

/// Low-relief dry substrate. Shared grid vertices keep the mottling continuous;
/// deterministic variation stays attached to the enclosure when it moves.
#[allow(clippy::too_many_arguments)]
pub fn substrate(
    out: &mut Mesh,
    lo: Vec2,
    hi: Vec2,
    z: f32,
    relief: f32,
    variation: f32,
    seed: u32,
    color: [f32; 4],
) {
    const CELLS: usize = 24;
    const SIDE: usize = CELLS + 1;
    let mut scatter = Scatter::new(seed);
    let mut noise = [0.0f32; SIDE * SIDE];
    for value in &mut noise {
        *value = scatter.next();
    }
    let field = |x: usize, y: usize| {
        let fine = noise[y * SIDE + x];
        let broad = ((x as f32 * 0.43).sin() * (y as f32 * 0.37 + 0.8).cos() + 1.0) * 0.5;
        fine * 0.48 + broad * 0.52
    };
    let dx = (hi.x - lo.x) / CELLS as f32;
    let dy = (hi.y - lo.y) / CELLS as f32;
    let base = out.verts.len() as u32;
    for y in 0..SIDE {
        for x in 0..SIDE {
            let h = field(x, y);
            // Keep the perimeter at its established elevation for a sealed edge.
            let edge = if x == 0 || y == 0 || x == CELLS || y == CELLS {
                0.0
            } else {
                1.0
            };
            let nx = (field(x.saturating_sub(1), y) - field((x + 1).min(CELLS), y)) * relief
                / (2.0 * dx).max(0.01);
            let ny = (field(x, y.saturating_sub(1)) - field(x, (y + 1).min(CELLS))) * relief
                / (2.0 * dy).max(0.01);
            let norm = (nx * nx + ny * ny + 1.0).sqrt();
            let tone = 1.0 + (h - 0.5) * variation * 2.0;
            out.verts.push(Vertex {
                texcoord: [0.0; 4],
                pos: [
                    lo.x + x as f32 * dx,
                    lo.y + y as f32 * dy,
                    z + h * relief * edge,
                ],
                normal: [nx / norm, ny / norm, 1.0 / norm],
                color: [color[0] * tone, color[1] * tone, color[2] * tone, color[3]],
                material: Material::MATTE,
            });
        }
    }
    for y in 0..CELLS {
        for x in 0..CELLS {
            let a = base + (y * SIDE + x) as u32;
            let b = a + 1;
            let c = a + SIDE as u32;
            let d = c + 1;
            out.indices.extend_from_slice(&[a, b, d, a, d, c]);
        }
    }
}
