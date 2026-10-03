use crate::{
    config::{Matcher, PluginConfig},
    semantic::SCHEMA_VERSION,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticConfig {
    pub schema_version: u32,
    pub provider: PluginConfig,
    pub contexts: Vec<CompilerContext>,
    pub rules: Vec<MemberRule>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompilerContext {
    pub id: String,
    pub tsconfig: String,
    pub files: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemberRule {
    pub id: String,
    pub kind: MemberRuleKind,
    pub files: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemberRuleKind {
    MissingMember,
}
impl SemanticConfig {
    pub fn read(path: &Path) -> Result<(Self, PathBuf)> {
        let path = path.canonicalize()?;
        let config: Self = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        config.validate()?;
        Ok((
            config,
            path.parent()
                .context("semantic config directory")?
                .to_owned(),
        ))
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION || self.contexts.is_empty() {
            bail!("unsupported or empty semantic configuration");
        }
        if self.provider.name.trim().is_empty()
            || self.provider.command.is_empty()
            || self.provider.command.iter().any(|s| s.trim().is_empty())
            || self.provider.timeout_ms == 0
        {
            bail!("invalid semantic provider command/deadline");
        }
        let mut ids = BTreeSet::new();
        for context in &self.contexts {
            if context.id.trim().is_empty()
                || !ids.insert(&context.id)
                || context.tsconfig.trim().is_empty()
                || context.files.is_empty()
            {
                bail!("invalid or duplicate compiler context");
            }
            Matcher::new(&context.files)?;
        }
        let mut ids = BTreeSet::new();
        for rule in &self.rules {
            if rule.id.trim().is_empty() || !ids.insert(&rule.id) || rule.files.is_empty() {
                bail!("invalid or duplicate semantic rule");
            }
            Matcher::new(&rule.files)?;
        }
        Ok(())
    }
}
