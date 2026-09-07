//! The enclosure, drawn as the container each animal is actually kept in.
//!
//! Under the tilted habitat camera the enclosure has real geometry: a floor, a
//! back wall and two side walls that rise out of the ground plane, and a sheet
//! of front glass between the viewer and everything else. The creature is
//! inside that box, not on top of a picture of one. What the box is made of,
//! how tall it stands and what is in it is decided per [`HabitatKind`]:
//!
//! | kind | what it is | module |
//! |---|---|---|
//! | `FlyCage` | a framed mesh rearing cage with a dish of medium, fruit and a spent vial | [`flycage`] |
//! | `Vivarium` | a tall cross-ventilated acrylic vivarium with cork bark, a twig and a silk retreat | [`vivarium`] |
//! | `AgarPlate` | a shallow square dish of agar with a bacterial lawn, worm tracks and a label | [`plate`] |
//! | `Pond` | a stone-coped pond with a dark liner, cobbles, lily pads and floating pellets | [`pond`] |
//! | `SandTerrarium` | a low glass tank with a deep bed of sand, a cork hide and a water dish | [`terrarium`] |
//!
//! ## Draw order is index order
//!
//! The enclosure and the creature share one pipeline and one vertex format, so
//! they reach the GPU as a single buffer and a single draw. That means the
//! order of the indices *is* the order the triangles are drawn, which is what
//! makes translucent glass work: [`build_back`] is written first, the creature
//! next, and [`build_front`] last — so the front pane blends over the animal
//! behind it, and the back wall never blends over anything it should be behind.
//! The same rule puts a lily pad in the front mesh: the koi rises to just under
//! the surface, and the pad has to blend over it.
//!
//! The rule also decides which *side wall* goes where. The camera is yawed, so
//! it looks in over one side wall as well as over the front: that wall is
//! between the viewer and the interior, and if it were written in the back
//! pass its depth would hide everything behind it — a twig against it, the
//! creature walking past it. [`Ctx::near_side`] is that wall, drawn in front;
//! [`Ctx::far_walls`] are the other two, drawn behind. Which is which is read
//! off the view direction, not assumed.
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

mod flycage;
mod plate;
mod pond;
mod prims;
pub(crate) mod terrarium;
mod vivarium;

use dfcore::{Habitat, HabitatKind, Vec2};

use crate::mesh::Mesh;
use prims::*;

/// The enclosure floor, in scene z. Below the creatures' own geometry: a
/// body's nominal plane is z = 0, but the fly's legs and wings reach about 4.7
/// units under it, and a floor drawn above that would have the fly standing
/// knee-deep in gravel. `runtime.rs` measures all four and asserts the
/// clearance rather than trusting this number.
pub const FLOOR_Z: f32 = -9.0;

/// How high the walls stand above the floor. This is where the enclosures
/// differ most, and it is a fact about the animal rather than a style choice:
///
/// - a rearing cage needs headroom for an insect that flies;
/// - a jumping spider's vivarium is *tall* — the animal lives on the walls and
///   builds its retreat in the top corner, so the box is higher than it is
///   wide;
/// - a petri dish is a centimetre deep; a worm lives on a film of gel, not in
///   a room;
/// - a raised pond is knee-high stone, and its depth is the water column the
///   fish moves through;
/// - a snake's terrarium is long and low: nothing in it climbs, and what
///   height it has is mostly the sand.
pub fn wall_height(kind: HabitatKind) -> f32 {
    match kind {
        HabitatKind::FlyCage => 170.0,
        HabitatKind::Vivarium => 230.0,
        HabitatKind::AgarPlate => 18.0,
        HabitatKind::Pond => 92.0,
        HabitatKind::SandTerrarium => 110.0,
    }
}

