//! Embed the construction chart in the habitat, then relax its silk in 3D.
use crate::habitatmesh::{creature_lift, wall_height, FLOOR_Z};
use dfcore::silk::{Anchor, Node};
use dfcore::{Habitat, PropKind, Vec2, Weaver as Species, WeaverBody};

pub fn locate(h: &Habitat, species: Species, p: Vec2, support: Option<i64>) -> [f32; 3] {
    let lo = h.region.min();
    let hi = h.region.max();
    let height = wall_height(h.kind);
    let lift = creature_lift(h.kind, 0.0);
    let v = ((p.y - lo.y) / h.region.size.1).clamp(0.0, 1.0);
    // Smooth depth keeps neighboring retreat knots together.
    let hash = ((p.x * 0.017 + p.y * 0.013).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
    let mut q = match species {
        Species::Araneus => [
            p.x,
            hi.y - h.region.size.1 * 0.16,
            FLOOR_Z + 3.0 + v * (height - 7.0),
        ],
        Species::Parasteatoda => [
            p.x,
            hi.y - h.region.size.1 * (0.06 + 0.32 * hash),
            FLOOR_Z + 3.0 + v * (height - 7.0),
        ],
        Species::Agelenopsis => [p.x, p.y, FLOOR_Z + 7.0 + 4.0 * hash],
    };
    if let Some(id) = support {
        if id == dfcore::anchors::WALLS {
            let distances = [
                (p.x - lo.x, 0),
                (hi.x - p.x, 1),
                (p.y - lo.y, 2),
                (hi.y - p.y, 3),
            ];
            match distances
                .into_iter()
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .unwrap()
                .1
            {
                0 => q[0] = lo.x + 0.5,
                1 => q[0] = hi.x - 0.5,
                2 if species != Species::Agelenopsis => q[2] = FLOOR_Z + 0.5,
                3 if species != Species::Agelenopsis => q[2] = FLOOR_Z + height - 0.5,
                2 => q[1] = lo.y + 0.5,
                _ => q[1] = hi.y - 0.5,
            }
        } else if id <= -2 {
            if let Some(prop) = h.props.get((-id - 2) as usize) {
                // These attachment surfaces share the vivarium furniture's dimensions.
                match prop.kind {
                    PropKind::Bark => {
                        let z = if species == Species::Agelenopsis {
                            0.12
                        } else {
                            0.75 + hash * 0.25
                        };
                        let foot = prop.pos.y.min(hi.y - 6.0);
                        q = [
                            p.x.clamp(
                                (prop.pos.x - prop.radius * 1.5).max(lo.x + 2.0),
                                (prop.pos.x + prop.radius * 1.5).min(hi.x - 2.0),
                            ),
                            foot + (hi.y - 1.5 - foot) * z - 0.4,
                            FLOOR_Z + 1.0 + (height * 0.62 - 1.0) * z,
                        ];
                    }
                    PropKind::Twig => {
                        let root = h.region.clamp_inside(prop.pos, 8.0);
                        let corner = if root.x < h.region.center.x {
                            lo.x
                        } else {
                            hi.x
                        };
                        let tip = h.region.clamp_inside(
                            Vec2::new(root.x + (corner - root.x) * 0.55, hi.y - 12.0),
                            10.0,
                        );
                        let t = if species == Species::Agelenopsis {
                            0.1
                        } else {
                            0.45 + hash * 0.5
                        };
                        q = [
                            root.x + (tip.x - root.x) * t,
                            root.y + (tip.y - root.y) * t - 3.4,
                            FLOOR_Z + 3.6 + (height * 0.72 - 3.6) * t,
                        ];
                    }
                    PropKind::Pebble => {
                        q = [prop.pos.x, prop.pos.y, FLOOR_Z + 1.0 + prop.radius * 0.62]
                    }
                    _ => q = [prop.pos.x, prop.pos.y, FLOOR_Z + prop.radius * 0.5],
                }
            }
        }
    }
    q[2] -= lift;
    if support.is_some() {
        crate::webcontact::project(h, q)
    } else {
        q
    }
}

pub fn step(w: &mut WeaverBody, habitat: Option<&Habitat>, dt: f32) {
    if dt > 0.0 {
        let weight = if habitat.is_some() {
            [0.0, 0.0, -12.0]
        } else {
            [0.0, -12.0, 0.0]
        };
        if let Some(p) = w.spatial_pos {
            w.silk.load_at(p, weight, 12.0);
        }
        let prey: Vec<_> = w
            .prey
            .iter()
            .filter(|b| b.stuck)
            .filter_map(|b| w.silk.spatial_at(b.pos).map(|p| (p, b.struggle)))
            .collect();
        for (p, struggle) in prey {
            w.silk.load_at(
                p,
                std::array::from_fn(|k| weight[k] * (0.3 + struggle * 0.2)),
                8.0,
            );
        }
    }
    let species = w.species;
    w.spatial_vertical = habitat.is_some() && species != Species::Agelenopsis;
    w.spatial_habitat = habitat.is_some();
    let floor = habitat.map_or(-1000.0, |h| FLOOR_Z - creature_lift(h.kind, 0.0) + 0.4);
    let map = |n: &Node| {
        habitat.map_or([n.pos.x, n.pos.y, 3.0], |h| {
            locate(
                h,
                species,
                n.pos,
                if n.anchor == Anchor::Fixed {
                    n.on
                } else {
                    None
                },
            )
        })
    };
    // A gumfoot is a drop to the substrate beneath its upper junction.
    // Give its floor pin the same depth instead of a separately scattered one.
    let mut feet = Vec::new();
    if habitat.is_some() {
        for t in &w.silk.threads {
            if t.kind == dfcore::ThreadKind::Gumfoot {
                let (a, b) = (w.silk.nodes[t.a], w.silk.nodes[t.b]);
                for (node, top) in [(a, b), (b, a)] {
                    let lower = map(&node);
                    if node.anchor == Anchor::Fixed && lower[2] <= floor + 1.0 {
                        let upper = top.spatial.map_or_else(|| map(&top), |s| s.rest);
                        feet.push((node.pos, [upper[0], upper[1], lower[2]]));
                    }
                }
            }
        }
    }
    w.silk.step_spatial(
        dt,
        if habitat.is_some() {
            [0.0, 0.0, -28.0]
        } else {
            [0.0, -8.0, 0.0]
        },
        floor,
        |n| {
            feet.iter()
                .find(|(p, _)| *p == n.pos && n.anchor == Anchor::Fixed)
                .map_or_else(|| map(n), |(_, p)| *p)
        },
    );
    // Display/enclosure boundaries also catch loose silk.
    let boundary = w
        .anchors()
        .structures
        .iter()
        .find(|s| s.id == dfcore::anchors::WALLS || s.id == dfcore::anchors::SCREEN)
        .copied();
    if let Some(boundary) = boundary {
        for n in &mut w.silk.nodes {
            let s = n.spatial.as_mut().unwrap();
            for (k, lo, hi) in [
                (0, boundary.lo.x, boundary.hi.x),
                (1, boundary.lo.y, boundary.hi.y),
            ] {
                let bounded = s.pos[k].clamp(lo, hi);
                if bounded != s.pos[k] {
                    s.pos[k] = bounded;
                    s.velocity[k] = 0.0;
                }
            }
        }
    }
    if dt > 0.0 {
        if let Some(h) = habitat {
            crate::webcontact::strands(h, &mut w.silk);
            for n in &mut w.silk.nodes {
                if n.anchor == Anchor::Free {
                    let s = n.spatial.as_mut().unwrap();
                    let contact = crate::webcontact::project(h, s.pos);
                    if contact != s.pos {
                        s.pos = contact;
                        s.velocity = [0.0; 3];
                    }
                }
            }
        }
        if w.silk.break_overstrained() > 0 {
            w.physical_damage();
        }
        crate::webtravel::advance(w, habitat, dt, floor);
    } else if w.spatial_pos.is_none() {
        w.spatial_pos =
            Some(habitat.map_or([w.pos.x, w.pos.y, 3.0], |_| [w.pos.x, w.pos.y, floor + 2.0]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn construction_and_physics_run_together_without_nonfinite_silk() {
        let h = Habitat::new(
            dfcore::HabitatKind::Vivarium,
            dfcore::Region::centered((400.0, 400.0)),
            12,
        );
        for species in [
            Species::Araneus,
            Species::Parasteatoda,
            Species::Agelenopsis,
        ] {
            let anchors = dfcore::Anchors::habitat(&h);
            let mut rng = dfcore::rng::Pcg32::new(12);
            let mut w = WeaverBody::new(
                species,
                Vec2::ZERO,
                12,
                dfcore::weaver::program_for(species, &anchors, &mut rng),
            );
            w.habitat_world = Some(anchors);
            for _ in 0..24_000 {
                w.update(0.05, h.region, None, None);
                step(&mut w, Some(&h), 0.05);
                if w.web_complete() {
                    break;
                }
            }
            assert!(
                w.web_complete(),
                "{species:?} stalled in {}",
                w.program.stage()
            );
            assert!(w.silk.nodes.iter().all(|n| n
                .spatial
                .unwrap()
                .pos
                .iter()
                .all(|v| v.is_finite())));
            assert!(w.silk.nodes.iter().any(|n| n.anchor == Anchor::Fixed));
        }
    }
    #[test]
    fn web_families_occupy_different_three_dimensional_surfaces() {
        let h = Habitat::new(
            dfcore::HabitatKind::Vivarium,
            dfcore::Region::centered((400.0, 400.0)),
            1,
        );
        let low = Vec2::new(0.0, -150.0);
        let high = Vec2::new(0.0, 150.0);
        let orb_lo = locate(&h, Species::Araneus, low, None);
        let orb_hi = locate(&h, Species::Araneus, high, None);
        assert!(orb_hi[2] - orb_lo[2] > wall_height(h.kind) * 0.5);
        assert_eq!(orb_hi[1], orb_lo[1]);
        let a = locate(&h, Species::Parasteatoda, Vec2::new(13.0, 71.0), None);
        let b = locate(&h, Species::Parasteatoda, Vec2::new(79.0, 71.0), None);
        assert!((a[1] - b[1]).abs() > 5.0, "tangle flattened into a plane");
        let sheet = locate(&h, Species::Agelenopsis, high, None);
        assert!(sheet[2] < 15.0 && sheet[1] == high.y);
    }
}

/// Carry the body (and its neurons) onto the web's physical surface.
pub fn place_body(mesh: &mut crate::mesh::Mesh, count: usize, w: &WeaverBody) {
    let Some(at) = w.spatial_pos else {
        return;
    };
    for v in mesh.verts.iter_mut().take(count) {
        let d = [v.pos[0] - w.pos.x, v.pos[1] - w.pos.y, v.pos[2]];
        if w.spatial_vertical {
            v.pos = [at[0] + d[0], at[1] - d[2], at[2] + d[1]];
            v.normal = [v.normal[0], -v.normal[2], v.normal[1]];
        } else {
            v.pos = [at[0] + d[0], at[1] + d[1], at[2] + d[2] - 3.0];
        }
    }
}
