//! Contact envelopes for the solid vivarium furniture, in creature coordinates.
use crate::habitatmesh::{creature_lift, wall_height, FLOOR_Z};
use dfcore::{Habitat, PropKind, Vec2};

/// Insert a real graph contact where a strand enters solid furniture. The
/// renderer and walking graph then share the same bent strand, and splitting
/// retains the original amount of material rather than creating free length.
pub fn strands(h: &Habitat, silk: &mut dfcore::silk::Silk) {
    let mut contacts = Vec::new();
    for (i, t) in silk.threads.iter().enumerate() {
        let (Some(a), Some(b)) = (silk.nodes[t.a].spatial, silk.nodes[t.b].spatial) else {
            continue;
        };
        let length = crate::webtravel::distance(a.pos, b.pos);
        if length < 4.0 {
            continue;
        }
        let samples = (length / 2.0).ceil().clamp(2.0, 128.0) as usize;
        for j in 1..samples {
            let u = j as f32 / samples as f32;
            if !(0.1..=0.9).contains(&u) {
                continue;
            }
            let p = std::array::from_fn(|k| a.pos[k] + (b.pos[k] - a.pos[k]) * u);
            let q = project(h, p);
            if crate::webtravel::distance(p, q) > 0.2 {
                contacts.push((i, u, q));
                break;
            }
        }
        if contacts.len() >= 16 {
            break;
        }
    }
    // split_thread replaces one edge and appends another; original indices stay valid.
    for (i, u, q) in contacts {
        if silk.nodes.len() >= 2000 {
            break;
        }
        let t = silk.threads[i];
        let (a, b) = (silk.nodes[t.a].pos, silk.nodes[t.b].pos);
        let n = silk.split_thread(i, Vec2::new(a.x + (b.x - a.x) * u, a.y + (b.y - a.y) * u));
        if let Some(s) = &mut silk.nodes[n].spatial {
            s.pos = q;
            s.velocity = [0.0; 3];
        }
    }
}

pub fn project(h: &Habitat, mut p: [f32; 3]) -> [f32; 3] {
    let floor = FLOOR_Z - creature_lift(h.kind, 0.0);
    for prop in h.props.iter().filter(|p| p.present()) {
        match prop.kind {
            PropKind::Pebble => {
                let c = [prop.pos.x, prop.pos.y, floor + 1.0];
                let r = [prop.radius, prop.radius, prop.radius * 0.62];
                let d: [f32; 3] = std::array::from_fn(|k| (p[k] - c[k]) / r[k].max(0.1));
                let len = d.iter().map(|v| v * v).sum::<f32>().sqrt();
                if len < 1.0 {
                    // Contact from above uses the exposed upper surface, never the buried half.
                    let xy = d[0] * d[0] + d[1] * d[1];
                    p[2] = c[2] + r[2] * (1.0 - xy).max(0.0).sqrt() + 0.05;
                }
            }
            PropKind::Bark => {
                let foot = prop.pos.y.min(h.region.max().y - 6.0);
                let head = h.region.max().y - 1.5;
                let u = (p[1] - foot) / (head - foot);
                if (0.0..=1.0).contains(&u) && (p[0] - prop.pos.x).abs() < prop.radius * 1.5 {
                    let z = floor + 1.0 + (wall_height(h.kind) * 0.62 - 1.0) * u;
                    if p[2] < z && p[2] > z - 5.0 {
                        p[2] = z + 0.05;
                    }
                }
            }
            PropKind::Twig => {
                let root = h.region.clamp_inside(prop.pos, 8.0);
                let corner = if root.x < h.region.center.x {
                    h.region.min().x
                } else {
                    h.region.max().x
                };
                let tip = h.region.clamp_inside(
                    Vec2::new(root.x + (corner - root.x) * 0.55, h.region.max().y - 12.0),
                    10.0,
                );
                let a = [root.x, root.y, floor + 3.6];
                let b = [tip.x, tip.y, floor + wall_height(h.kind) * 0.72];
                capsule(&mut p, a, b, 3.4);
                for (u, dir) in [(0.45, -1.0), (0.7, 1.0)] {
                    let a = std::array::from_fn(|k| a[k] + (b[k] - a[k]) * u);
                    let e = h
                        .region
                        .clamp_inside(Vec2::new(a[0] + dir * 22.0, a[1] - 6.0), 8.0);
                    capsule(&mut p, a, [e.x, e.y, a[2] + 10.0], 1.9);
                }
            }
            _ => {}
        }
    }
    p
}

fn capsule(p: &mut [f32; 3], a: [f32; 3], b: [f32; 3], r: f32) {
    let d: [f32; 3] = std::array::from_fn(|k| b[k] - a[k]);
    let u = ((0..3).map(|k| (p[k] - a[k]) * d[k]).sum::<f32>()
        / d.iter().map(|v| v * v).sum::<f32>())
    .clamp(0.0, 1.0);
    let q: [f32; 3] = std::array::from_fn(|k| a[k] + d[k] * u);
    let n: [f32; 3] = std::array::from_fn(|k| p[k] - q[k]);
    let len = n.iter().map(|v| v * v).sum::<f32>().sqrt();
    if len < r {
        for k in 0..3 {
            p[k] = q[k]
                + if len > 0.001 {
                    n[k] * (r + 0.05) / len
                } else {
                    if k == 1 {
                        -r - 0.05
                    } else {
                        0.0
                    }
                };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_obstructed_strand_gets_a_real_contact_without_extra_material() {
        let mut h = Habitat::new(
            dfcore::HabitatKind::Vivarium,
            dfcore::Region::centered((400.0, 400.0)),
            1,
        );
        h.props.truncate(1);
        h.props[0].kind = PropKind::Pebble;
        h.props[0].pos = Vec2::ZERO;
        h.props[0].radius = 12.0;
        let z = FLOOR_Z - creature_lift(h.kind, 0.0) + 2.0;
        let mut s = dfcore::silk::Silk::new();
        s.pay_out(Vec2::new(-30.0, 0.0), dfcore::ThreadKind::Frame);
        s.attach(Vec2::new(30.0, 0.0), dfcore::silk::Anchor::Fixed);
        s.release();
        s.step_spatial(0.0, [0.0; 3], -1000.0, |n| [n.pos.x, 0.0, z]);
        let length = s.threads[0].material_len.unwrap();
        strands(&h, &mut s);
        assert!(s.nodes.len() > 2);
        let contact = s.nodes.last().unwrap().spatial.unwrap().pos;
        assert!(contact[2] > z);
        assert_eq!(project(&h, contact), contact);
        assert!(
            (s.threads
                .iter()
                .map(|t| t.material_len.unwrap())
                .sum::<f32>()
                - length)
                .abs()
                < 0.001
        );
    }
    #[test]
    fn twig_contact_projects_to_surface_and_is_idempotent() {
        let mut p = [0.0, 0.0, 5.0];
        capsule(&mut p, [0.0, 0.0, 0.0], [0.0, 0.0, 10.0], 3.4);
        assert!((p[1] + 3.45).abs() < 0.001);
        let before = p;
        capsule(&mut p, [0.0, 0.0, 0.0], [0.0, 0.0, 10.0], 3.4);
        assert_eq!(p, before);
    }
}
