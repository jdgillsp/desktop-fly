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

/// How large the tank is drawn, as a multiple of the size the app picks for
/// the display. 1.0 is that default; the ends are where the tank stops being
/// worth having — too small to see the animal in, or so large it owns the
/// screen. `fit_size` still has the last word, so the top of this range simply
/// means "as big as the display allows".
pub const MIN_ZOOM: f32 = 0.45;
pub const MAX_ZOOM: f32 = 2.2;
/// Straight down is not available here: at pitch 0 the walls project to lines
/// and the box stops being a box (see `Tilted`). The far end stops short of
/// edge-on, where the floor collapses to a sliver.
pub const MIN_PITCH: f32 = 0.20;
pub const MAX_PITCH: f32 = 1.32;
/// Yaw is symmetric — turning the tank the other way shows the *other* side
/// wall, which is a real choice, not a mistake. Zero is allowed, and looks like
/// a flat elevation; it is the one setting where the tank reads as a backdrop.
pub const MAX_YAW: f32 = 1.15;

/// The part of the habitat view the user owns: how the camera is angled and how
/// large the tank is drawn. Split out from [`Camera`] because zoom is not a
/// camera property at all — the projection is orthographic and the tank is
/// *built bigger*, which is what makes the creature inside it keep its constant
/// apparent size (PORT_PLAN.md §8). A perspective dolly would have shrunk the
/// animal along with its tank.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HabitatView {
    pub pitch: f32,
    pub yaw: f32,
    pub zoom: f32,
}

impl Default for HabitatView {
    fn default() -> Self {
        HabitatView {
            pitch: DEFAULT_PITCH,
            yaw: DEFAULT_YAW,
            zoom: 1.0,
        }
    }
}

impl HabitatView {
    /// The camera this view implies. Always clamped on the way out, so no
    /// caller can produce a degenerate projection however it got its numbers —
    /// a restored settings file included.
    pub fn camera(&self) -> Camera {
        Camera::Tilted {
            pitch: self.pitch.clamp(MIN_PITCH, MAX_PITCH),
            yaw: self.yaw.clamp(-MAX_YAW, MAX_YAW),
        }
    }

    pub fn clamped_zoom(&self) -> f32 {
        self.zoom.clamp(MIN_ZOOM, MAX_ZOOM)
    }

    /// Fold the numbers back into range and report whether anything actually
    /// moved — the caller only rebuilds the tank when it did.
    pub fn adjust(&mut self, d_pitch: f32, d_yaw: f32, d_zoom: f32) -> bool {
        let before = *self;
        self.pitch = (self.pitch + d_pitch).clamp(MIN_PITCH, MAX_PITCH);
        self.yaw = (self.yaw + d_yaw).clamp(-MAX_YAW, MAX_YAW);
        // Zoom multiplies rather than adds: a fixed step would be a third of
        // the tank at the small end and a twentieth at the large one.
        self.zoom = (self.zoom * (1.0 + d_zoom)).clamp(MIN_ZOOM, MAX_ZOOM);
        *self != before
    }

    /// Is this the view the app ships with? The tray uses it to grey out
    /// "Reset View" rather than offering a no-op.
    pub fn is_default(&self) -> bool {
        *self == HabitatView::default()
    }

    pub fn describe(&self) -> String {
        format!(
            "tilt {:.0}deg, turn {:.0}deg, size {:.0}%",
            self.pitch.to_degrees(),
            self.yaw.to_degrees(),
            self.clamped_zoom() * 100.0
        )
    }
}

