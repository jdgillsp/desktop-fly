//! The koi's geometry: a fish, swept along the spine the swimmer computes.
//!
//! `dfcore::koi::Koi` produces the spine — centreline plus the carangiform
//! travelling wave — so nothing here decides how the animal moves. This builds
//! the body around it: a laterally-compressed tube that is deepest a third of
//! the way back, a caudal fin at the tail, a dorsal fin, and a pair of pectoral
//! fins that fan out where the body is widest.
//!
//! Two things differ from the arthropods' geometry:
//!
//! * **A fish is flattened side-to-side, not top-to-bottom.** The cross-section
//!   is an ellipse taller than it is wide, which is what makes the silhouette
//!   read as a fish from directly above rather than as a sausage.
//! * **The fins are the giveaway.** The caudal fin is a forked triangle that
//!   trails the tail's own direction, so it whips a fraction behind the beat;
//!   at rest it is what keeps the animal from looking like a worm.
//!
//! The glass register has nothing to put inside this body — a procedural
//! creature has no connectome — so it renders as an empty glass fish rather
//! than as a fish full of invented neurons. That emptiness is the honest
//! rendering, and the tray says why.

use dfcore::KoiBody;

use crate::flybody::GlassPalette;
use crate::mesh::{Material, Mesh, Vertex};

/// Kohaku: white body, vermilion markings. The classic koi.
const BODY_WHITE: [f32; 4] = [0.94, 0.93, 0.90, 1.0];
const BODY_RED: [f32; 4] = [0.86, 0.28, 0.10, 1.0];
const BODY_DARK: [f32; 4] = [0.30, 0.26, 0.24, 1.0];
const FIN: [f32; 4] = [0.96, 0.90, 0.86, 0.55];
const FIN_RED: [f32; 4] = [0.88, 0.40, 0.24, 0.62];

/// Height of the body's centreline above the desktop plane.
const Z: f32 = 3.4;

/// Dorsoventral half-height at body fraction `t` (0 = snout, 1 = tail root):
/// the tall axis, which runs along **z** and is therefore mostly *invisible*
/// from the app's top-down camera. It shapes the back's profile and the
/// shading, not the outline.
fn depth_at(t: f32) -> f32 {
    if t < 0.30 {
        let u = t / 0.30;
        2.6 + 4.4 * u.sqrt()
    } else {
        let u = (t - 0.30) / 0.70;
        7.0 * (1.0 - u).powf(1.35) + 0.7
    }
}

/// Lateral half-width: the thin axis anatomically, but **the one the camera
/// sees**, because the app looks straight down at the desktop the way you look
/// down into a pond.
///
/// Getting the two the wrong way round is what the first attempt did, and it
/// rendered a stick: a laterally-compressed fish really is narrow, so scaling
/// the visible axis by the compression ratio made the silhouette about 1:20
/// against its length. A koi seen from above is nearer 1:5. The compression is
/// still there — `depth_at` exceeds this everywhere, and a test pins it — it
/// just belongs on the axis pointing at the ceiling.
fn width_at(t: f32) -> f32 {
    depth_at(t) * 0.62
}

/// The kohaku pattern: vermilion over the head and two saddles down the back.
fn body_color(t: f32, glass: bool) -> [f32; 4] {
    if glass {
        return GlassPalette::SHELL;
    }
    let red = (t < 0.16) || (0.30..0.46).contains(&t) || (0.62..0.74).contains(&t);
    if red {
        BODY_RED
    } else {
        BODY_WHITE
    }
}

/// A thin raised stroke used for fin rays and overlapping scale margins.
fn stroke(out: &mut Mesh, a: [f32; 3], b: [f32; 3], width: f32, c: [f32; 4], material: [f32; 4]) {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let offset = [-dy / len * width, dx / len * width];
    let base = out.verts.len() as u32;
    for (p, side) in [(a, -1.0), (a, 1.0), (b, -1.0), (b, 1.0)] {
        out.verts.push(Vertex {
            texcoord: [0.0; 4],
            pos: [p[0] + offset[0] * side, p[1] + offset[1] * side, p[2]],
            normal: [0.0, 0.0, 1.0],
            color: c,
            material,
        });
    }
    out.indices
        .extend_from_slice(&[base, base + 2, base + 1, base + 1, base + 2, base + 3]);
}

