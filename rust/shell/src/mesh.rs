//! Primitive mesh generation — the SceneKit shapes the fly is built from.
//!
//! `SCNSphere`, `SCNCapsule` and `SCNCone` come free on Apple platforms; on
//! wgpu they have to be generated. This is the concrete shape of the
//! "SceneKit is doing more for this app than it looks" cost in PORT_PLAN.md §2.

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub verts: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn append(&mut self, other: &Mesh, base_color: [f32; 4]) {
        let off = self.verts.len() as u32;
        self.verts.extend(other.verts.iter().map(|v| Vertex {
            pos: v.pos,
            normal: v.normal,
            color: base_color,
        }));
        self.indices.extend(other.indices.iter().map(|i| i + off));
    }
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}

/// `SCNSphere(radius:)` — a UV sphere centred on the origin.
pub fn sphere(radius: f32, rings: u32, sectors: u32) -> Mesh {
    let mut m = Mesh::default();
    for r in 0..=rings {
        let phi = std::f32::consts::PI * r as f32 / rings as f32;
        for s in 0..=sectors {
            let theta = std::f32::consts::TAU * s as f32 / sectors as f32;
            let n = [
                phi.sin() * theta.cos(),
                phi.cos(),
                phi.sin() * theta.sin(),
            ];
            m.verts.push(Vertex {
                pos: [n[0] * radius, n[1] * radius, n[2] * radius],
                normal: n,
                color: [1.0; 4],
            });
        }
    }
    let stride = sectors + 1;
    for r in 0..rings {
        for s in 0..sectors {
            let a = r * stride + s;
            let b = a + stride;
            m.indices
                .extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    m
}

/// `SCNCapsule(capRadius:height:)` — a y-axis capsule whose **total** height is
/// `height` (including both hemispherical caps), centred on the origin, exactly
/// as SceneKit defines it. Getting this wrong makes every leg segment too long.
pub fn capsule(cap_radius: f32, height: f32, rings: u32, sectors: u32) -> Mesh {
    let mut m = Mesh::default();
    let cyl_half = (height / 2.0 - cap_radius).max(0.0);
    let half_rings = rings.max(2) / 2;

    let mut ring = |y_center: f32, phi_from: f32, phi_to: f32, steps: u32, m: &mut Mesh| {
        for r in 0..=steps {
            let phi = phi_from + (phi_to - phi_from) * r as f32 / steps as f32;
            for s in 0..=sectors {
                let theta = std::f32::consts::TAU * s as f32 / sectors as f32;
                let n = [
                    phi.sin() * theta.cos(),
                    phi.cos(),
                    phi.sin() * theta.sin(),
                ];
                m.verts.push(Vertex {
                    pos: [
                        n[0] * cap_radius,
                        n[1] * cap_radius + y_center,
                        n[2] * cap_radius,
                    ],
                    normal: n,
                    color: [1.0; 4],
                });
            }
        }
    };

    // Top cap, then bottom cap; the cylinder is the band between them.
    ring(cyl_half, 0.0, std::f32::consts::FRAC_PI_2, half_rings, &mut m);
    ring(
        -cyl_half,
        std::f32::consts::FRAC_PI_2,
        std::f32::consts::PI,
        half_rings,
        &mut m,
    );

    let stride = sectors + 1;
    let rows = m.verts.len() as u32 / stride;
    for r in 0..rows - 1 {
        for s in 0..sectors {
            let a = r * stride + s;
            let b = a + stride;
            m.indices
                .extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    m
}

/// `SCNCone(topRadius:bottomRadius:height:)` — y-axis, centred on the origin.
pub fn cone(top_radius: f32, bottom_radius: f32, height: f32, sectors: u32) -> Mesh {
    let mut m = Mesh::default();
    let half = height / 2.0;
    let slope = normalize([height, bottom_radius - top_radius, 0.0]);
    for s in 0..=sectors {
        let theta = std::f32::consts::TAU * s as f32 / sectors as f32;
        let (st, ct) = theta.sin_cos();
        let n = normalize([slope[0] * ct, slope[1], slope[0] * st]);
        m.verts.push(Vertex {
            pos: [top_radius * ct, half, top_radius * st],
            normal: n,
            color: [1.0; 4],
        });
        m.verts.push(Vertex {
            pos: [bottom_radius * ct, -half, bottom_radius * st],
            normal: n,
            color: [1.0; 4],
        });
    }
    for s in 0..sectors {
        let a = s * 2;
        m.indices
            .extend_from_slice(&[a, a + 1, a + 2, a + 2, a + 1, a + 3]);
    }
    // Caps, so the cone is closed from every angle.
    for (y, ny, radius) in [(half, 1.0f32, top_radius), (-half, -1.0f32, bottom_radius)] {
        if radius <= 0.0001 {
            continue;
        }
        let center = m.verts.len() as u32;
        m.verts.push(Vertex {
            pos: [0.0, y, 0.0],
            normal: [0.0, ny, 0.0],
            color: [1.0; 4],
        });
        for s in 0..=sectors {
            let theta = std::f32::consts::TAU * s as f32 / sectors as f32;
            m.verts.push(Vertex {
                pos: [radius * theta.cos(), y, radius * theta.sin()],
                normal: [0.0, ny, 0.0],
                color: [1.0; 4],
            });
        }
        for s in 0..sectors {
            let a = center + 1 + s;
            if ny > 0.0 {
                m.indices.extend_from_slice(&[center, a, a + 1]);
            } else {
                m.indices.extend_from_slice(&[center, a + 1, a]);
            }
        }
    }
    m
}

/// The wing: `SCNShape` extruding an oval Bezier path
/// (`FlyModel.swift:130` — `NSBezierPath(ovalIn:)`, 5.2 x 16.5, extruded 0.12).
/// Approximated as a flat elliptical disc, which is what it reads as on screen.
pub fn wing(width: f32, height: f32, y_offset: f32, sectors: u32) -> Mesh {
    let mut m = Mesh::default();
    let (rx, ry) = (width / 2.0, height / 2.0);
    let center = 0u32;
    m.verts.push(Vertex {
        pos: [0.0, y_offset, 0.0],
        normal: [0.0, 0.0, 1.0],
        color: [1.0; 4],
    });
    for s in 0..=sectors {
        let theta = std::f32::consts::TAU * s as f32 / sectors as f32;
        m.verts.push(Vertex {
            pos: [rx * theta.cos(), y_offset + ry * theta.sin(), 0.0],
            normal: [0.0, 0.0, 1.0],
            color: [1.0; 4],
        });
    }
    for s in 0..sectors {
        m.indices
            .extend_from_slice(&[center, center + 1 + s, center + 2 + s]);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn indices_are_valid(m: &Mesh) -> bool {
        m.indices.iter().all(|&i| (i as usize) < m.verts.len()) && m.indices.len() % 3 == 0
    }

    #[test]
    fn sphere_has_the_requested_radius_and_valid_topology() {
        let m = sphere(4.6, 16, 24);
        let (lo, hi) = bounds(&m);
        for i in 0..3 {
            assert!((hi[i] - 4.6).abs() < 0.01, "axis {i} max {}", hi[i]);
            assert!((lo[i] + 4.6).abs() < 0.01, "axis {i} min {}", lo[i]);
        }
        assert!(indices_are_valid(&m));
        // Every normal is unit length and points outward from the centre.
        for v in &m.verts {
            let l = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!((l - 1.0).abs() < 1e-3);
        }
    }

    /// SceneKit's capsule `height` is the TOTAL height including both caps.
    /// Treating it as the cylinder length makes every leg segment too long,
    /// which is exactly the kind of silent geometry drift a port introduces.
    #[test]
    fn capsule_total_height_includes_the_caps() {
        let m = capsule(0.48, 4.2, 12, 16);
        let (lo, hi) = bounds(&m);
        assert!((hi[1] - 2.1).abs() < 0.02, "top {}", hi[1]);
        assert!((lo[1] + 2.1).abs() < 0.02, "bottom {}", lo[1]);
        assert!((hi[0] - 0.48).abs() < 0.02, "radius {}", hi[0]);
        assert!(indices_are_valid(&m));
    }

    #[test]
    fn capsule_degenerates_to_a_sphere_when_height_equals_diameter() {
        let m = capsule(1.0, 2.0, 12, 16);
        let (lo, hi) = bounds(&m);
        assert!((hi[1] - 1.0).abs() < 0.02 && (lo[1] + 1.0).abs() < 0.02);
    }

    #[test]
    fn cone_matches_its_radii_and_height() {
        let m = cone(0.6, 0.22, 2.4, 16);
        let (lo, hi) = bounds(&m);
        assert!((hi[1] - 1.2).abs() < 0.01 && (lo[1] + 1.2).abs() < 0.01);
        assert!((hi[0] - 0.6).abs() < 0.02, "widest radius {}", hi[0]);
        assert!(indices_are_valid(&m));
    }

    /// The Swift wing is `NSBezierPath(ovalIn: NSRect(x: -2.6, y: -15.5,
    /// width: 5.2, height: 16.5))`, so it spans y = -15.5 .. +1.0 — mostly
    /// behind the hinge, with a small leading edge in front of it.
    #[test]
    fn wing_matches_the_swift_bezier_oval_bounds() {
        let m = wing(5.2, 16.5, -7.25, 24);
        let (lo, hi) = bounds(&m);
        assert!((hi[2] - lo[2]).abs() < 1e-5, "wing must be flat in z");
        assert!((lo[1] + 15.5).abs() < 0.05, "trailing edge {}", lo[1]);
        assert!((hi[1] - 1.0).abs() < 0.05, "leading edge {}", hi[1]);
        assert!((hi[0] - 2.6).abs() < 0.05, "half-width {}", hi[0]);
        assert!(indices_are_valid(&m));
    }
}
