//! Small shared math, ported from the free functions at the top of
//! `FlyModel.swift:14-22`.

#[inline]
pub fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    hi.min(lo.max(v))
}

#[inline]
pub fn hypot(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

/// Shortest signed angular difference from `from` to `to`, in (-pi, pi].
#[inline]
pub fn angle_diff(from: f32, to: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut d = (to - from) % tau;
    if d > std::f32::consts::PI {
        d -= tau;
    }
    if d < -std::f32::consts::PI {
        d += tau;
    }
    d
}

#[inline]
pub fn smoothstep(t: f32) -> f32 {
    let x = clamp(t, 0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };
    pub fn new(x: f32, y: f32) -> Self {
        Vec2 { x, y }
    }
    pub fn dist(self, o: Vec2) -> f32 {
        hypot(o.x - self.x, o.y - self.y)
    }
}

/// A walkable window top edge, in scene coordinates (origin at screen centre,
/// y up). Port of `Ledge` (Environment.swift:11). The platform layer produces
/// these; nothing below this point knows what a window is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ledge {
    pub y: f32,
    pub x0: f32,
    pub x1: f32,
    pub id: i64,
}

/// Drosophila circadian activity: morning and evening peaks, a midday siesta,
/// night quiescence. Port of `circadianActivity` (Environment.swift:72).
/// Returns a multiplier for the sim's baseline drive.
pub fn circadian_activity(hour: f32) -> f32 {
    const PTS: [(f32, f32); 10] = [
        (0.0, 0.25),
        (5.0, 0.25),
        (8.0, 1.0),
        (10.0, 1.0),
        (13.0, 0.55),
        (15.0, 0.55),
        (17.0, 1.0),
        (20.0, 1.0),
        (23.0, 0.3),
        (24.0, 0.25),
    ];
    for i in 0..PTS.len() - 1 {
        if hour >= PTS[i].0 && hour <= PTS[i + 1].0 {
            let t = (hour - PTS[i].0) / (PTS[i + 1].0 - PTS[i].0).max(0.001);
            return PTS[i].1 + (PTS[i + 1].1 - PTS[i].1) * t;
        }
    }
    0.25
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_diff_takes_the_short_way() {
        let eps = 1e-5;
        assert!((angle_diff(0.0, 0.5) - 0.5).abs() < eps);
        // 6.0 rad is nearly a full turn; the short way is backwards.
        assert!(angle_diff(0.0, 6.0) < 0.0);
        assert!((angle_diff(0.0, 6.0) + 0.283_185).abs() < 1e-4);
    }

    #[test]
    fn smoothstep_endpoints_and_midpoint() {
        assert_eq!(smoothstep(-1.0), 0.0);
        assert_eq!(smoothstep(0.0), 0.0);
        assert_eq!(smoothstep(1.0), 1.0);
        assert_eq!(smoothstep(2.0), 1.0);
        assert!((smoothstep(0.5) - 0.5).abs() < 1e-6);
    }

    /// The 17th behaviour check (main.swift:437) asserts exactly this shape.
    #[test]
    fn circadian_curve_has_night_dip_siesta_and_peaks() {
        let night = circadian_activity(3.0);
        let dawn = circadian_activity(9.0);
        let siesta = circadian_activity(14.0);
        let dusk = circadian_activity(18.0);
        assert!(night < 0.4, "night {night}");
        assert!(dawn > 0.9, "dawn {dawn}");
        assert!(siesta < 0.7 && siesta > 0.3, "siesta {siesta}");
        assert!(dusk > 0.9, "dusk {dusk}");
    }
}
