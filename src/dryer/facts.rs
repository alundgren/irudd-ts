use crate::{
    dryer::{DryerConfig, NormalizationOptions},
    quality::*,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FunctionFacts {
    pub function: FunctionLocation,
    pub nodes: usize,
    pub opaque_nodes: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FunctionExclusion {
    pub function: FunctionLocation,
    pub reason: String,
    pub nodes: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimilarityValues {
    pub set: f64,
    pub multiset: f64,
    pub weighted: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClonePair {
    pub left: FunctionLocation,
    pub right: FunctionLocation,
    pub similarity: SimilarityValues,
    pub exact_normalized_match: bool,
    pub left_opaque_nodes: usize,
    pub right_opaque_nodes: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloneGroup {
    pub members: Vec<FunctionLocation>,
    pub pair_indices: Vec<usize>,
    pub all_members_match: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DryerReport {
    pub schema_version: u32,
    pub normalization_version: u32,
    pub parser_version: String,
    pub root: String,
    pub configuration: DryerConfig,
    pub selection: SelectionReport,
    pub files: Vec<SourceFile>,
    pub complete: bool,
    pub functions: Vec<FunctionFacts>,
    pub excluded: Vec<FunctionExclusion>,
    pub pairs: Vec<ClonePair>,
    pub groups: Vec<CloneGroup>,
    pub groups_complete: bool,
    pub problems: Vec<AnalysisProblem>,
    pub omitted_evidence: OmittedEvidence,
    pub elapsed_ms: f64,
}

// Flat nodes contain exact scalar keys and ordered postorder child indices.
// These are implementation records, not part of the public JSON protocol.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodeKey {
    pub kind: String,
    pub scalar: String,
    pub children: Vec<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NormalizedFunction {
    pub facts: FunctionFacts,
    pub tree: Vec<NodeKey>,
}

#[derive(Debug, Clone)]
pub struct FunctionInventory {
    pub normalization: NormalizationOptions,
    pub functions: Vec<FunctionFacts>,
    pub excluded: Vec<FunctionExclusion>,
    pub complete: bool,
    pub problems: Vec<AnalysisProblem>,
    pub omitted_evidence: OmittedEvidence,
    pub(crate) normalized: Vec<NormalizedFunction>,
    pub(crate) nodes: usize,
}
