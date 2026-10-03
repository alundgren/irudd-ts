//! Versioned mutation plan contracts, independent of command execution.
mod facts;
mod plan;

pub use facts::*;
pub use plan::{apply_edit, inventory, plan, validate_mutant};

pub(crate) use plan::plan_guarded;