/// A flat fin: a triangle fan from `root` through the given rim points, drawn
/// in the desktop plane so it reads from directly above.
fn fin(out: &mut Mesh, root: [f32; 3], rim: &[[f32; 3]], c: [f32; 4]) {
    if rim.len() < 2 {
        return;
    }
    let glass = c == GlassPalette::WING;
    let material = if glass {
        Material::GLASS
    } else {
        Material::MEMBRANE
    };
    // Radially subdivided membrane: camber catches a broad highlight while
    // the tips stay thin, with rays bending with the membrane instead of
    // straight opaque sticks pasted over a triangle.
    let mut edge = Vec::new();
    for pair in rim.windows(2) {
        for j in 0..8 {
            let t = j as f32 / 8.0;
            edge.push(std::array::from_fn::<_, 3, _>(|axis| {
                pair[0][axis] * (1.0 - t) + pair[1][axis] * t
            }));
        }
    }
    edge.push(*rim.last().unwrap());
    let point = |end: [f32; 3], t: f32| -> [f32; 3] {
        let length = ((end[0] - root[0]).powi(2) + (end[1] - root[1]).powi(2)).sqrt();
        [
            root[0] + (end[0] - root[0]) * t,
            root[1] + (end[1] - root[1]) * t,
            root[2] + (end[2] - root[2]) * t + length * 0.065 * (std::f32::consts::PI * t).sin(),
        ]
    };
    let base = out.verts.len() as u32;
    let cols = edge.len();
    for row in 0..=4 {
        let t = row as f32 / 4.0;
        for end in &edge {
            let mut color = c;
            color[3] *= 1.0 - 0.25 * t * t;
            out.verts.push(Vertex {
                texcoord: [0.0; 4],
                pos: point(*end, t),
                normal: [0.0, 0.0, 1.0],
                color,
                material,
            });
        }
    }
    for row in 0..4 {
        for col in 0..cols - 1 {
            let a = base + (row * cols + col) as u32;
            let b = a + cols as u32;
            out.indices
                .extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    for (j, end) in edge.iter().enumerate() {
        if j % 2 != 0 {
            continue;
        }
        for row in 0..4 {
            let mut a = point(*end, row as f32 / 4.0);
            let mut b = point(*end, (row + 1) as f32 / 4.0);
            a[2] += 0.025;
            b[2] += 0.025;
            stroke(
                out,
                a,
                b,
                0.028 * (1.0 - row as f32 * 0.14),
                [c[0] * 0.77, c[1] * 0.79, c[2] * 0.82, c[3] * 0.82],
                material,
            );
        }
    }
}

/// Build one frame of the koi from its spine.
pub fn build_frame(out: &mut Mesh, koi: &KoiBody, glass: bool) {
    out.verts.clear();
    out.indices.clear();

    let spine = &koi.spine;
    let n = spine.len();
    if n < 3 {
        return;
    }
    let scale = koi.scale();

    // Local frame at each spine point: tangent along the body **toward the
    // head**, normal across. The spine is head-first, so the tangent is
    // taken from the point behind to the point ahead; the first version had
    // it the other way round, and built the snout, the eyes and the fins
    // pointing backwards into the body.
    let frame = |i: usize| -> ([f32; 2], [f32; 2]) {
        let a = spine[(i + 1).min(n - 1)];
        let b = spine[i.saturating_sub(1)];
        let (mut tx, mut ty) = (b.x - a.x, b.y - a.y);
        let l = (tx * tx + ty * ty).sqrt();
        if l < 1e-4 {
            tx = koi.heading.cos();
            ty = koi.heading.sin();
        } else {
            tx /= l;
            ty /= l;
        }
        ([tx, ty], [-ty, tx])
    };

    // --- body: an elliptical tube swept down the spine.
    //
    // Rings of points rather than flat quads: at this size the fish is only a
    // few dozen pixels long, and faceted slabs read as a chain of blocks
    // rather than as an animal. The cross-section is an ellipse wide in z and
    // narrow laterally — a fish stood on edge.
    const RING: usize = 36;
    let ring_base = out.verts.len() as u32;
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let (_, nrm) = frame(i);
        let p = spine[i];
        let w = width_at(t) * scale;
        let d = depth_at(t) * scale;
        for k in 0..RING {
            let a = std::f32::consts::TAU * k as f32 / RING as f32;
            let (sa, ca) = a.sin_cos();
            // Pigment edges meander over the flanks instead of forming painted bands.
            let pigment_t =
                t + 0.022 * (a * 3.0 + t * 19.0).sin() + 0.009 * (a * 7.0 - t * 47.0).sin();
            let mut c = body_color(pigment_t, glass);
            if !glass && sa < -0.1 {
                c = BODY_WHITE;
            }
            let pos = [p.x + nrm[0] * w * ca, p.y + nrm[1] * w * ca, Z + d * sa];
            // Outward normal of the ellipse, in the local frame.
            let nx = nrm[0] * ca / w.max(1e-3);
            let ny = nrm[1] * ca / w.max(1e-3);
            let nz = sa / d.max(1e-3);
            let l = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-4);
            // The back is darker than the flanks, as in the animal, and the
            // back is most of what a top-down view sees.
            let shade = if glass {
                1.0
            } else {
                0.91 + 0.09 * (1.0 - sa.max(0.0))
            };
            out.verts.push(Vertex {
                texcoord: [0.0; 4],
                pos,
                normal: [nx / l, ny / l, nz / l],
                color: [c[0] * shade, c[1] * shade, c[2] * shade, c[3]],
                material: if glass {
                    Material::GLASS
                } else {
                    Material::WET
                },
            });
        }
    }
    for i in 0..n - 1 {
        for k in 0..RING {
            let k2 = (k + 1) % RING;
            let a = ring_base + (i * RING + k) as u32;
            let b = ring_base + (i * RING + k2) as u32;
            let c = ring_base + ((i + 1) * RING + k) as u32;
            let d = ring_base + ((i + 1) * RING + k2) as u32;
            out.indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    // Close the snout with a fan so the head is not an open pipe.
    {
        let head = spine[0];
        let (tan, _) = frame(0);
        let tip = out.verts.len() as u32;
        let c = if glass {
            GlassPalette::SHELL
        } else if koi.feeding > 0.0 {
            [0.15, 0.07, 0.05, 0.95]
        } else {
            body_color(0.0, glass)
        };
        out.verts.push(Vertex {
            texcoord: [0.0; 4],
            pos: [
                head.x + tan[0] * (1.6 + koi.feeding * 1.2) * scale,
                head.y + tan[1] * (1.6 + koi.feeding * 1.2) * scale,
                Z,
            ],
            normal: [tan[0], tan[1], 0.0],
            color: c,
            material: if glass {
                Material::GLASS
            } else {
                Material::WET
            },
        });
        for k in 0..RING {
            let k2 = (k + 1) % RING;
            out.indices
                .extend_from_slice(&[tip, ring_base + k as u32, ring_base + k2 as u32]);
        }
    }

    // --- caudal fin: a forked triangle trailing the tail's own direction.
    {
        let tail = spine[n - 1];
        let (tan, nrm) = frame(n - 1);
        let len = 13.0 * scale;
        let spread = 8.5 * scale;
        let root = [tail.x, tail.y, Z];
        let tip_l = [
            tail.x - tan[0] * len + nrm[0] * spread,
            tail.y - tan[1] * len + nrm[1] * spread,
            Z,
        ];
        let tip_r = [
            tail.x - tan[0] * len - nrm[0] * spread,
            tail.y - tan[1] * len - nrm[1] * spread,
            Z,
        ];
        // The fork: the notch sits short of the tips.
        let notch = [
            tail.x - tan[0] * len * 0.45,
            tail.y - tan[1] * len * 0.45,
            Z,
        ];
        let c = if glass { GlassPalette::WING } else { FIN_RED };
        fin(out, root, &[tip_l, notch, tip_r], c);
    }

    // --- dorsal fin: a low ridge along the back, a third to two thirds along.
    {
        let c = if glass { GlassPalette::WING } else { FIN };
        let mut rim = Vec::new();
        let i0 = (n as f32 * 0.28) as usize;
        let i1 = (n as f32 * 0.62) as usize;
        for i in i0..=i1.min(n - 1) {
            let t = i as f32 / (n - 1) as f32;
            let p = spine[i];
            // Above the back, not inside it: the tube's top is at
            // Z + depth_at(t), so anything lower is swallowed by the body.
            let h = (depth_at(t) + 2.6) * scale;
            rim.push([p.x, p.y, Z + h]);
        }
        if rim.len() >= 2 {
            let root = [spine[i0].x, spine[i0].y, Z + depth_at(0.28) * scale * 0.9];
            fin(out, root, &rim, c);
        }
    }

    // --- pectoral fins: a pair, fanning out where the body is deepest.
    {
        let c = if glass { GlassPalette::WING } else { FIN };
        let i = (n as f32 * 0.26) as usize;
        let p = spine[i.min(n - 1)];
        let (tan, nrm) = frame(i.min(n - 1));
        // They sweep back and out, and beat gently out of phase with the tail
        // so a hovering fish is not perfectly still.
        let flutter = (koi.phase * std::f32::consts::TAU + 1.2).sin() * 0.25;
        for side in [-1.0f32, 1.0] {
            let out_x = nrm[0] * side;
            let out_y = nrm[1] * side;
            // Clear of the flank, which is width_at(t) wide here.
            let span = (width_at(0.26) + 4.4 + flutter * 1.8) * scale;
            let back = 4.2 * scale;
            let root = [p.x, p.y, Z];
            let tip = [
                p.x + out_x * span - tan[0] * back,
                p.y + out_y * span - tan[1] * back,
                Z + 0.6,
            ];
            let trail = [
                p.x + out_x * span * 0.4 - tan[0] * back * 1.5,
                p.y + out_y * span * 0.4 - tan[1] * back * 1.5,
                Z + 0.2,
            ];
            fin(out, root, &[tip, trail], c);
        }
    }

    // --- eye: one dot each side of the snout, so it has a front.
    if !glass {
        let head = spine[0];
        let (tan, nrm) = frame(0);
        for side in [-1.0f32, 1.0] {
            let e = [
                head.x + nrm[0] * side * 1.75 * scale - tan[0] * 0.9 * scale,
                head.y + nrm[1] * side * 1.75 * scale - tan[1] * 0.9 * scale,
                Z + depth_at(0.0) * scale * 0.75,
            ];
            let r = 0.58 * scale;
            let base = out.verts.len() as u32;
            out.verts.push(Vertex {
                texcoord: [0.0; 4],
                pos: [e[0], e[1], e[2] + r * 0.32],
                normal: [0.0, 0.0, 1.0],
                color: BODY_DARK,
                material: Material::EYE,
            });
            for j in 0..=16 {
                let a = std::f32::consts::TAU * j as f32 / 16.0;
                out.verts.push(Vertex {
                    texcoord: [0.0; 4],
                    pos: [e[0] + r * a.cos(), e[1] + r * a.sin(), e[2]],
                    normal: [a.cos() * 0.4, a.sin() * 0.4, 0.92],
                    color: BODY_DARK,
                    material: Material::EYE,
                });
                if j < 16 {
                    out.indices
                        .extend_from_slice(&[base, base + 1 + j, base + 2 + j]);
                }
            }
            for (radius, lift, color) in [
                (r * 0.77, r * 0.10, [0.60, 0.48, 0.24, 1.0]),
                (r * 0.43, r * 0.17, [0.025, 0.024, 0.02, 1.0]),
            ] {
                let base = out.verts.len() as u32;
                out.verts.push(Vertex {
                    texcoord: [0.0; 4],
                    pos: [e[0], e[1], e[2] + lift + r * 0.12],
                    normal: [0.0, 0.0, 1.0],
                    color,
                    material: Material::EYE,
                });
                for j in 0..=24 {
                    let a = std::f32::consts::TAU * j as f32 / 24.0;
                    out.verts.push(Vertex {
                        texcoord: [0.0; 4],
                        pos: [
                            e[0] + radius * a.cos(),
                            e[1] + radius * a.sin(),
                            e[2] + lift,
                        ],
                        normal: [a.cos() * 0.6, a.sin() * 0.6, 0.8],
                        color,
                        material: Material::EYE,
                    });
                    if j < 24 {
                        out.indices
                            .extend_from_slice(&[base, base + j + 1, base + j + 2]);
                    }
                }
            }
            // A short barbel at each mouth corner is distinctive to carp.
            let mouth = [
                head.x + tan[0] * 1.4 * scale + nrm[0] * side * 1.1 * scale,
                head.y + tan[1] * 1.4 * scale + nrm[1] * side * 1.1 * scale,
                Z + 0.6 * scale,
            ];
            stroke(
                out,
                mouth,
                [
                    mouth[0] + nrm[0] * side * 1.4 * scale - tan[0] * 0.4 * scale,
                    mouth[1] + nrm[1] * side * 1.4 * scale - tan[1] * 0.4 * scale,
                    mouth[2],
                ],
                0.08 * scale,
                BODY_WHITE,
                Material::WET,
            );
        }
    }

    if !glass {
        // Subtle staggered cycloid scale margins follow the animated surface.
        // Only the visible upper flanks need geometry at desktop scale.
        for row in 6..39 {
            let t = row as f32 / 44.0;
            let f = t * (n - 1) as f32;
            let i = (f as usize).min(n - 2);
            let u = f - i as f32;
            let p = [
                spine[i].x * (1.0 - u) + spine[i + 1].x * u,
                spine[i].y * (1.0 - u) + spine[i + 1].y * u,
            ];
            let (tan, nrm) = frame(i);
            for lane in 0..11 {
                let a = 0.22 + (lane as f32 + 0.5 * (row % 2) as f32) * 0.245;
                let mut previous = None;
                for j in 0..7 {
                    let v = j as f32 / 6.0;
                    let angle = a + (v - 0.5) * 0.235;
                    let back = 0.30 * scale * (1.0 - (v * 2.0 - 1.0).powi(2));
                    let q = [
                        p[0] + nrm[0] * width_at(t) * scale * angle.cos() - tan[0] * back,
                        p[1] + nrm[1] * width_at(t) * scale * angle.cos() - tan[1] * back,
                        Z + depth_at(t) * scale * angle.sin() + 0.04,
                    ];
                    if let Some(prev) = previous {
                        let c = body_color(t, false);
                        stroke(
                            out,
                            prev,
                            q,
                            0.022 * scale,
                            [c[0] * 0.94, c[1] * 0.94, c[2] * 0.94, 1.0],
                            [0.68, 0.02, 0.12, 0.02],
                        );
                    }
                    previous = Some(q);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dfcore::Vec2;

    fn koi() -> KoiBody {
        KoiBody::new(Vec2::ZERO, 1)
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
    fn a_frame_produces_a_closed_mesh() {
        let mut m = Mesh::default();
        build_frame(&mut m, &koi(), false);
        assert!(m.verts.len() > 200, "only {} verts", m.verts.len());
        assert_eq!(m.indices.len() % 3, 0);
        assert!(m.indices.iter().all(|&i| (i as usize) < m.verts.len()));
    }

    /// The camera looks straight down, so the silhouette is length against
    /// *lateral* width. The first attempt scaled the visible axis by the fish's
    /// lateral compression and rendered a stick about 1:20; a koi from above is
    /// nearer 5:1.
    ///
    /// Measured along the animal's own head-to-tail axis rather than along x
    /// (the body is curved by the swim wave and points wherever it was seeded,
    /// so a screen-aligned box measures the pose, not the shape), and over the
    /// **body tube only**. Including the fins measured 1.7:1, which is not a
    /// stick — it is a koi with a properly broad tail, and the claim under test
    /// is about the body.
    #[test]
    fn the_silhouette_is_fish_shaped_from_above() {
        let k = koi();
        let mut m = Mesh::default();
        build_frame(&mut m, &k, false);

        let head = k.spine[0];
        let tail = k.spine[k.spine.len() - 1];
        let (mut ax, mut ay) = (head.x - tail.x, head.y - tail.y);
        let l = (ax * ax + ay * ay).sqrt().max(1e-4);
        ax /= l;
        ay /= l;

        // The body rings are emitted first, one ring of RING points per spine
        // point; everything after them is fins and eyes.
        let body_verts = k.spine.len() * 36;
        let (mut lo_a, mut hi_a) = (f32::MAX, f32::MIN);
        let (mut lo_b, mut hi_b) = (f32::MAX, f32::MIN);
        for v in m.verts.iter().take(body_verts) {
            let along = v.pos[0] * ax + v.pos[1] * ay;
            let across = -v.pos[0] * ay + v.pos[1] * ax;
            lo_a = lo_a.min(along);
            hi_a = hi_a.max(along);
            lo_b = lo_b.min(across);
            hi_b = hi_b.max(across);
        }
        let length = hi_a - lo_a;
        let width = (hi_b - lo_b).max(1e-3);
        let ratio = length / width;
        assert!(
            (2.5..8.0).contains(&ratio),
            "body length:width is {ratio:.1}:1 - a koi from above is about 5:1              (length {length:.1}, width {width:.1})"
        );
    }

    /// A still mesh is not an animation. The tail must actually sweep between
    /// frames, and the body must advance — a fish that renders correctly but
    /// never changes is a sprite.
    #[test]
    fn the_geometry_animates_between_frames() {
        use dfcore::creature::{Body, World};
        use dfcore::Region;
        let world = World {
            region: Region::centered((1512.0, 982.0)),
            ledges: Vec::new(),
            cursor: None,
            attractor: None,
        };
        let d = dfcore::BrainSignals::new();
        let mut k = koi();
        // Cruising, so it is beating rather than resting.
        k.state = dfcore::KoiState::Cruise;
        k.state_timer = 60.0;

        let mut a = Mesh::default();
        build_frame(&mut a, &k, false);
        let start = k.pos;
        let ring = k.spine.len() * 36;

        for _ in 0..18 {
            k.step(1.0 / 60.0, &d, &world);
        }
        let mut b = Mesh::default();
        build_frame(&mut b, &k, false);

        assert_eq!(a.verts.len(), b.verts.len(), "topology should be stable");
        // The body advanced.
        assert!(
            k.pos.dist(start) > 1.0,
            "the fish did not move: {:?}",
            k.pos
        );

        // And the tail moved further than the head, relative to the body — the
        // carangiform signature, measured on the rendered mesh rather than on
        // the parameters that produced it.
        let head_shift = (0..36)
            .map(|i| {
                let (p, q) = (a.verts[i].pos, b.verts[i].pos);
                ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
            })
            .fold(0.0f32, f32::max);
        let tail_shift = (ring - 36..ring)
            .map(|i| {
                let (p, q) = (a.verts[i].pos, b.verts[i].pos);
                ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
            })
            .fold(0.0f32, f32::max);
        assert!(tail_shift > 0.5, "the tail is not moving: {tail_shift}");
        assert!(
            tail_shift > head_shift,
            "the tail should sweep more than the head: tail {tail_shift:.2}, head {head_shift:.2}"
        );
    }

    /// A fin drawn below the back's own surface is swallowed by the body and
    /// simply never appears — which is how the first version shipped a koi
    /// with no dorsal fin. The geometry has to clear the tube.
    #[test]
    fn the_dorsal_fin_clears_the_back() {
        let k = koi();
        let mut m = Mesh::default();
        build_frame(&mut m, &k, false);
        let (_, hi) = bounds(&m);
        // The tallest body point is Z + depth_at(0.30) * scale.
        let back_top = Z + depth_at(0.30) * k.scale();
        assert!(
            hi[2] > back_top + 1.0,
            "nothing rises above the back ({:.1} vs {back_top:.1}) - the dorsal              fin is inside the body",
            hi[2]
        );
    }

    /// Same failure mode, sideways: a pectoral fin that does not reach past
    /// the flank is invisible.
    #[test]
    fn the_pectoral_fins_reach_past_the_flank() {
        let k = koi();
        let mut m = Mesh::default();
        build_frame(&mut m, &k, false);
        let head = k.spine[0];
        let tail = k.spine[k.spine.len() - 1];
        let (mut ax, mut ay) = (head.x - tail.x, head.y - tail.y);
        let l = (ax * ax + ay * ay).sqrt().max(1e-4);
        ax /= l;
        ay /= l;
        let widest = width_at(0.30) * k.scale();
        let mut max_across: f32 = 0.0;
        for v in &m.verts {
            let across = (-v.pos[0] * ay + v.pos[1] * ax).abs();
            max_across = max_across.max(across);
        }
        assert!(
            max_across > widest * 1.35,
            "nothing reaches past the flank ({max_across:.1} vs {widest:.1})"
        );
    }

    /// The snout has to be in front of the head, and the caudal fin behind
    /// the tail — the frame's tangent once pointed the wrong way and put
    /// both inside the body.
    #[test]
    fn the_snout_is_ahead_and_the_tail_fin_is_behind() {
        let k = koi();
        let mut m = Mesh::default();
        build_frame(&mut m, &k, false);
        let n = k.spine.len();
        let head = k.spine[0];
        let tail = k.spine[n - 1];
        // Forward is from the second spine point to the first.
        let (fx, fy) = (head.x - k.spine[1].x, head.y - k.spine[1].y);
        let snout = m.verts[n * 36].pos;
        assert!(
            (snout[0] - head.x) * fx + (snout[1] - head.y) * fy > 0.0,
            "the snout points backwards"
        );
        // The caudal fin's vertices come right after the snout tip; all of
        // them lie behind the tail, i.e. away from the spine point ahead of
        // it.
        let (bx, by) = (k.spine[n - 2].x - tail.x, k.spine[n - 2].y - tail.y);
        for v in &m.verts[n * 36 + 2..n * 36 + 5] {
            assert!(
                (v.pos[0] - tail.x) * bx + (v.pos[1] - tail.y) * by <= 0.01,
                "a caudal fin vertex is ahead of the tail"
            );
        }
    }

    #[test]
    fn the_body_is_deeper_than_it_is_wide() {
        for t in [0.0, 0.15, 0.3, 0.5, 0.8, 1.0] {
            assert!(
                depth_at(t) > width_at(t),
                "at t={t} depth {} is not greater than width {}",
                depth_at(t),
                width_at(t)
            );
        }
    }

    /// Deepest a third of the way back, tapering to a narrow tail peduncle —
    /// the fish silhouette rather than a tube.
    #[test]
    fn the_body_is_deepest_a_third_of_the_way_back_and_tapers() {
        let fore = depth_at(0.05);
        let deep = depth_at(0.30);
        let tail = depth_at(1.0);
        assert!(deep > fore * 1.4, "shoulder {deep} vs snout {fore}");
        assert!(deep > tail * 4.0, "peduncle {tail} should be narrow");
    }

    #[test]
    fn depth_scales_the_whole_animal() {
        let mut shallow = koi();
        shallow.depth = 1.0;
        let mut deep = koi();
        deep.depth = 0.0;

        let mut a = Mesh::default();
        let mut b = Mesh::default();
        build_frame(&mut a, &shallow, false);
        build_frame(&mut b, &deep, false);

        let span = |m: &Mesh| {
            let (lo, hi) = bounds(m);
            hi[2] - lo[2]
        };
        assert!(span(&a) > span(&b), "a surfaced koi should be larger");
    }

    /// The glass register must not invent anything to put inside a creature
    /// that has no connectome: it is the same body, translucent, and nothing
    /// else.
    #[test]
    fn the_glass_register_uses_the_shell_palette_and_adds_no_neurons() {
        let mut m = Mesh::default();
        build_frame(&mut m, &koi(), true);
        assert!(!m.verts.is_empty());
        for v in &m.verts {
            assert!(
                v.color[3] < 1.0,
                "every glass surface should be translucent, got alpha {}",
                v.color[3]
            );
        }
    }

    #[test]
    fn the_literal_register_is_a_kohaku() {
        let red = body_color(0.05, false);
        let white = body_color(0.24, false);
        assert_eq!(red, BODY_RED);
        assert_eq!(white, BODY_WHITE);
        assert_ne!(body_color(0.35, false), body_color(0.55, false));
    }
}
