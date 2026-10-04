//! Structural review evidence for selected TypeScript and TSX functions.
mod analyze;
mod config;
mod facts;
mod groups;
mod normalize;
pub use analyze::{analyze, analyze_cached};
pub use config::*;
pub use facts::{
    CloneGroup, ClonePair, DryerReport, FunctionExclusion, FunctionFacts, FunctionInventory,
    SimilarityValues,
};
pub use normalize::extract;
