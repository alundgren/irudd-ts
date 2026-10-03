use crate::quality::{AnalysisLimits, SourceSelection};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_PATH_BYTES: usize = 4_096;

pub fn validate_relative_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.contains(['\\', ':'])
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        bail!("expected a bounded relative forward-slash path without parent traversal");
    }
    Ok(())
}

pub fn is_source_path(path: &str) -> bool {
    [".ts", ".tsx", ".mts", ".cts"]
        .iter()
        .any(|suffix| path.ends_with(suffix))
        && ![".d.ts", ".d.mts", ".d.cts"]
            .iter()
            .any(|suffix| path.ends_with(suffix))
}

pub fn validate_sha256(hash: &str) -> Result<()> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("expected a full lowercase SHA256 digest");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceLocation {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub end_line: usize,
}

impl SourceLocation {
    pub fn validate(&self) -> Result<()> {
        validate_relative_path(&self.file)?;
        if !is_source_path(&self.file)
            || self.start >= self.end
            || self.line == 0
            || self.end_line < self.line
        {
            bail!("invalid TypeScript source location or exclusive byte range");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceFile {
    pub path: String,
    pub bytes: usize,
    pub sha256: String,
}

impl SourceFile {
    pub fn validate(&self) -> Result<()> {
        validate_relative_path(&self.path)?;
        if !is_source_path(&self.path) {
            bail!("selected source must be a non-declaration TS, TSX, MTS, or CTS file");
        }
        validate_sha256(&self.sha256)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FunctionLocation {
    pub name: String,
    pub kind: FunctionKind,
    pub location: SourceLocation,
    pub name_truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FunctionKind {
    FunctionDeclaration,
    VariableFunction,
    VariableArrow,
    Method,
    FieldFunction,
    FieldArrow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisProblem {
    pub kind: ProblemKind,
    pub file: String,
    pub offset: usize,
    pub message: String,
    pub message_truncated: bool,
    pub limit: Option<AnalysisLimitKind>,
}

impl AnalysisProblem {
    pub fn validate(&self, limits: &AnalysisLimits) -> Result<()> {
        if self.file != "." {
            validate_relative_path(&self.file)?;
        }
        if self.message.is_empty() || self.message.len() > limits.max_problem_bytes {
            bail!("problem message must be nonempty and within its evidence byte limit");
        }
        match (self.kind, self.limit) {
            (
                ProblemKind::SourceComplexityLimit,
                Some(AnalysisLimitKind::RawUnits | AnalysisLimitKind::DelimiterDepth),
            )
            | (ProblemKind::AnalysisLimit, Some(_))
            | (ProblemKind::ReportLimit, Some(AnalysisLimitKind::ReportBytes)) => Ok(()),
            (
                ProblemKind::SourceComplexityLimit
                | ProblemKind::AnalysisLimit
                | ProblemKind::ReportLimit,
                _,
            ) => bail!("limit problems require a matching typed limit"),
            (_, None) => Ok(()),
            (_, Some(_)) => bail!("ordinary diagnostics cannot claim a resource limit"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProblemKind {
    Traversal,
    Read,
    InvalidUtf8,
    UnsupportedLanguage,
    Parse,
    Binding,
    ChangedSource,
    UnsupportedSyntax,
    SourceComplexityLimit,
    AnalysisLimit,
    ReportLimit,
    ParserRuntime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisLimitKind {
    FileBytes,
    TotalBytes,
    Files,
    DiscoveryEntries,
    RawUnits,
    DelimiterDepth,
    NodesPerFile,
    Nodes,
    Candidates,
    Comparisons,
    ComparisonEntries,
    Pairs,
    Sites,
    ReportBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkippedSource {
    pub path: String,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkipReason {
    ConfiguredExclusion,
    UnsupportedLanguage,
    DeclarationOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectionReport {
    pub requested: SourceSelection,
    pub selected: Vec<SourceFile>,
    pub skipped: Vec<SkippedSource>,
    pub complete_within_selection: bool,
}

impl SelectionReport {
    pub fn validate(&self) -> Result<()> {
        self.requested.validate()?;
        let include = crate::config::Matcher::new(&self.requested.include)?;
        let exclude = crate::config::Matcher::new(&self.requested.exclude)?;
        let mut paths = BTreeSet::new();
        let mut previous: Option<&str> = None;
        for file in &self.selected {
            file.validate()?;
            if !include.matches(&file.path) || exclude.matches(&file.path) {
                bail!("selected source does not belong to the requested selection");
            }
            if previous.is_some_and(|path| path >= file.path.as_str()) {
                bail!("selected source inventory must be sorted and unique");
            }
            previous = Some(&file.path);
            paths.insert(file.path.as_str());
        }
        for skipped in &self.skipped {
            validate_relative_path(&skipped.path)?;
            if !paths.insert(&skipped.path) {
                bail!("duplicate or conflicting source selection entry");
            }
            if self.complete_within_selection && skipped.reason == SkipReason::UnsupportedLanguage {
                bail!("unsupported requested language cannot be complete");
            }
        }
        if self.complete_within_selection && self.selected.is_empty() {
            bail!("an empty selected source inventory cannot be complete");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Completion {
    pub complete: bool,
    pub problems: Vec<AnalysisProblem>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OmittedEvidence {
    pub selected_sources: usize,
    pub skipped_sources: usize,
    pub functions: usize,
    pub excluded: usize,
    pub pairs: usize,
    pub sites: usize,
    pub problems: usize,
}

impl OmittedEvidence {
    pub fn is_empty(&self) -> bool {
        self.selected_sources == 0
            && self.skipped_sources == 0
            && self.functions == 0
            && self.excluded == 0
            && self.pairs == 0
            && self.sites == 0
            && self.problems == 0
    }
}
