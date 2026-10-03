use crate::quality::{AnalysisLimits, SourceSelection, validate_configuration};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const NORMALIZATION_VERSION: u32 = 1;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DryerConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub selection: SourceSelection,
    #[serde(default = "default_lines")]
    pub minimum_lines: usize,
    #[serde(default = "default_nodes")]
    pub minimum_nodes: usize,
    #[serde(default)]
    pub normalization: NormalizationOptions,
    #[serde(default = "default_threshold")]
    pub similarity_threshold: f64,
    #[serde(default)]
    pub limits: AnalysisLimits,
}
fn default_lines() -> usize {
    4
}
fn default_nodes() -> usize {
    20
}
fn default_threshold() -> f64 {
    0.82
}
impl Default for DryerConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            selection: SourceSelection::default(),
            minimum_lines: default_lines(),
            minimum_nodes: default_nodes(),
            normalization: NormalizationOptions::default(),
            similarity_threshold: default_threshold(),
            limits: AnalysisLimits::default(),
        }
    }
}
impl DryerConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == SCHEMA_VERSION,
            "unsupported dryer configuration schemaVersion"
        );
        ensure!(
            self.minimum_lines > 0
                && self.minimum_lines <= self.limits.max_file_bytes
                && self.minimum_nodes > 0
                && self.minimum_nodes <= self.limits.max_nodes_per_file,
            "dryer minimum candidate counts must be positive and within analysis limits"
        );
        ensure!(
            self.similarity_threshold.is_finite()
                && (0.0..=1.0).contains(&self.similarity_threshold),
            "dryer similarity threshold must be finite and between zero and one"
        );
        self.selection.validate()?;
        self.limits.validate()?;
        self.normalization.validate()?;
        validate_configuration(self)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct NormalizationOptions {
    pub normalization_version: u32,
    pub local_identifiers: LocalIdentifiers,
    pub properties: PropertyNames,
    pub literals: LiteralValues,
}
impl Default for NormalizationOptions {
    fn default() -> Self {
        Self {
            normalization_version: NORMALIZATION_VERSION,
            local_identifiers: LocalIdentifiers::Bindings,
            properties: PropertyNames::Preserve,
            literals: LiteralValues::Kind,
        }
    }
}
impl NormalizationOptions {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.normalization_version == NORMALIZATION_VERSION,
            "unsupported normalizationVersion"
        );
        validate_configuration(self)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LocalIdentifiers {
    Bindings,
    Erase,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyNames {
    Preserve,
    Erase,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LiteralValues {
    Kind,
    Value,
}
