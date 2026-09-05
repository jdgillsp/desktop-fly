//! DesktopFly core — the OS-free part.
//!
//! Connectome data, the 1 kHz LIF simulation, the rates→commands mapping, and
//! the body's behaviour and gait. No windowing, no rendering, no platform APIs;
//! this crate compiles and its test suites run identically on any target
//! (PORT_PLAN.md §3).
//!
//! The two suites in [`suites`] are the ground truth for the port, exactly as
//! `--simtest` and `--behaviortest` are for the Swift build. Per `CLAUDE.md`
//! they must be run after any change here.

pub mod body;
pub mod data;
pub mod env;
pub mod habituation;
pub mod lif;
pub mod rng;
pub mod signals;
pub mod suites;
pub mod util;

pub use body::{Fly, Pose, State, FLY_SCALE};
pub use data::{BrainData, CircuitFile};
pub use env::{EnvSnapshot, Rect, ScreenSpace, Senses};
pub use habituation::Habituation;
pub use lif::{LifParams, LifSim};
pub use signals::{BrainSignals, SignalBuilder};
pub use util::{circadian_activity, Ledge, Vec2};

/// Default seed for the deterministic PRNG. See `rng` for why the port seeds
/// explicitly where the Swift build did not.
pub const DEFAULT_SEED: u64 = rng::Pcg32::DEFAULT_SEED;