/// The top of the box: the cut edge of the glass, the top bar of the frame,
/// the lip of the dish, the top of the coping. Placement asks for this rather
/// than adding `wall_height` at four call sites.
pub fn top_z(kind: HabitatKind) -> f32 {
    FLOOR_Z + wall_height(kind)
}

/// How much of the default footprint each enclosure wants, as (width, depth)
/// multipliers. The vivarium is narrow for its height, because that is the
/// shape of the thing; the pond is a little wider than a tank; the dish is
/// slightly smaller than a cage.
pub fn footprint(kind: HabitatKind) -> (f32, f32) {
    match kind {
        HabitatKind::FlyCage => (1.0, 1.0),
        HabitatKind::Vivarium => (0.74, 0.80),
        HabitatKind::AgarPlate => (0.90, 0.90),
        HabitatKind::Pond => (1.08, 1.0),
        HabitatKind::SandTerrarium => (1.10, 0.86),
    }
}

/// How deep the bed of sand in the terrarium is. The burrowers sit on top of
/// it and sink beneath it; the sandworm's ripple runs across it.
pub const SAND: f32 = 10.0;

/// How thick the layer of agar in the dish is. The worm is on top of it, and
/// the shadow plane is its surface, not the dish's bottom.
pub const AGAR: f32 = 7.0;

/// Where in an enclosure a creature's own origin should sit, given how high it
/// wants to be (0 = on the bottom, 1 = just under the surface).
///
/// Walkers get a small lift so their feet meet the substrate rather than
/// sinking into it — the fly's legs reach ~4.7 below its origin. The worm is
/// lifted onto the *agar*, not the floor of the dish. A swimmer gets the water
/// column, which is what turns the koi's `depth` from a scale trick into an
/// actual position: looking straight down there was no way to show a fish
/// rising, so it was faked with size.
pub fn creature_lift(kind: HabitatKind, hint: f32) -> f32 {
    match kind {
        HabitatKind::Pond => FLOOR_Z + 12.0 + hint.clamp(0.0, 1.0) * wall_height(kind) * 0.50,
        HabitatKind::AgarPlate => FLOOR_Z + AGAR + 1.2,
        HabitatKind::SandTerrarium => FLOOR_Z + SAND + 0.6,
        HabitatKind::FlyCage | HabitatKind::Vivarium => FLOOR_Z + 5.0,
    }
}

/// The plane the creature's contact shadow is flattened onto: the substrate,
/// or the surface of the agar.
pub fn shadow_z(kind: HabitatKind) -> f32 {
    match kind {
        HabitatKind::AgarPlate => FLOOR_Z + AGAR + 0.3,
        HabitatKind::SandTerrarium => FLOOR_Z + SAND + 0.3,
        _ => FLOOR_Z + 0.9,
    }
}

/// Everything the per-kind builders need, read once.
pub(crate) struct Ctx<'a> {
    pub h: &'a Habitat,
    pub lo: Vec2,
    pub hi: Vec2,
    pub top: f32,
    pub view: [f32; 3],
}

impl<'a> Ctx<'a> {
    fn new(h: &'a Habitat, view: [f32; 3]) -> Self {
        Ctx {
            h,
            lo: h.region.min(),
            hi: h.region.max(),
            top: top_z(h.kind),
            view,
        }
    }

    /// Scatter seeded from the tank's size: the same tank always scatters the
    /// same way, and a resize re-rolls it.
    pub fn scatter(&self) -> Scatter {
        Scatter::new(self.h.region.size.0 as i32 as u32)
    }

    /// Which side wall the camera looks in over: the one whose *outward*
    /// normal points toward the viewer. `true` for the −x wall.
    pub fn near_is_lo_x(&self) -> bool {
        self.view[0] < 0.0
    }

    /// The x of the near side wall, and its inward normal.
    pub fn near_side_x(&self) -> (f32, [f32; 3]) {
        if self.near_is_lo_x() {
            (self.lo.x, [1.0, 0.0, 0.0])
        } else {
            (self.hi.x, [-1.0, 0.0, 0.0])
        }
    }

