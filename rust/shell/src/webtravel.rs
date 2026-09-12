//! Persistent physical construction travel. Chart crossings do not join paths.
use dfcore::silk::Anchor;
use dfcore::weaver::{Move, Navigation, Op, Waypoint, BUILD_SPEED};
use dfcore::{Habitat, Vec2, WeaverBody};
use std::collections::VecDeque;

pub fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f32>()).sqrt()
}
fn mix(a: [f32; 3], b: [f32; 3], u: f32) -> [f32; 3] {
    std::array::from_fn(|k| a[k] + (b[k] - a[k]) * u)
}

pub fn goal(w: &WeaverBody, h: Option<&Habitat>, m: &Move) -> [f32; 3] {
    let mut support = None;
    for op in &m.ops {
        match op {
            Op::PayOut(_) | Op::Attach(Anchor::Fixed) => {
                support = w.anchors().on(m.to, 18.0);
                if support.is_some() {
                    break;
                }
            }
            Op::StartLine { radius, .. } | Op::AttachNear(radius) => {
                if let Some(i) = w.silk.node_at(m.to, *radius) {
                    if let Some(p) = w.silk.nodes[i].spatial {
                        return p.pos;
                    }
                }
            }
            Op::StartOn {
                on: kind, radius, ..
            }
            | Op::AttachOn { kind, radius } => {
                if let Some((i, _)) = w.silk.thread_near(m.to, *kind, *radius) {
                    let t = w.silk.threads[i];
                    let a = w.silk.nodes[t.a];
                    let b = w.silk.nodes[t.b];
                    if let (Some(pa), Some(pb)) = (a.spatial, b.spatial) {
                        let d = Vec2::new(b.pos.x - a.pos.x, b.pos.y - a.pos.y);
                        let u = (((m.to.x - a.pos.x) * d.x + (m.to.y - a.pos.y) * d.y)
                            / (d.x * d.x + d.y * d.y).max(1e-8))
                        .clamp(0.0, 1.0);
                        return mix(pa.pos, pb.pos, u);
                    }
                }
            }
            _ => {}
        }
    }
    if support.is_none() && m.ops.iter().all(|o| matches!(o, Op::Release | Op::Nothing)) {
        if let Some(p) = w.silk.spatial_at(m.to) {
            return p;
        }
    }
    let mut target = h.map_or([m.to.x, m.to.y, 3.0], |h| {
        crate::webspace::locate(h, w.species, m.to, support)
    });
    if let Some(h) = h {
        let floor =
            crate::habitatmesh::FLOOR_Z - crate::habitatmesh::creature_lift(h.kind, 0.0) + 0.4;
        if w.silk.trailing_kind() == Some(dfcore::ThreadKind::Gumfoot)
            && m.ops
                .iter()
                .any(|op| matches!(op, Op::Attach(Anchor::Fixed)))
            && target[2] <= floor + 1.0
        {
            if let Some(p) = w
                .silk
                .trailing_anchor()
                .and_then(|p| w.silk.node_at(p, 0.01))
                .and_then(|i| w.silk.nodes[i].spatial)
            {
                target[0] = p.rest[0];
                target[1] = p.rest[1];
            }
        }
    }
    target
}

