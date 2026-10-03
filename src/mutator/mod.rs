//! Versioned mutation plans and bounded trusted execution.
mod facts;
mod plan;

pub use facts::*;
pub use plan::{apply_edit, inventory, plan, plan_with_guard, validate_mutant};

pub(crate) use plan::plan_with_guard as plan_guarded;
mod config;
mod result;
mod state;
mod storage;
mod workspace;
pub use config::{
    DependencyCopy, ExecutionConfig, ExecutionLimits, MutatorConfig, ReusePolicy, StateConfig,
    WorkspaceConfig,
};
pub use result::{
    BaselineOutcome, BaselineResult, ExecutionPhase, ExecutionProblem, ExecutionProblemKind,
    ExecutionSummary, MutationOutcome, MutationReport, MutationResult, MutationSummary,
    SourceContext, TestCompletionReason, TestCounts, TestExecutionRequest, TestExecutionResult,
    TestFailure, TestFailureKind, WorkspaceSummary,
};

mod run;
pub use run::{CancellationToken, run};
