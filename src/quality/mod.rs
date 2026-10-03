//! Shared source selection and evidence contracts for TypeScript quality analysis.
mod config;
mod facts;

pub use config::{
    AnalysisLimits, MAX_CONFIGURATION_BYTES, MIN_REPORT_BYTES, SourceSelection,
    validate_configuration, validate_report_size,
};
pub use facts::*;