/// The habitat camera at its default angles. The app builds its camera from a
/// [`HabitatView`] the user owns; this is the shorthand the tests are written
/// against, and the fixed point they check the adjustable one against.
#[cfg(test)]
pub fn habitat() -> Camera {
    HabitatView::default().camera()
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

    /// The default view must reproduce the constants the tank was designed and
    /// verified against, or making the view adjustable would silently have
    /// moved everyone's tank.
    #[test]
    fn the_default_view_is_the_camera_the_tank_was_built_for() {
        let v = HabitatView::default();
        assert!(v.is_default());
        assert_eq!(v.clamped_zoom(), 1.0);
        assert_eq!(
            v.camera(),
            Camera::Tilted {
                pitch: DEFAULT_PITCH,
                yaw: DEFAULT_YAW
            }
        );
        assert_eq!(v.camera(), habitat());
    }

    /// However hard the user leans on a control, the projection has to stay one
    /// the rest of the app can use: never edge-on, never straight down (where
    /// the walls collapse to lines), never a tank of zero size.
    #[test]
    fn no_amount_of_adjustment_produces_a_degenerate_view() {
        for (dp, dy, dz) in [
            (10.0, 10.0, 10.0),
            (-10.0, -10.0, -10.0),
            (0.0, 0.0, -0.99),
            (1e9, -1e9, 1e9),
        ] {
            let mut v = HabitatView::default();
            // Repeatedly, because zoom is multiplicative and one step is not
            // enough to reach either end.
            for _ in 0..64 {
                v.adjust(dp, dy, dz);
            }
            assert!((MIN_PITCH..=MAX_PITCH).contains(&v.pitch), "pitch {}", v.pitch);
            assert!(v.yaw.abs() <= MAX_YAW, "yaw {}", v.yaw);
            assert!(
                (MIN_ZOOM..=MAX_ZOOM).contains(&v.clamped_zoom()),
                "zoom {}",
                v.zoom
            );
            // The two properties the tank depends on: the ground still has
            // screen area to un-project a cursor through, and height still
            // reads as height.
            let c = v.camera();
            let (lo, hi) = c.screen_offsets((200.0, 200.0), 0.0, 0.0);
            assert!(
                (hi.x - lo.x) > 1.0 && (hi.y - lo.y) > 1.0,
                "the floor collapsed at pitch {}",
                v.pitch
            );
            let floor = math::transform_point(&c.view(), [0.0, 0.0, 0.0]);
            let up = math::transform_point(&c.view(), [0.0, 0.0, 10.0]);
            assert!(up[1] > floor[1], "height stopped going up at pitch {}", v.pitch);
        }
    }

    /// `adjust` reports whether anything moved — the app only rebuilds the tank
    /// when it did, so a control pinned at its limit must not rebuild forever.
    #[test]
    fn adjusting_against_a_limit_reports_no_change() {
        let mut v = HabitatView::default();
        assert!(v.adjust(0.05, 0.0, 0.0), "a real nudge reported nothing");
        for _ in 0..200 {
            v.adjust(1.0, 1.0, 1.0);
        }
        assert!(
            !v.adjust(1.0, 1.0, 1.0),
            "pinned at the limit, another push still claimed to change something"
        );
        assert!(!v.is_default());
    }

    /// The ground round-trip is what keeps the cursor and the creature agreeing
    /// about where things are. It has to hold at every angle the user can
    /// reach, not just at the default the feature shipped with.
    #[test]
    fn the_ground_mapping_round_trips_at_every_reachable_angle() {
        for &pitch in &[MIN_PITCH, 0.5, DEFAULT_PITCH, 1.1, MAX_PITCH] {
            for &yaw in &[-MAX_YAW, -0.3, 0.0, DEFAULT_YAW, MAX_YAW] {
                let c = HabitatView { pitch, yaw, zoom: 1.0 }.camera();
                for p in [Vec2::new(0.0, 0.0), Vec2::new(310.0, -244.0), Vec2::new(-88.0, 402.0)] {
                    let back = c.unproject_ground(c.project_ground(p));
                    assert!(
                        (back.x - p.x).abs() < 1e-2 && (back.y - p.y).abs() < 1e-2,
                        "pitch {pitch}, yaw {yaw}: ({}, {}) round-tripped to ({}, {})",
                        p.x, p.y, back.x, back.y
                    );
                }
            }
        }
    }

    /// Zoom has to actually change the tank, and `fit_size` has to keep the
    /// biggest setting on the display — the reason zoom is applied before the
    /// fit rather than after it.
    #[test]
    fn zoom_grows_the_tank_but_never_off_the_display() {
        let display = (1920.0, 1080.0);
        let (lo, hi) = (-40.0, 320.0);
        let base = (700.0, 460.0);
        let mut last = 0.0;
        for &z in &[MIN_ZOOM, 0.7, 1.0, 1.5, MAX_ZOOM] {
            let c = HabitatView { pitch: DEFAULT_PITCH, yaw: DEFAULT_YAW, zoom: z }.camera();
            let want = (base.0 * z, base.1 * z);
            let size = c.fit_size(display, want, lo, hi, 24.0);
            assert!(size.0 >= last, "zoom {z} shrank the tank");
            last = size.0;
            let half = (size.0 / 2.0, size.1 / 2.0);
            let (olo, ohi) = c.screen_offsets(half, lo, hi);
            assert!(
                ohi.x - olo.x <= display.0 - 48.0 + 1.0 && ohi.y - olo.y <= display.1 - 48.0 + 1.0,
                "zoom {z} put a {:.0}x{:.0} tank on a {:.0}x{:.0} display",
                ohi.x - olo.x, ohi.y - olo.y, display.0, display.1
            );
        }
        // And the ends are genuinely different sizes, not a clamp doing nothing.
        let small = HabitatView { pitch: DEFAULT_PITCH, yaw: DEFAULT_YAW, zoom: MIN_ZOOM };
        assert!(small.clamped_zoom() < 0.5);
    }

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