    /// The x of the far side wall, and its inward normal.
    pub fn far_side_x(&self) -> (f32, [f32; 3]) {
        if self.near_is_lo_x() {
            (self.hi.x, [-1.0, 0.0, 0.0])
        } else {
            (self.lo.x, [1.0, 0.0, 0.0])
        }
    }

    fn side_wall(
        &self,
        out: &mut Mesh,
        x: f32,
        normal: [f32; 3],
        glass: [f32; 3],
        a0: f32,
        a1: f32,
    ) {
        let (lo, hi, top) = (self.lo, self.hi, self.top);
        let f = fresnel(normal, self.view);
        face(
            out,
            [
                [x, lo.y, FLOOR_Z],
                [x, hi.y, FLOOR_Z],
                [x, hi.y, top],
                [x, lo.y, top],
            ],
            normal,
            rgba(glass, a0 + a1 * f),
        );
    }

    /// The back wall and the far side wall of a glass box, seen from the
    /// inside, each brightened by how edge-on the camera sees it. These are
    /// behind everything and belong in the back pass.
    pub fn far_walls(&self, out: &mut Mesh, glass: [f32; 3], a0: f32, a1: f32) {
        let (lo, hi, top) = (self.lo, self.hi, self.top);
        let normal = [0.0, -1.0, 0.0];
        let f = fresnel(normal, self.view);
        face(
            out,
            [
                [lo.x, hi.y, FLOOR_Z],
                [hi.x, hi.y, FLOOR_Z],
                [hi.x, hi.y, top],
                [lo.x, hi.y, top],
            ],
            normal,
            rgba(glass, a0 + a1 * f),
        );
        let (x, n) = self.far_side_x();
        self.side_wall(out, x, n, glass, a0, a1);
    }

    /// The side wall between the camera and the interior. Belongs in the
    /// front pass with the front pane, for the same reason.
    pub fn near_side(&self, out: &mut Mesh, glass: [f32; 3], a0: f32, a1: f32) {
        let (x, n) = self.near_side_x();
        self.side_wall(out, x, n, glass, a0, a1);
    }

    /// The front pane: barely there face-on, catching the light at its edges.
    pub fn front_pane(&self, out: &mut Mesh, glass: [f32; 3], a0: f32, a1: f32) {
        let (lo, hi, top) = (self.lo, self.hi, self.top);
        let normal = [0.0, 1.0, 0.0];
        let f = fresnel(normal, self.view);
        face(
            out,
            [
                [lo.x, lo.y, FLOOR_Z],
                [hi.x, lo.y, FLOOR_Z],
                [hi.x, lo.y, top],
                [lo.x, lo.y, top],
            ],
            normal,
            rgba(glass, a0 + a1 * f),
        );
    }

    /// The cut top edge of all four walls, `t` wide, at the top of the box.
    pub fn rim(&self, out: &mut Mesh, t: f32, color: [f32; 4]) {
        let (lo, hi, top) = (self.lo, self.hi, self.top);
        for (a, b) in [
            (Vec2::new(lo.x, hi.y - t), Vec2::new(hi.x, hi.y)),
            (Vec2::new(lo.x, lo.y), Vec2::new(hi.x, lo.y + t)),
            (Vec2::new(lo.x, lo.y), Vec2::new(lo.x + t, hi.y)),
            (Vec2::new(hi.x - t, lo.y), Vec2::new(hi.x, hi.y)),
        ] {
            ground_face(out, a, b, top, color);
        }
    }
}

/// Everything behind the creature: floor, substrate, back and side walls, and
/// whatever stands on the floor. Write this into the buffer first.
pub fn build_back(out: &mut Mesh, h: &Habitat, view: [f32; 3]) {
    out.verts.clear();
    out.indices.clear();
    let c = Ctx::new(h, view);
    match h.kind {
        HabitatKind::FlyCage => flycage::back(out, &c),
        HabitatKind::Vivarium => vivarium::back(out, &c),
        HabitatKind::AgarPlate => plate::back(out, &c),
        HabitatKind::Pond => pond::back(out, &c),
        HabitatKind::SandTerrarium => terrarium::back(out, &c),
    }
}

