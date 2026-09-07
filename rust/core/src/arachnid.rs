//! The eight-leg rig every spider in the app walks on (WEB_PLAN.md §7).
//!
//! Extracted from the salticid's body so the web builders do not fork it. A
//! leg is an angle and a lift; the gait is an alternating tetrapod (legs 1 and
//! 3 on one side step with 2 and 4 on the other) driven by one phase scalar,
//! exactly as the fly's tripod is. What differs between species is the
//! geometry the shell draws around these numbers, never the numbers.
//!
//! Every branch here is the salticid's `update_legs` moved verbatim, and the
//! salticid's suites are byte-identical across the move — that is the test.

use crate::util::{clamp, smoothstep};

#[derive(Debug, Clone, Copy)]
pub struct SpiderLeg {
    /// +1 right, -1 left.
    pub side: f32,
    /// Front (0) to back (3).
    pub rank: usize,
    pub phase: f32,
    pub angle: f32,
    pub lift: f32,
}

/// Alternating tetrapod: left 1/3 and right 2/4 share a phase.
pub const LEG_PHASES: [f32; 8] = [0.0, 0.5, 0.5, 0.0, 0.0, 0.5, 0.5, 0.0];

pub fn new_legs() -> [SpiderLeg; 8] {
    std::array::from_fn(|i| SpiderLeg {
        side: if i % 2 == 0 { -1.0 } else { 1.0 },
        rank: i / 2,
        phase: LEG_PHASES[i],
        angle: 0.0,
        lift: 0.0,
    })
}

/// What the legs are doing this frame. The body maps its state onto one of
/// these; the rig does not know what a state is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LegMode {
    /// Stepping at this absolute ground speed, reversed when backing up.
    Walk { speed: f32, backward: bool },
    /// Front legs and palps drawn through the chelicerae.
    Groom { time: f32 },
    /// Hind legs extended to launch, front legs reaching forward.
    Launch,
    /// Hanging on a line, legs held in.
    Hang,
    /// Everything relaxes to neutral.
    Rest,
}

/// Advance the rig one frame. `gait_phase` is the shared locomotor phase the
/// body also reports as proprioception.
pub fn step_legs(legs: &mut [SpiderLeg; 8], gait_phase: &mut f32, dt: f32, mode: LegMode) {
    match mode {
        LegMode::Walk { speed: v, backward } => {
            let amp = clamp(0.18 + v * 0.0025, 0.18, 0.45);
            let stride = (2.0 * amp * 12.0).max(4.0);
            let freq = clamp(v / stride, 2.5, 9.0);
            *gait_phase = (*gait_phase + freq * dt) % 1.0;
            let stance_frac = 0.62;
            for leg in legs.iter_mut() {
                let p = (*gait_phase + leg.phase) % 1.0;
                if p < stance_frac {
                    leg.angle = amp * (1.0 - 2.0 * (p / stance_frac));
                    leg.lift = 0.0;
                } else {
                    let s = (p - stance_frac) / (1.0 - stance_frac);
                    leg.angle = -amp + 2.0 * amp * smoothstep(s);
                    leg.lift = (s * std::f32::consts::PI).sin() * 0.5;
                }
                if backward {
                    leg.angle = -leg.angle;
                }
            }
        }
        LegMode::Groom { time: t } => {
            for leg in legs.iter_mut() {
                if leg.rank == 0 {
                    leg.angle = 0.5 + 0.25 * (t * 18.0 + leg.side * 1.1).sin();
                    leg.lift = 0.5 + 0.15 * (t * 20.0).sin();
                } else {
                    leg.angle += (0.0 - leg.angle) * (8.0 * dt).min(1.0);
                    leg.lift += (0.0 - leg.lift) * (8.0 * dt).min(1.0);
                }
            }
        }
        LegMode::Launch => {
            for leg in legs.iter_mut() {
                let (a, l) = if leg.rank >= 2 { (-0.6, 0.2) } else { (0.5, 0.35) };
                leg.angle += (a - leg.angle) * (14.0 * dt).min(1.0);
                leg.lift += (l - leg.lift) * (14.0 * dt).min(1.0);
            }
        }
        LegMode::Hang => {
            for leg in legs.iter_mut() {
                leg.angle += (0.15 - leg.angle) * (6.0 * dt).min(1.0);
                leg.lift += (0.45 - leg.lift) * (6.0 * dt).min(1.0);
            }
        }
        LegMode::Rest => {
            for leg in legs.iter_mut() {
                leg.angle += (0.0 - leg.angle) * (10.0 * dt).min(1.0);
                leg.lift += (0.0 - leg.lift) * (10.0 * dt).min(1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rig_is_an_alternating_tetrapod() {
        let legs = new_legs();
        let phase = |side: f32, rank: usize| {
            legs.iter().find(|l| l.side == side && l.rank == rank).unwrap().phase
        };
        assert_eq!(phase(-1.0, 0), phase(-1.0, 2));
        assert_eq!(phase(-1.0, 0), phase(1.0, 1));
        assert_eq!(phase(-1.0, 0), phase(1.0, 3));
        assert_ne!(phase(-1.0, 0), phase(-1.0, 1));
    }

    #[test]
    fn walking_advances_the_phase_and_lifts_swinging_legs() {
        let mut legs = new_legs();
        let mut phase = 0.0;
        let mut lifted = false;
        for _ in 0..30 {
            step_legs(&mut legs, &mut phase, 1.0 / 60.0, LegMode::Walk { speed: 40.0, backward: false });
            lifted |= legs.iter().any(|l| l.lift > 0.1);
        }
        assert!(phase > 0.0 && phase < 1.0);
        assert!(lifted, "some leg must swing");
    }

    #[test]
    fn rest_relaxes_everything_to_neutral() {
        let mut legs = new_legs();
        let mut phase = 0.3;
        step_legs(&mut legs, &mut phase, 1.0 / 60.0, LegMode::Launch);
        for _ in 0..120 {
            step_legs(&mut legs, &mut phase, 1.0 / 60.0, LegMode::Rest);
        }
        assert!(legs.iter().all(|l| l.angle.abs() < 1e-3 && l.lift.abs() < 1e-3));
        assert_eq!(phase, 0.3, "resting does not advance the gait");
    }
}
