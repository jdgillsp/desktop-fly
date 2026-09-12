//! Authored Blender skin bound to the live behavioral spine.
use crate::mesh::{Material, Mesh, Vertex};
use dfcore::HognoseBody;
use std::sync::OnceLock;

pub struct HognoseAsset {
    vertices: Vec<[f32; 18]>,
    indices: Vec<u32>,
    scale: f32,
    rest_height: f32,
}
static ASSET: OnceLock<Result<HognoseAsset, String>> = OnceLock::new();
impl HognoseAsset {
    pub fn embedded() -> Result<&'static Self, &'static str> {
        ASSET
            .get_or_init(|| Self::parse(include_str!("../../../assets/hognose/mesh.json")))
            .as_ref()
            .map_err(|e| e.as_str())
    }
    fn parse(text: &str) -> Result<Self, String> {
        let data: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("hognose asset JSON: {e}"))?;
        if data["version"].as_u64() != Some(1) {
            return Err("unsupported hognose asset version".into());
        }
        let length = data["reference_length"].as_f64().unwrap_or(0.0) as f32;
        if !length.is_finite() || length <= 0.0 {
            return Err("invalid hognose reference length".into());
        }
        let rows = data["vertices"]
            .as_array()
            .ok_or("missing hognose vertices")?;
        if rows.is_empty() {
            return Err("empty hognose skin".into());
        }
        let mut vertices = Vec::with_capacity(rows.len());
        for row in rows {
            let row = row.as_array().ok_or("invalid hognose vertex")?;
            if row.len() != 18 {
                return Err("hognose vertex stride must be 18".into());
            }
            let mut v = [0.0; 18];
            for (dst, src) in v.iter_mut().zip(row) {
                *dst = src.as_f64().ok_or("non-numeric hognose vertex")? as f32;
            }
            if v.iter().any(|n| !n.is_finite()) || !(0.0..=1.0).contains(&v[0]) {
                return Err("non-finite or out-of-range hognose binding".into());
            }
            if v[4] * v[4] + v[5] * v[5] + v[6] * v[6] < 1e-10 {
                return Err("zero hognose normal".into());
            }
            vertices.push(v);
        }
        let rows = data["indices"]
            .as_array()
            .ok_or("missing hognose indices")?;
        if rows.is_empty() || rows.len() % 3 != 0 {
            return Err("invalid hognose triangles".into());
        }
        let mut indices = Vec::with_capacity(rows.len());
        for i in rows {
            let n = i.as_u64().ok_or("invalid hognose index")?;
            if n >= vertices.len() as u64 {
                return Err("hognose triangle index outside skin".into());
            }
            indices.push(n as u32);
        }
        let scale = dfcore::hognose::BODY_LEN / length;
        // Ground the lowest authored ventral point, including the tapered tail.
        let ventral = vertices
            .iter()
            .filter(|v| v[0] > 0.15)
            .map(|v| v[3])
            .fold(0.0_f32, f32::min);
        Ok(Self {
            vertices,
            indices,
            scale,
            rest_height: -ventral * scale + 0.05,
        })
    }
    pub fn build_frame(&self, out: &mut Mesh, snake: &HognoseBody) {
        out.verts.clear();
        out.indices.clear();
        if snake.spine.len() < 2 {
            return;
        }
        let frames = Spine::new(snake, self.rest_height);
        out.verts.reserve(self.vertices.len());
        for v in &self.vertices {
            let t = v[0];
            let frame = frames.sample(t, snake);
            let hood = if (0.04..0.24).contains(&t) {
                snake.hood * ((t - 0.04) / 0.20 * std::f32::consts::PI).sin() * 1.1
            } else {
                0.0
            };
            let swell = 1.0 + 0.22 * snake.puff * (t * 6.0).min(1.0);
            let widths = [1.0, swell * (1.0 + hood), swell * (1.0 - hood * 0.20)];
            let offset = [
                v[1] * self.scale,
                v[2] * self.scale * widths[1],
                v[3] * self.scale * widths[2],
            ];
            let normal = unit(frame.vector([v[4], v[5] / widths[1], v[6] / widths[2]]));
            out.verts.push(Vertex {
                pos: frame.point(offset),
                normal,
                color: [v[9], v[10], v[11], v[12]],
                material: [v[13], v[14], v[15], v[16]],
                texcoord: [v[7], 1.0 - v[8], v[17], 0.028],
            });
        }
        out.indices.extend_from_slice(&self.indices);
        // A separate animated oral ribbon leaves the authored rostrum intact.
        let head = frames.sample(0.0, snake);
        let roll = snake.belly_up * (1.0 - snake.peek);
        if roll > 0.3 {
            let gape = (roll - 0.3) / 0.7;
            ribbon(
                out,
                &head,
                [
                    [0.08, -0.28, 0.58],
                    [0.08, 0.28, 0.58],
                    [-0.25, 0.24, 0.58 - gape * 0.35],
                    [-0.25, -0.24, 0.58 - gape * 0.35],
                ],
                [0.86, 0.48, 0.52, 1.0],
            );
        }
        let flick = snake.tongue.max(roll * 0.8);
        if flick > 0.05 {
            let end = -0.15 - flick * 1.6;
            ribbon(
                out,
                &head,
                [
                    [0.04, -0.035, 0.58],
                    [0.04, 0.035, 0.58],
                    [end, 0.025, 0.48],
                    [end, -0.025, 0.48],
                ],
                [0.40, 0.08, 0.08, 1.0],
            );
            for side in [-1.0, 1.0] {
                ribbon(
                    out,
                    &head,
                    [
                        [end, -0.025, 0.48],
                        [end, 0.025, 0.48],
                        [end - 0.35, side * 0.16, 0.48],
                        [end - 0.29, side * 0.14, 0.48],
                    ],
                    [0.40, 0.08, 0.08, 1.0],
                );
            }
        }
    }
}
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    add(a, mul(b, -1.0))
}
fn unit(a: [f32; 3]) -> [f32; 3] {
    mul(
        a,
        1.0 / (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-8),
    )
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
struct Frame {
    p: [f32; 3],
    t: [f32; 3],
    s: [f32; 3],
    u: [f32; 3],
}
impl Frame {
    fn vector(&self, v: [f32; 3]) -> [f32; 3] {
        add(add(mul(self.t, v[0]), mul(self.s, v[1])), mul(self.u, v[2]))
    }
    fn point(&self, v: [f32; 3]) -> [f32; 3] {
        add(self.p, self.vector(v))
    }
}
struct Spine {
    p: Vec<[f32; 3]>,
    arc: Vec<f32>,
    total: f32,
}
impl Spine {
    fn new(s: &HognoseBody, rest_height: f32) -> Self {
        let mut arc = vec![0.0];
        let mut p = Vec::with_capacity(s.spine.len());
        for (i, v) in s.spine.iter().enumerate() {
            if i > 0 {
                arc.push(arc[i - 1] + v.dist(s.spine[i - 1]).max(1e-5));
            }
            let neck = (1.0 - i as f32 / 6.0).max(0.0);
            p.push([
                v.x,
                v.y,
                rest_height - 9.0 * s.buried
                    + 6.0 * s.head_lift * neck * neck
                    + 2.0 * s.peek * neck,
            ]);
        }
        let total = *arc.last().unwrap_or(&1.0);
        Self { p, arc, total }
    }
    fn sample(&self, t: f32, snake: &HognoseBody) -> Frame {
        let distance = t.clamp(0.0, 1.0) * self.total;
        let i = self
            .arc
            .partition_point(|x| *x <= distance)
            .saturating_sub(1)
            .min(self.p.len() - 2);
        let u = ((distance - self.arc[i]) / (self.arc[i + 1] - self.arc[i])).clamp(0.0, 1.0);
        let a = self.p[i];
        let b = self.p[i + 1];
        let m0 = if i == 0 {
            sub(b, a)
        } else {
            mul(sub(b, self.p[i - 1]), 0.5)
        };
        let m1 = if i + 2 == self.p.len() {
            sub(b, a)
        } else {
            mul(sub(self.p[i + 2], a), 0.5)
        };
        let u2 = u * u;
        let u3 = u2 * u;
        let p = add(
            add(
                mul(a, 2.0 * u3 - 3.0 * u2 + 1.0),
                mul(b, -2.0 * u3 + 3.0 * u2),
            ),
            add(mul(m0, u3 - 2.0 * u2 + u), mul(m1, u3 - u2)),
        );
        let mut tangent = unit(add(
            add(mul(a, 6.0 * u2 - 6.0 * u), mul(b, -6.0 * u2 + 6.0 * u)),
            add(
                mul(m0, 3.0 * u2 - 4.0 * u + 1.0),
                mul(m1, 3.0 * u2 - 2.0 * u),
            ),
        ));
        if tangent == [0.0; 3] {
            tangent = [-snake.heading.cos(), -snake.heading.sin(), 0.0];
        }
        let side = unit([tangent[1], -tangent[0], 0.0]);
        let up = unit(cross(side, tangent));
        let peek = snake.peek * (1.0 - t * (self.p.len() - 1) as f32 / 6.0).max(0.0);
        let angle = std::f32::consts::PI * snake.belly_up * (1.0 - peek);
        let (sin, cos) = angle.sin_cos();
        Frame {
            p,
            t: tangent,
            s: add(mul(side, cos), mul(up, sin)),
            u: add(mul(up, cos), mul(side, -sin)),
        }
    }
}
fn ribbon(out: &mut Mesh, frame: &Frame, points: [[f32; 3]; 4], color: [f32; 4]) {
    let base = out.verts.len() as u32;
    let normal = unit(frame.vector(cross(sub(points[1], points[0]), sub(points[2], points[0]))));
    for p in points {
        out.verts.push(Vertex {
            pos: frame.point(p),
            normal,
            color,
            material: Material::WET,
            texcoord: [0.0; 4],
        });
    }
    out.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use dfcore::Vec2;
    fn fixture() -> HognoseAsset {
        HognoseAsset {
            vertices: vec![
                [
                    0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.2, 0.0,
                    0.0, 1.0,
                ],
                [
                    0.14, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.2,
                    0.0, 0.0, 1.0,
                ],
                [
                    0.7, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.2, 0.0,
                    0.0, 1.0,
                ],
            ],
            indices: vec![0, 1, 2],
            scale: 1.0,
            rest_height: 0.8,
        }
    }
    #[test]
    fn translation_and_burial_follow_live_spine() {
        let asset = fixture();
        let mut s = HognoseBody::new(Vec2::ZERO, 1);
        s.tongue = 0.0;
        let mut a = Mesh::default();
        asset.build_frame(&mut a, &s);
        for p in &mut s.spine {
            p.x += 31.0;
            p.y -= 17.0;
        }
        s.buried = 1.0;
        let mut b = Mesh::default();
        asset.build_frame(&mut b, &s);
        for (a, b) in a.verts.iter().zip(&b.verts) {
            assert!((b.pos[0] - a.pos[0] - 31.0).abs() < 1e-4);
            assert!((b.pos[1] - a.pos[1] + 17.0).abs() < 1e-4);
            assert!((b.pos[2] - a.pos[2] + 9.0).abs() < 1e-4);
        }
    }
    #[test]
    fn roll_inverts_actual_skin_normals_and_peek_rights_only_head() {
        let asset = fixture();
        let mut s = HognoseBody::new(Vec2::ZERO, 1);
        let mut a = Mesh::default();
        s.belly_up = 1.0;
        asset.build_frame(&mut a, &s);
        assert!(a.verts[0].normal[2] < -0.99);
        assert!(a.verts.len() > 3);
        s.peek = 1.0;
        asset.build_frame(&mut a, &s);
        assert!(a.verts[0].normal[2] > 0.9);
        assert!(a.verts[2].normal[2] < -0.99);
    }
    #[test]
    fn hood_and_lift_deform_authored_vertices() {
        let asset = fixture();
        let mut s = HognoseBody::new(Vec2::ZERO, 1);
        let mut a = Mesh::default();
        asset.build_frame(&mut a, &s);
        s.head_lift = 1.0;
        s.hood = 1.0;
        let mut b = Mesh::default();
        asset.build_frame(&mut b, &s);
        assert!(b.verts[0].pos[2] > a.verts[0].pos[2] + 5.0);
        assert_ne!(b.verts[1].pos, a.verts[1].pos);
        assert_eq!(b.verts[2].pos, a.verts[2].pos);
    }
    #[test]
    fn embedded_asset_is_valid_and_bounded() {
        let asset = HognoseAsset::embedded().expect("authored asset must validate");
        assert!(asset.vertices.len() > 100);
        assert!(asset
            .indices
            .iter()
            .all(|i| (*i as usize) < asset.vertices.len()));
    }
    #[test]
    fn authored_ventral_surface_rests_on_the_ground() {
        let asset = HognoseAsset::embedded().expect("authored asset must validate");
        let mut snake = HognoseBody::new(Vec2::ZERO, 1);
        snake.tongue = 0.0;
        let mut mesh = Mesh::default();
        asset.build_frame(&mut mesh, &snake);
        let lowest = mesh
            .verts
            .iter()
            .zip(&asset.vertices)
            .filter(|(_, binding)| binding[0] > 0.15)
            .map(|(vertex, _)| vertex.pos[2])
            .fold(f32::INFINITY, f32::min);
        assert!(
            (lowest - 0.05).abs() < 0.005,
            "lowest ventral surface: {lowest}"
        );
        assert!(
            asset.rest_height < 1.0,
            "authored narrow body must not float at old primitive height"
        );
    }
    #[test]
    fn changing_spine_moves_skin_without_changing_texture_or_topology() {
        use dfcore::creature::{Body, World};
        let asset = fixture();
        let mut snake = HognoseBody::new(Vec2::ZERO, 19);
        let mut before = Mesh::default();
        asset.build_frame(&mut before, &snake);
        let world = World {
            region: dfcore::Region::centered((800.0, 600.0)),
            ledges: vec![],
            cursor: None,
            attractor: None,
        };
        for _ in 0..60 {
            snake.step(1.0 / 60.0, &dfcore::BrainSignals::new(), &world);
        }
        let mut after = Mesh::default();
        asset.build_frame(&mut after, &snake);
        assert_ne!(before.verts[1].pos, after.verts[1].pos);
        assert_eq!(before.verts[1].texcoord, after.verts[1].texcoord);
        assert_eq!(&before.indices[..3], &after.indices[..3]);
    }
    #[test]
    fn out_of_bounds_indices_and_invalid_binding_are_rejected() {
        let fixture = fixture();
        let mut data = serde_json::json!({"version":1,"reference_length":1.0,"vertices":fixture.vertices,"indices":[0,1,3]});
        assert!(HognoseAsset::parse(&data.to_string()).is_err());
        data["indices"] = serde_json::json!([0, 1, 2]);
        assert!(HognoseAsset::parse(&data.to_string()).is_ok());
        data["vertices"][0][0] = serde_json::json!(-0.1);
        assert!(HognoseAsset::parse(&data.to_string()).is_err());
    }
    #[test]
    fn malformed_asset_is_rejected() {
        assert!(HognoseAsset::parse("{}").is_err());
        assert!(HognoseAsset::parse(
            r#"{"version":1,"reference_length":1,"vertices":[[0]],"indices":[0,0,0]}"#
        )
        .is_err());
    }
}
