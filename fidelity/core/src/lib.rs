//! Fidelity measurement between a real desktop application and its ComputerWorld
//! reimplementation, without a model in the loop.
//!
//! The crate is pure: it defines the shared trace ontology ([`schema`]), turns each
//! side's emission into one comparable form ([`normalize`]), diffs semantic trees with
//! an edit script ([`treediff`]), measures pixel crops ([`pixels`]), runs the divergence
//! cascade over a step or a whole trajectory ([`cascade`]), tracks coverage and sampling
//! priority ([`coverage`]), fits tolerances to human verdicts ([`tune`]) and fixes the
//! on-disk layout ([`store`]). All host I/O is in the `cwfid` tool.
pub mod cascade;
pub mod coverage;
pub mod hash;
pub mod normalize;
pub mod pixels;
pub mod schema;
pub mod store;
pub mod text;
pub mod treediff;
pub mod tune;

pub use cascade::{
    compare_step, compare_trajectories, CompareConfig, Finding, FindingKind, StepComparison,
    TrajectoryComparison,
};
pub use normalize::{normalize, NormalizeOptions};
pub use pixels::Image;
pub use schema::*;
