//! Versioned mutation plan contracts, independent of command execution.
mod facts;
mod plan;

pub use facts::*;
pub use plan::{apply_edit, inventory, plan, plan_with_guard, validate_mutant};

pub(crate) use plan::plan_with_guard as plan_guarded;
