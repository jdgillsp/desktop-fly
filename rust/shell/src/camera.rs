//! Where the camera is, and the one consequence that matters: whether scene
//! coordinates are still screen coordinates.
//!
//! The app has always looked straight down. That is not an aesthetic choice —
//! it is what makes the fly able to stand on the top edge of *your* browser
//! window. `ScreenSpace` maps a window's top edge to a scene y, the fly walks
//! to that y, and it lands on the real pixels of the real window because the
//! projection is identity in x and y. Tilt the camera and that registration is
//! gone: the fly would walk along a line that sits on nothing.
//!
//! Inside an enclosure none of that applies. The tank is a self-contained
//! world that is not registered to anything on the desktop, and window ledges
//! are already suppressed there (HABITAT_PLAN.md §2). So the tilted camera is
//! habitat-only, and [`Camera`] is the seam: free roam passes `TopDown` and is
//! bit-for-bit unchanged, habitat mode passes `Tilted`, and pointing free roam
//! at a tilted camera later is a one-line change plus a decision about ledges.
//!
//! The projection stays **orthographic**. A perspective camera would make the
//! creature's apparent size depend on where it stood, and constant apparent
//! size across displays is a decision the port already made (PORT_PLAN.md §8).

use dfcore::{Region, Vec2};

use crate::math::{self, Mat4};

/// How far the camera is pushed back before projecting, looking straight down.
/// Inherited from the Swift rig; the ortho near/far bracket it.
const EYE: f32 = 300.0;
/// And tilted. A tank several hundred units deep turns that ground depth into
/// view-space distance — the near corner of a 1200-unit floor swings ~470 units
/// toward the camera — so 300 puts it *behind* the eye and it silently vanishes.
/// Orthographic projection makes this free: moving the camera back changes
/// nothing but which depths are representable.
const EYE_TILTED: f32 = 1500.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Camera {
    /// Straight down. Scene x/y *are* screen x/y, so anything registered to the
    /// real desktop lines up.
    TopDown,
    /// Turned by `yaw` about the vertical, then tilted down from vertical by
    /// `pitch`. The diorama view.
    ///
    /// Pitch alone is not enough. A wall at constant x has no extent in screen
    /// x when the yaw is zero, so it projects to a *line*: the side panes go
    /// invisible and the tank reads as a backdrop with a floor rather than as a
    /// container. Yaw is what turns one side wall toward the viewer and makes
    /// the box a box.
    ///
    /// The cost is that the ground's axes are no longer the screen's, so
    /// "right on screen" is not "right in the tank" and every ground/screen
    /// conversion has to go through the matrix rather than a scalar.
    Tilted { pitch: f32, yaw: f32 },
}

/// The default tilt: enough that the walls of a tank have real presence and a
/// swimming fish visibly changes depth, not so much that the floor collapses to
/// a sliver and the creature spends its life edge-on.
pub const DEFAULT_PITCH: f32 = 0.90; // ~51.6 degrees off vertical
/// The default turn about the vertical. Enough to show a side wall and give the
/// box its corner; not so much that the tank becomes a lozenge.
pub const DEFAULT_YAW: f32 = 0.60; // ~34.4 degrees

/// The habitat camera.
pub fn habitat() -> Camera {
    Camera::Tilted {
        pitch: DEFAULT_PITCH,
        yaw: DEFAULT_YAW,
    }
}

impl Camera {
    /// World → view. `TopDown` is the original `translate(0, 0, -EYE)`, so the
    /// free-roam matrix is unchanged to the bit.
    pub fn view(&self) -> Mat4 {
        match self {
            Camera::TopDown => math::translate(0.0, 0.0, -EYE),
            // Turn first, then tilt. Negative pitch, so that +z (height) rises
            // *up* the screen rather than down it — getting that sign wrong
            // buries a flying fly instead of lifting it.
            Camera::Tilted { pitch, yaw } => math::mul(
                math::translate(0.0, 0.0, -EYE_TILTED),
                math::mul(math::rotate_x(-pitch), math::rotate_z(*yaw)),
            ),
        }
    }

    /// Unit vector from the scene toward the camera, in world space. The shader
    /// needs it for specular and rim light; it used to be hard-coded to +z,
    /// which is only right when looking straight down.
    ///
    /// Read straight off the view matrix rather than re-derived from the
    /// angles: the rotation part is orthonormal, so its inverse is its
    /// transpose, and the camera's +z axis in world space is simply the third
    /// row. One fewer place for a sign to be wrong.
    pub fn view_dir(&self) -> [f32; 3] {
        let m = self.view();
        [m[0][2], m[1][2], m[2][2]]
    }

    /// The far plane. A tilted camera turns the ground's depth into view-space
    /// distance, so a tank several hundred units deep needs more room than the
    /// original 600 — but `TopDown` keeps that exact value so nothing about the
    /// free-roam depth buffer changes.
    pub fn far(&self) -> f32 {
        match self {
            Camera::TopDown => 600.0,
            Camera::Tilted { .. } => 4000.0,
        }
    }