fn silk_route(w: &WeaverBody, from: [f32; 3], to: [f32; 3]) -> Option<VecDeque<Waypoint>> {
    let (a, u, da) = w.silk.physical_contact(from)?;
    let (b, v, db) = w.silk.physical_contact(to)?;
    if da > 12.0 || db > 1.0 {
        return None;
    }
    let ta = w.silk.threads[a];
    let tb = w.silk.threads[b];
    let at = |n: usize| w.silk.nodes[n].spatial.unwrap().pos;
    let mut route = VecDeque::from([Waypoint::Point(mix(at(ta.a), at(ta.b), u))]);
    if a == b {
        route.push_back(Waypoint::Point(to));
        return Some(route);
    }
    // Start and finish through actual graph endpoints, never through a visual crossing.
    let start = if u < 0.5 { ta.a } else { ta.b };
    let end = if v < 0.5 { tb.a } else { tb.b };
    let mut links = vec![Vec::new(); w.silk.nodes.len()];
    for t in &w.silk.threads {
        links[t.a].push(t.b);
        links[t.b].push(t.a);
    }
    let mut parent = vec![usize::MAX; links.len()];
    parent[start] = start;
    let mut queue = VecDeque::from([start]);
    while let Some(n) = queue.pop_front() {
        if n == end {
            break;
        }
        for &next in &links[n] {
            if parent[next] == usize::MAX {
                parent[next] = n;
                queue.push_back(next);
            }
        }
    }
    if parent[end] == usize::MAX {
        return None;
    }
    let mut nodes = vec![end];
    while *nodes.last().unwrap() != start {
        nodes.push(parent[*nodes.last().unwrap()]);
    }
    for n in nodes.into_iter().rev() {
        route.push_back(Waypoint::Node(n));
    }
    route.push_back(Waypoint::Point(to));
    Some(route)
}

fn surface_route(
    w: &WeaverBody,
    h: Option<&Habitat>,
    from: [f32; 3],
    to: [f32; 3],
    m: &Move,
    floor: f32,
) -> VecDeque<Waypoint> {
    let Some(h) = h else {
        return VecDeque::from([Waypoint::Point(to)]);
    };
    let lo = h.region.min();
    let hi = h.region.max();
    let ground = floor + 2.0;
    let top =
        crate::habitatmesh::top_z(h.kind) - crate::habitatmesh::creature_lift(h.kind, 0.0) - 0.5;
    let mut route = VecDeque::new();
    // Retreat to the substrate on a safety line before approaching a new surface.
    route.push_back(Waypoint::Point([from[0], from[1], ground]));
    if to[2] <= ground + 10.0 {
        route.push_back(Waypoint::Point(to));
        return route;
    }
    if let Some(id) = w.anchors().on(m.to, 18.0) {
        if id <= -2 {
            if let Some(p) = h.props.get((-id - 2) as usize) {
                let foot = if p.kind == dfcore::PropKind::Bark {
                    [to[0], p.pos.y.min(hi.y - 6.0), ground]
                } else {
                    [p.pos.x, p.pos.y, ground]
                };
                route.push_back(Waypoint::Point(foot));
                route.push_back(Waypoint::Point(to));
                return route;
            }
        }
    }
    let edges = [
        (to[0] - lo.x, [lo.x + 0.5, to[1], ground]),
        (hi.x - to[0], [hi.x - 0.5, to[1], ground]),
        (to[1] - lo.y, [to[0], lo.y + 0.5, ground]),
        (hi.y - to[1], [to[0], hi.y - 0.5, ground]),
    ];
    let (d, base) = edges
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap();
    route.push_back(Waypoint::Point(base));
    if d < 1.0 {
        route.push_back(Waypoint::Point(to));
    } else {
        // Climb the wall, traverse the lid, then abseil. No unsupported air walk.
        route.push_back(Waypoint::Point([base[0], base[1], top]));
        route.push_back(Waypoint::Point([to[0], to[1], top]));
        route.push_back(Waypoint::Point(to));
    }
    route
}

fn surface_contacts(h: &Habitat, from: [f32; 3], route: VecDeque<Waypoint>) -> VecDeque<Waypoint> {
    let mut out = VecDeque::new();
    let mut start = from;
    for waypoint in route {
        let Waypoint::Point(end) = waypoint else {
            out.push_back(waypoint);
            continue;
        };
        let steps = distance(start, end).ceil().max(1.0) as usize;
        for i in 1..=steps {
            let p = mix(start, end, i as f32 / steps as f32);
            out.push_back(Waypoint::Point(crate::webcontact::project(h, p)));
        }
        start = end;
    }
    out
}