/// Everything in front of the creature: the front pane or bars, the water
/// surface and what floats on it, and the rim. Write this into the buffer
/// *after* the creature so it blends over it.
///
/// While the user is repositioning the enclosure its top edge lights up — the
/// only feedback available, since it is being dragged by a held chord rather
/// than by a click it could highlight on press.
pub fn build_front(out: &mut Mesh, h: &Habitat, view: [f32; 3], grabbed: bool) {
    out.verts.clear();
    out.indices.clear();
    let c = Ctx::new(h, view);
    match h.kind {
        HabitatKind::FlyCage => flycage::front(out, &c, grabbed),
        HabitatKind::Vivarium => vivarium::front(out, &c, grabbed),
        HabitatKind::AgarPlate => plate::front(out, &c, grabbed),
        HabitatKind::Pond => pond::front(out, &c, grabbed),
        HabitatKind::SandTerrarium => terrarium::front(out, &c, grabbed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;
    use dfcore::{PropKind, Region};

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
    fn every_kind_produces_a_closed_mesh() {
        for kind in HabitatKind::ALL {
            let m = whole(&tank(kind));
            assert!(m.verts.len() > 400, "{kind:?}: only {} verts", m.verts.len());
            assert_eq!(m.indices.len() % 3, 0);
            assert!(
                m.indices.iter().all(|&i| (i as usize) < m.verts.len()),
                "{kind:?} produced out-of-range indices"
            );
            // And it stays a few thousand triangles: this is drawn every frame
            // on top of someone's desktop, and it is furniture.
            assert!(
                m.indices.len() / 3 < 20_000,
                "{kind:?} is {} triangles",
                m.indices.len() / 3
            );
        }
    }

    /// The enclosure must not spill over its own walls in the ground plane — a
    /// box drawn wider than the region the creature is confined to would be a
    /// lie about where the boundary is.
    #[test]
    fn the_drawing_stays_within_the_region_footprint() {
        for kind in HabitatKind::ALL {
            let h = tank(kind);
            let m = whole(&h);
            let (lo, hi) = bounds(&m);
            let (rlo, rhi) = (h.region.min(), h.region.max());
            assert!(
                lo[0] >= rlo.x - 0.01 && lo[1] >= rlo.y - 0.01,
                "{kind:?} spills past the near corner: mesh ({}, {}) vs region ({}, {})",
                lo[0],
                lo[1],
                rlo.x,
                rlo.y
            );
            assert!(
                hi[0] <= rhi.x + 0.01 && hi[1] <= rhi.y + 0.01,
                "{kind:?} spills past the far corner: mesh ({}, {}) vs region ({}, {})",
                hi[0],
                hi[1],
                rhi.x,
                rhi.y
            );
        }
    }

    /// The whole point of the tilt: the enclosure has height now — and each
    /// kind has *its* height. A vivarium towers over a petri dish.
    #[test]
    fn each_enclosure_is_a_box_of_its_declared_height() {
        for kind in HabitatKind::ALL {
            let m = whole(&tank(kind));
            let (lo, hi) = bounds(&m);
            assert!(
                (lo[2] - FLOOR_Z).abs() < 0.01,
                "{kind:?}: the floor is at {} not {FLOOR_Z}",
                lo[2]
            );
            assert!(
                (hi[2] - top_z(kind)).abs() < 0.01,
                "{kind:?}: the top is at {} not {}",
                hi[2],
                top_z(kind)
            );
        }
        assert!(wall_height(HabitatKind::Vivarium) > wall_height(HabitatKind::FlyCage));
        assert!(wall_height(HabitatKind::AgarPlate) < wall_height(HabitatKind::Pond) / 3.0);
        // And the walls occupy screen height the ground rectangle knows
        // nothing about — which is exactly what placement has to account for.
        let c = cam();
        let (flat_lo, flat_hi) = c.screen_offsets((310.0, 210.0), FLOOR_Z, FLOOR_Z);
        let (box_lo, box_hi) =
            c.screen_offsets((310.0, 210.0), FLOOR_Z, top_z(HabitatKind::FlyCage));
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
    /// vertex ties with the front pane's. What matters is the front against
    /// the middle of the tank, which is where the animal is.
    #[test]
    fn the_front_is_nearer_than_the_space_the_creature_lives_in() {
        for kind in HabitatKind::ALL {
            let h = tank(kind);
            let mut back = Mesh::default();
            let mut front = Mesh::default();
            build_back(&mut back, &h, view());
            build_front(&mut front, &h, view(), false);
            assert!(!front.indices.is_empty(), "{kind:?} has no front");

            let v = cam().view();
            let depth = |p: [f32; 3]| crate::math::transform_point(&v, p)[2];
            let mid_z = FLOOR_Z + wall_height(kind) / 2.0;
            let pane = depth([h.region.center.x, h.region.min().y, mid_z]);
            // (The pond has no pane; its water surface stops at the coping,
            // which is why the tolerance is a stone's width.)
            assert!(
                front
                    .verts
                    .iter()
                    .any(|p| (p.pos[1] - h.region.min().y).abs() < 20.0),
                "{kind:?}: the front mesh has nothing on the near edge"
            );
            // Versus the creature in the middle at the top of its range — the
            // nearest to the front it ever gets.
            let creature = depth([
                h.region.center.x,
                h.region.center.y,
                creature_lift(h.kind, 1.0),
            ]);
            assert!(
                pane > creature,
                "{kind:?}: the creature is in front of the glass: pane {pane:.1}, creature {creature:.1}"
            );
            // And the back wall must be behind that same point — measured at
            // the creature's own height, since the top of a tall wall leans
            // toward the camera in view depth and legitimately so.
            let wall = depth([
                h.region.center.x,
                h.region.max().y,
                creature_lift(h.kind, 1.0),
            ]);
            assert!(
                wall < creature,
                "{kind:?}: the back wall is in front of the creature: wall {wall:.1}, creature {creature:.1}"
            );
        }
    }

    /// The camera is yawed, so it looks in over one side wall as well as the
    /// front. Both of those are between the viewer and the animal and have to
    /// be in the front mesh; the other two have to be behind. A near wall in
    /// the back pass writes its depth first and hides everything behind it —
    /// which is how the vivarium's twig vanished the first time.
    #[test]
    fn the_walls_the_camera_looks_over_are_drawn_in_front() {
        let v = view();
        assert!(v[1] < 0.0, "the camera is expected in front of the tank");
        for kind in [
            HabitatKind::FlyCage,
            HabitatKind::Vivarium,
            HabitatKind::AgarPlate,
            HabitatKind::SandTerrarium,
        ] {
            let h = tank(kind);
            let mut back = Mesh::default();
            let mut front = Mesh::default();
            build_back(&mut back, &h, view());
            build_front(&mut front, &h, view(), false);
            let (lo, hi) = (h.region.min(), h.region.max());
            let (near_x, far_x) = if v[0] < 0.0 { (lo.x, hi.x) } else { (hi.x, lo.x) };
            // A wall is present in a mesh if one of its *triangles* lies at
            // that x and reaches from the floor to the top — a frame bar has
            // vertices at both, but no single triangle spans them.
            let spans = |m: &Mesh, x: f32| {
                m.indices.chunks(3).any(|t| {
                    let v: Vec<[f32; 3]> = t.iter().map(|&i| m.verts[i as usize].pos).collect();
                    // (A frame upright also has a floor-to-top face at that
                    // x; only a wall reaches the far edge as well.)
                    v.iter().all(|p| (p[0] - x).abs() < 0.01)
                        && v.iter().any(|p| (p[1] - lo.y).abs() < 0.01)
                        && v.iter().any(|p| (p[1] - hi.y).abs() < 0.01)
                        && v.iter().any(|p| (p[2] - FLOOR_Z).abs() < 0.01)
                        && v.iter().any(|p| (p[2] - top_z(kind)).abs() < 0.01)
                })
            };
            assert!(spans(&front, near_x), "{kind:?}: the near side wall is not in front");
            assert!(!spans(&back, near_x), "{kind:?}: the near side wall is also behind");
            assert!(spans(&back, far_x), "{kind:?}: the far side wall is not behind");
            assert!(!spans(&front, far_x), "{kind:?}: the far side wall is in front");
        }
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
        for kind in HabitatKind::ALL {
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
        let h = tank(HabitatKind::Vivarium);
        let i = h
            .props
            .iter()
            .position(|p| p.kind == PropKind::Plant)
            .unwrap();
        let mut only = h.clone();
        only.props = vec![h.props[i]];
        let mut m = Mesh::default();
        // Walls would dominate the bounds, so measure the prop alone.
        vivarium::prop(&mut m, &Ctx::new(&only, view()), &only.props[0]);
        let (_, hi) = bounds(&m);
        let height = hi[2] - FLOOR_Z;
        assert!(
            height > h.props[i].radius * 2.0,
            "the plant is only {height:.1} tall"
        );
        assert!(height < wall_height(h.kind), "the plant grows out of the tank");
    }

    /// The spider's furniture is *vertical* furniture: the bark and the twig
    /// have to climb most of the way up the tank, or it is a floor with a
    /// stick on it rather than an arboreal enclosure.
    #[test]
    fn the_vivarium_furnishes_its_height() {
        let h = tank(HabitatKind::Vivarium);
        for kind in [PropKind::Bark, PropKind::Twig] {
            let p = *h.props.iter().find(|p| p.kind == kind).unwrap();
            let mut only = h.clone();
            only.props = vec![p];
            let mut m = Mesh::default();
            vivarium::prop(&mut m, &Ctx::new(&only, view()), &p);
            let (_, hi) = bounds(&m);
            let reach = (hi[2] - FLOOR_Z) / wall_height(h.kind);
            assert!(reach > 0.5, "{kind:?} only reaches {:.0}% of the way up", reach * 100.0);
            assert!(reach <= 1.0, "{kind:?} pokes out of the tank");
        }
    }

    /// A stirred plant must actually move, or the cursor's only way of touching
    /// the tank produces no visible result.
    #[test]
    fn a_stirred_plant_moves() {
        let mut h = tank(HabitatKind::Vivarium);
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

    /// An eaten pellet stops being drawn — otherwise the fish appears to swim
    /// through food it has already taken.
    #[test]
    fn eaten_food_is_not_drawn() {
        let mut h = tank(HabitatKind::Pond);
        let i = h
            .props
            .iter()
            .position(|p| p.kind == PropKind::Food)
            .unwrap();
        let before = whole(&h).verts.len();
        h.props[i].respawn = 4.0;
        assert!(
            whole(&h).verts.len() < before,
            "the pellet is still on screen after being eaten"
        );
    }

    /// What floats has to be drawn *over* the fish, which means in the front
    /// mesh, at the surface — a lily pad on the bottom of the pond is a
    /// plate.
    #[test]
    fn lily_pads_float_on_the_surface_in_front_of_the_fish() {
        let h = tank(HabitatKind::Pond);
        let mut back = Mesh::default();
        let mut front = Mesh::default();
        build_back(&mut back, &h, view());
        build_front(&mut front, &h, view(), false);
        let pad = h.props.iter().find(|p| p.kind == PropKind::LilyPad).unwrap();
        let near = |m: &Mesh| {
            m.verts
                .iter()
                .filter(|v| {
                    (v.pos[0] - pad.pos.x).abs() < pad.radius + 1.0
                        && (v.pos[1] - pad.pos.y).abs() < pad.radius + 1.0
                        && v.pos[2] > FLOOR_Z + wall_height(h.kind) * 0.5
                })
                .count()
        };
        assert!(near(&front) > 10, "the pad is not in the front mesh");
        assert_eq!(near(&back), 0, "the pad is drawn behind the fish");
    }

    /// The enclosures have to look different from one another, or "the
    /// container each animal is kept in" is a claim with no rendering behind
    /// it. Compared by their floors, which are the largest surfaces in view.
    #[test]
    fn the_kinds_look_different() {
        let floors: Vec<[f32; 4]> = HabitatKind::ALL
            .iter()
            .map(|&k| whole(&tank(k)).verts[0].color)
            .collect();
        let n = HabitatKind::ALL.len();
        for i in 0..n {
            for j in (i + 1)..n {
                let d: f32 = (0..3).map(|c| (floors[i][c] - floors[j][c]).abs()).sum();
                assert!(
                    d > 0.15,
                    "{:?} and {:?} have the same floor: {:?} vs {:?}",
                    HabitatKind::ALL[i],
                    HabitatKind::ALL[j],
                    floors[i],
                    floors[j]
                );
            }
        }
        // And the pond is the wet one: blue-green, dark.
        let pond = floors[HabitatKind::ALL
            .iter()
            .position(|&k| k == HabitatKind::Pond)
            .unwrap()];
        assert!(pond[2] > pond[0], "the pond bottom is not blue: {pond:?}");
        // The plate is agar: warm amber.
        let plate = whole(&tank(HabitatKind::AgarPlate));
        let agar = plate
            .verts
            .iter()
            .find(|v| (v.pos[2] - (FLOOR_Z + AGAR)).abs() < 0.01)
            .expect("no agar surface");
        assert!(
            agar.color[0] > agar.color[2] + 0.2,
            "the agar is not amber: {:?}",
            agar.color
        );
        // The terrarium's sand is pale and warm, and its surface is where
        // the burrowers are lifted to.
        let terr = whole(&tank(HabitatKind::SandTerrarium));
        let sand = terr
            .verts
            .iter()
            .find(|v| (v.pos[2] - (FLOOR_Z + SAND)).abs() < 0.01)
            .expect("no sand surface");
        assert!(sand.color[0] > sand.color[2] + 0.15, "the sand is not warm: {:?}", sand.color);
        assert!(sand.color[0] + sand.color[1] + sand.color[2] > 1.8, "the sand is dark");
        assert!(creature_lift(HabitatKind::SandTerrarium, 0.0) > FLOOR_Z + SAND);
    }

    /// The hide is the one piece of furniture the snake goes to, and it has
    /// to be an arch it can get *under*: half a tube standing proud of the
    /// sand, not a log lying on top of it.
    #[test]
    fn the_hide_is_an_arch_on_the_sand() {
        let h = tank(HabitatKind::SandTerrarium);
        let p = *h.props.iter().find(|p| p.kind == PropKind::Hide).unwrap();
        let mut only = h.clone();
        only.props = vec![p];
        let mut m = Mesh::default();
        terrarium::prop_mesh(&mut m, &Ctx::new(&only, view()), &p);
        let (lo, hi) = bounds(&m);
        let sand = FLOOR_Z + SAND;
        assert!(hi[2] > sand + p.radius * 0.4, "the hide is flat: top at {}", hi[2]);
        assert!(lo[2] < sand, "the hide sits on the sand rather than in it");
        assert!(hi[2] < top_z(h.kind), "the hide pokes out of the tank");
    }
}