    /// Ground point → where it lands on screen, in scene units. Height is
    /// separate: this is the z = 0 plane the creature walks on.
    pub fn project_ground(&self, p: Vec2) -> Vec2 {
        let m = self.view();
        Vec2::new(
            m[0][0] * p.x + m[1][0] * p.y,
            m[0][1] * p.x + m[1][1] * p.y,
        )
    }

    /// The inverse: a screen position (already in scene units, via
    /// `ScreenSpace::to_scene`) → the ground point under it.
    ///
    /// This is what the cursor has to go through in habitat mode. Without it
    /// the creature reacts to a cursor that is nowhere near where the user sees
    /// it — and with yaw the error is a rotation, not just a stretch.
    pub fn unproject_ground(&self, screen: Vec2) -> Vec2 {
        let m = self.view();
        // The 2x2 block mapping ground (x, y) to screen (x, y).
        let (a, b, c, d) = (m[0][0], m[1][0], m[0][1], m[1][1]);
        let det = a * d - b * c;
        // Edge-on: the ground has no screen area and any answer is as good as
        // another. Guarding beats dividing by zero.
        if det.abs() < 1e-4 {
            return screen;
        }
        Vec2::new(
            (d * screen.x - b * screen.y) / det,
            (a * screen.y - c * screen.x) / det,
        )
    }

    /// The screen-space bounding box of a box standing on a ground rectangle of
    /// half-size `half`, spanning `z_lo..z_hi` — given *relative to where its
    /// ground centre projects*.
    ///
    /// Placement needs this and cannot shortcut it. The projection is linear,
    /// so the box around the corners is the box around the whole shape, but
    /// under yaw the eight corners no longer line up with the screen axes and
    /// there is no scalar that stands in for the answer.
    pub fn screen_offsets(&self, half: (f32, f32), z_lo: f32, z_hi: f32) -> (Vec2, Vec2) {
        let m = self.view();
        let mut lo = Vec2::new(f32::MAX, f32::MAX);
        let mut hi = Vec2::new(f32::MIN, f32::MIN);
        for &sx in &[-half.0, half.0] {
            for &sy in &[-half.1, half.1] {
                for &z in &[z_lo, z_hi] {
                    let p = math::transform_point(&m, [sx, sy, z]);
                    // Drop the translation: this is an offset, not a position.
                    let (x, y) = (p[0], p[1] - m[3][1]);
                    lo = Vec2::new(lo.x.min(x), lo.y.min(y));
                    hi = Vec2::new(hi.x.max(x), hi.y.max(y));
                }
            }
        }
        (lo, hi)
    }

    /// Put a box of ground size `size` in the lower-right of the display, as
    /// close to the corner as `margin` allows.
    pub fn place_lower_right(
        &self,
        display: (f32, f32),
        size: (f32, f32),
        z_lo: f32,
        z_hi: f32,
        margin: f32,
    ) -> Region {
        let half = (size.0 / 2.0, size.1 / 2.0);
        let (olo, ohi) = self.screen_offsets(half, z_lo, z_hi);
        // Where the ground centre has to *project* for the box to touch the
        // bottom-right corner, then work back to where that is on the ground.
        let target = Vec2::new(
            display.0 / 2.0 - margin - ohi.x,
            -display.1 / 2.0 + margin - olo.y,
        );
        Region::new(self.unproject_ground(target), size)
    }

    /// Keep a box on the display. The clamp is in **screen** terms: clamping
    /// the ground rectangle instead would let a tilted tank's walls climb off
    /// the top, because they occupy screen height the ground rectangle knows
    /// nothing about.
    pub fn clamp_on_screen(
        &self,
        region: Region,
        display: (f32, f32),
        z_lo: f32,
        z_hi: f32,
        margin: f32,
    ) -> Region {
        let half = (region.size.0 / 2.0, region.size.1 / 2.0);
        let (olo, ohi) = self.screen_offsets(half, z_lo, z_hi);
        let at = self.project_ground(region.center);
        let clamp = |v: f32, lo: f32, hi: f32| if lo > hi { (lo + hi) / 2.0 } else { v.clamp(lo, hi) };
        let target = Vec2::new(
            clamp(
                at.x,
                -display.0 / 2.0 + margin - olo.x,
                display.0 / 2.0 - margin - ohi.x,
            ),
            clamp(
                at.y,
                -display.1 / 2.0 + margin - olo.y,
                display.1 / 2.0 - margin - ohi.y,
            ),
        );
        Region::new(self.unproject_ground(target), region.size)
    }

