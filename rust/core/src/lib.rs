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
pub mod creature;
pub mod data;
pub mod env;
pub mod graded;
pub mod habituation;
pub mod lif;
pub mod rng;
pub mod roles;
pub mod signals;
pub mod spider;
pub mod suites;
pub mod util;
pub mod worm;

pub use body::{Fly, Pose, State, FLY_SCALE};
pub use creature::{
    by_id, Body, CElegans, Connectome, Creature, Drosophila, DynamicsSpec, GradedParams, Origin,
    Provenance, Salticid, Sim, Substrate, World, CREATURE_IDS,
};
pub use data::{BrainData, CircuitFile};
pub use env::{EnvSnapshot, Rect, ScreenSpace, Senses};
pub use habituation::Habituation;
pub use graded::GradedSim;
pub use lif::{LifParams, LifSim};
pub use roles::{RoleManifest, drosophila};
pub use signals::{BrainSignals, GradedSignalBuilder, SignalBuilder};
pub use util::{circadian_activity, Ledge, Vec2};
pub use spider::{Bug, Spider, SpiderPose, SpiderState};
pub use worm::{Worm, WormState};

/// Default seed for the deterministic PRNG. See `rng` for why the port seeds
/// explicitly where the Swift build did not.
pub const DEFAULT_SEED: u64 = rng::Pcg32::DEFAULT_SEED;