pub fn advance(w: &mut WeaverBody, h: Option<&Habitat>, dt: f32, floor: f32) {
    let mut nav = w.navigation.take().unwrap_or_default();
    let mut at = w
        .spatial_pos
        .unwrap_or_else(|| h.map_or([w.pos.x, w.pos.y, 3.0], |_| [w.pos.x, w.pos.y, floor + 2.0]));
    let previous = at;
    let current = w.construction_move().cloned();
    if let Some(m) = current {
        let target = goal(w, h, &m);
        let topology = (w.silk.nodes.len(), w.silk.threads.len());
        if nav.goal.as_ref() != Some(&m) || nav.topology != topology {
            nav = Navigation::default();
            nav.goal = Some(m.clone());
            nav.topology = topology;
            if let Some(route) = silk_route(w, at, target) {
                nav.route = route;
                nav.mode = "silk";
            } else if distance(at, target) <= 12.0 {
                nav.route.push_back(Waypoint::Point(target));
                nav.mode = "reach";
            } else {
                nav.route = surface_route(w, h, at, target, &m, floor);
                if let Some(h) = h {
                    nav.route = surface_contacts(h, at, nav.route);
                }
                nav.mode = "surface";
            }
        }
        // Track the endpoint's motion, but retain the chosen route and strand contact.
        if let Some(Waypoint::Point(p)) = nav.route.back_mut() {
            *p = target;
        }
        if nav.route.is_empty() && distance(at, target) > 0.25 {
            nav.route.push_back(Waypoint::Point(target));
        }
        let mut budget = BUILD_SPEED * dt.clamp(0.0, 0.05);
        if !w.build_gate {
            budget = 0.0;
        }
        while let Some(point) = nav.route.front() {
            let next = match point {
                Waypoint::Node(i) => w
                    .silk
                    .nodes
                    .get(*i)
                    .and_then(|n| n.spatial)
                    .map(|s| s.pos)
                    .unwrap_or(at),
                Waypoint::Point(p) => *p,
            };
            let d = distance(at, next);
            if nav.mode == "surface"
                && next[2] < at[2] - 0.01
                && (next[0] - at[0]).abs() < 0.1
                && (next[1] - at[1]).abs() < 0.1
            {
                if nav.safety_anchor.is_none() {
                    nav.safety_anchor = Some(at);
                }
            } else if nav.mode == "surface" {
                nav.safety_anchor = None;
            }
            if d <= budget.max(0.001) {
                at = next;
                budget = (budget - d).max(0.0);
                nav.route.pop_front();
            } else {
                if budget > 0.0 {
                    at = mix(at, next, budget / d);
                }
                break;
            }
        }
        nav.ready = nav.route.is_empty() && distance(at, target) <= 0.25;
    }
    // During an attachment/dwell keep physical contact; do not re-project onto a crossing.
    w.spatial_pos = Some(at);
    w.spatial_vertical =
        h.is_some() && w.species != dfcore::Weaver::Agelenopsis && at[2] > floor + 8.0;
    w.navigation = Some(nav);
    w.physical_gait(std::array::from_fn(|k| at[k] - previous[k]), dt);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_visual_crossing_cannot_connect_disjoint_strands() {
        let region = dfcore::Region::centered((400.0, 400.0));
        let anchors = dfcore::Anchors::desktop(region, region, &[]);
        let mut rng = dfcore::rng::Pcg32::new(1);
        let species = dfcore::Weaver::Araneus;
        let mut w = WeaverBody::new(
            species,
            Vec2::ZERO,
            1,
            dfcore::weaver::program_for(species, &anchors, &mut rng),
        );
        w.silk
            .pay_out(Vec2::new(-30.0, 0.0), dfcore::ThreadKind::Frame);
        w.silk.attach(Vec2::new(30.0, 0.0), Anchor::Fixed);
        w.silk.release();
        w.silk
            .pay_out(Vec2::new(0.0, -30.0), dfcore::ThreadKind::Frame);
        w.silk.attach(Vec2::new(0.0, 30.0), Anchor::Fixed);
        w.silk.release();
        w.silk
            .step_spatial(0.0, [0.0; 3], 0.0, |n| [n.pos.x, n.pos.y, 10.0]);
        assert!(silk_route(&w, [-20.0, 0.0, 10.0], [0.0, 20.0, 10.0]).is_none());
        assert!(silk_route(&w, [-20.0, 0.0, 10.0], [20.0, 0.0, 10.0]).is_some());
    }
}