    /// How large a ground rectangle may be and still fit on `display`, given a
    /// box standing `z_lo..z_hi` on it. Binary-searched rather than solved,
    /// because under yaw the screen extent mixes both ground axes.
    pub fn fit_size(
        &self,
        display: (f32, f32),
        want: (f32, f32),
        z_lo: f32,
        z_hi: f32,
        margin: f32,
    ) -> (f32, f32) {
        let fits = |k: f32| {
            let (lo, hi) = self.screen_offsets((want.0 * k / 2.0, want.1 * k / 2.0), z_lo, z_hi);
            hi.x - lo.x <= display.0 - 2.0 * margin && hi.y - lo.y <= display.1 - 2.0 * margin
        };
        if fits(1.0) {
            return want;
        }
        let (mut lo, mut hi) = (0.05f32, 1.0f32);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if fits(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        (want.0 * lo, want.1 * lo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Free roam must be untouched: same matrix, same view direction, same far
    /// plane, and the identity mapping that lets the fly stand on a real window.
    #[test]
    fn top_down_is_exactly_what_it_always_was() {
        let c = Camera::TopDown;
        assert_eq!(c.view(), math::translate(0.0, 0.0, -EYE));
        assert_eq!(c.view_dir(), [0.0, 0.0, 1.0]);
        assert_eq!(c.far(), 600.0);
        // Looking straight down, height is invisible: a raised point lands in
        // exactly the same place as one on the ground.
        let (lo, hi) = c.screen_offsets((100.0, 100.0), 0.0, 500.0);
        assert_eq!(hi.y - lo.y, 200.0);
        assert_eq!(hi.x - lo.x, 200.0);
        for p in [
            Vec2::new(0.0, 0.0),
            Vec2::new(413.0, -207.0),
            Vec2::new(-755.0, 491.0),
        ] {
            assert_eq!(c.project_ground(p), p);
            assert_eq!(c.unproject_ground(p), p);
        }
    }

    /// Project then unproject must land back where it started, or the cursor
    /// and the creature disagree about where things are.
    #[test]
    fn the_ground_mapping_round_trips() {
        let c = habitat();
        for p in [
            Vec2::new(0.0, 0.0),
            Vec2::new(300.0, 220.0),
            Vec2::new(-140.0, -388.0),
        ] {
            let back = c.unproject_ground(c.project_ground(p));
            assert!(
                (back.x - p.x).abs() < 1e-3 && (back.y - p.y).abs() < 1e-3,
                "({}, {}) round-tripped to ({}, {})",
                p.x,
                p.y,
                back.x,
                back.y
            );
        }
    }

    /// The two things the tilt is *for*: the ground foreshortens, and height
    /// becomes visible at all.
    #[test]
    fn tilting_foreshortens_the_ground_and_reveals_height() {
        let c = habitat();
        // The ground shrinks: a square patch of floor is wider than it is tall
        // on screen, which is what reads as "receding".
        let (glo, ghi) = c.screen_offsets((200.0, 200.0), 0.0, 0.0);
        let (gw, gh) = (ghi.x - glo.x, ghi.y - glo.y);
        assert!(
            gh < gw * 0.85,
            "the ground is not foreshortened: {gw:.0} wide by {gh:.0} tall"
        );
        // And height is worth something on screen at all, which is the whole
        // reason a fish's depth can now be seen.
        let (blo, bhi) = c.screen_offsets((200.0, 200.0), 0.0, 200.0);
        assert!(
            (bhi.y - blo.y) > gh + 100.0,
            "200 units of height only added {:.0} of screen",
            (bhi.y - blo.y) - gh
        );

        // A point one unit up must land higher on screen than the same point on
        // the floor. This is the sign error that would bury a flying fly.
        let floor = math::transform_point(&c.view(), [0.0, 0.0, 0.0]);
        let up = math::transform_point(&c.view(), [0.0, 0.0, 10.0]);
        assert!(
            up[1] > floor[1],
            "height goes down the screen: floor y {}, raised y {}",
            floor[1],
            up[1]
        );
        // And it must be *nearer* the camera, not further.
        assert!(up[2] > floor[2], "raising a point pushed it away");
    }

    /// Under the tilt, further "north" on the ground must be further away, so
    /// the depth buffer sorts the tank's back wall behind its front glass.
    #[test]
    fn the_far_side_of_the_ground_is_further_from_the_camera() {
        let c = habitat();
        let near = math::transform_point(&c.view(), [0.0, -200.0, 0.0]);
        let far = math::transform_point(&c.view(), [0.0, 200.0, 0.0]);
        assert!(
            near[2] > far[2],
            "the back of the tank is nearer than the front: near {}, far {}",
            near[2],
            far[2]
        );
    }

    /// Everything a large tank contains has to fit between the near and far
    /// planes, or geometry silently vanishes at the edges.
    #[test]
    fn a_large_tank_fits_inside_the_depth_range() {
        let c = habitat();
        let v = c.view();
        for &(y, z) in &[
            (-600.0, -40.0),
            (600.0, -40.0),
            (-600.0, 200.0),
            (600.0, 200.0),
        ] {
            let p = math::transform_point(&v, [0.0, y, z]);
            // View space looks down -z, so visible depth is -p[2] in [near, far].
            let d = -p[2];
            assert!(
                d > 1.0 && d < c.far(),
                "a tank corner at y={y}, z={z} sits at depth {d}, outside 1..{}",
                c.far()
            );
        }
    }
}
