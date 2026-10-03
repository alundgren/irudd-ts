//! Optional compiler analysis with its own versioned facts contract.
mod config;
mod facts;
mod inventory;
mod rules;
mod validation;

pub use config::{CompilerContext, MemberRule, MemberRuleKind, SemanticConfig};
pub use facts::*;
pub use inventory::{hash, inventory, request};
pub use rules::{SemanticRule, check};
pub use validation::validate;

use crate::facts::ProjectFacts;
use anyhow::{Context, Result};
use std::path::Path;

pub fn analyze(
    project: &ProjectFacts,
    config: &SemanticConfig,
    cwd: &Path,
) -> Result<SemanticFacts> {
    let request = request(project, config)?;
    crate::subprocess::run(&request, &config.provider, cwd, |output| {
        let facts: SemanticFacts =
            serde_json::from_slice(output).context("expected semantic facts JSON")?;
        validate(&request, &facts)?;
        Ok(facts)
    })
}
