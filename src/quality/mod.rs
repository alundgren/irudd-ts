//! Shared source selection and evidence contracts for TypeScript quality analysis.
mod config;
mod facts;
pub(crate) mod functions;
pub(crate) mod load;
pub(crate) mod syntax;
pub(crate) mod workers;

pub(crate) use config::encoded_size;
pub use config::{
    AnalysisLimits, MAX_CONFIGURATION_BYTES, MIN_REPORT_BYTES, SourceSelection,
    validate_configuration, validate_report_size,
};
pub use facts::*;
