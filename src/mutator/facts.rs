use crate::quality::{
    AnalysisLimits, AnalysisProblem, FunctionLocation, MAX_PATH_BYTES, OmittedEvidence,
    ProblemKind, SelectionReport, SourceFile, SourceLocation, SourceSelection,
    validate_configuration, validate_report_size, validate_sha256,
};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::Path,
};

pub const SCHEMA_VERSION: u32 = 1;
pub const OPERATOR_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MutationPlanConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub selection: SourceSelection,
    #[serde(default)]
    pub limits: AnalysisLimits,
    #[serde(default = "default_operators")]
    pub operators: Vec<MutationOperator>,
}

fn default_operators() -> Vec<MutationOperator> {
    MutationOperator::ALL.into()
}

impl Default for MutationPlanConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            selection: SourceSelection::default(),
            limits: AnalysisLimits::default(),
            operators: default_operators(),
        }
    }
}

impl MutationPlanConfig {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            bail!(
                "unsupported mutation plan configuration schemaVersion {}",
                self.schema_version
            );
        }
        validate_configuration(self)?;
        self.selection.validate()?;
        self.limits.validate()?;
        let unique: BTreeSet<_> = self.operators.iter().collect();
        if unique.is_empty() || unique.len() != self.operators.len() {
            bail!("mutation operator list must be nonempty and unique");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MutationOperator {
    Comparison,
    Equality,
    Arithmetic,
    Logical,
    Update,
    Boolean,
    ZeroOne,
}

impl MutationOperator {
    pub const ALL: [Self; 7] = [
        Self::Comparison,
        Self::Equality,
        Self::Arithmetic,
        Self::Logical,
        Self::Update,
        Self::Boolean,
        Self::ZeroOne,
    ];

    pub fn accepts(self, expected: &str, replacement: &str) -> bool {
        match self {
            Self::Comparison => matches!(
                (expected, replacement),
                ("<", "<=") | ("<=", "<") | (">", ">=") | (">=", ">")
            ),
            Self::Equality => matches!(
                (expected, replacement),
                ("==", "!=") | ("!=", "==") | ("===", "!==") | ("!==", "===")
            ),
            Self::Arithmetic => matches!(
                (expected, replacement),
                ("+", "-") | ("-", "+") | ("*", "/") | ("/", "*")
            ),
            Self::Logical => matches!((expected, replacement), ("&&", "||") | ("||", "&&")),
            Self::Update => matches!((expected, replacement), ("++", "--") | ("--", "++")),
            Self::Boolean => matches!(
                (expected, replacement),
                ("true", "false") | ("false", "true")
            ),
            Self::ZeroOne => numeric_zero_one(expected).is_some_and(|value| {
                (value == 0.0 && replacement == "1") || (value == 1.0 && replacement == "0")
            }),
        }
    }
}

fn numeric_zero_one(text: &str) -> Option<f64> {
    if text.is_empty() || text.starts_with(['+', '-']) || text.chars().any(char::is_whitespace) {
        return None;
    }
    let text = text.replace('_', "");
    for (prefixes, radix) in [(["0x", "0X"], 16), (["0o", "0O"], 8), (["0b", "0B"], 2)] {
        if let Some(digits) = prefixes.iter().find_map(|prefix| text.strip_prefix(prefix)) {
            return u64::from_str_radix(digits, radix)
                .ok()
                .map(|value| value as f64);
        }
    }
    text.parse().ok()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MutationPlan {
    pub schema_version: u32,
    pub operator_version: u32,
    pub configuration: MutationPlanConfig,
    pub root: String,
    pub selection: SelectionReport,
    pub files: Vec<SourceFile>,
    pub sites: Vec<MutationSite>,
    pub complete: bool,
    pub problems: Vec<AnalysisProblem>,
    pub omitted_evidence: OmittedEvidence,
}

impl MutationPlan {
    /// Validate the persisted contract; source bytes and AST sites need independent validation.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION || self.operator_version != OPERATOR_VERSION {
            bail!("unsupported mutation plan or operator version");
        }
        self.configuration.validate()?;
        if self.root.len() > MAX_PATH_BYTES
            || !Path::new(&self.root).is_absolute()
            || self.root.contains('\\')
            || self.root.chars().any(char::is_control)
            || self.root.split('/').any(|part| matches!(part, "." | ".."))
            || (self.root != "/" && (self.root.contains("//") || self.root.ends_with('/')))
        {
            bail!("mutation plan root must be a bounded absolute canonical path");
        }
        self.selection.validate()?;
        if self.selection.requested != self.configuration.selection
            || self.files != self.selection.selected
        {
            bail!("mutation plan selection and source inventories disagree");
        }
        if self.complete
            && (!self.selection.complete_within_selection
                || !self.problems.is_empty()
                || !self.omitted_evidence.is_empty())
        {
            bail!("mutation plan cannot be complete with missing analysis or omitted evidence");
        }
        if self.sites.len() > self.configuration.limits.max_sites
            || self.files.len() > self.configuration.limits.max_files
        {
            bail!("mutation plan exceeds configured inventory limits");
        }
        let sources: BTreeMap<_, _> = self
            .files
            .iter()
            .map(|file| (file.path.as_str(), file))
            .collect();
        let mut total_bytes = 0usize;
        for file in &self.files {
            total_bytes = total_bytes
                .checked_add(file.bytes)
                .ok_or_else(|| anyhow::anyhow!("source byte count overflow"))?;
            if file.bytes > self.configuration.limits.max_file_bytes
                || total_bytes > self.configuration.limits.max_total_bytes
            {
                bail!("source inventory exceeds configured byte limits");
            }
        }
        for problem in &self.problems {
            problem.validate(&self.configuration.limits)?;
        }
        let mut ids = BTreeSet::new();
        for site in &self.sites {
            site.validate()?;
            if !ids.insert(&site.id) || !self.configuration.operators.contains(&site.operator) {
                bail!("duplicate mutation ID or unconfigured operator");
            }
            let Some(file) = sources.get(site.location.file.as_str()) else {
                bail!("mutation site has no selected source file");
            };
            if site.source_sha256 != file.sha256 || site.location.end > file.bytes {
                bail!("mutation site source hash or byte range disagrees with inventory");
            }
            if site
                .owner
                .as_ref()
                .is_some_and(|owner| owner.name.len() > self.configuration.limits.max_name_bytes)
            {
                bail!("mutation owner display name exceeds its evidence limit");
            }
        }
        validate_report_size(self, &self.configuration.limits)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MutationInventory {
    pub sites: Vec<MutationSite>,
    pub complete: bool,
    pub problems: Vec<AnalysisProblem>,
    pub omitted_evidence: OmittedEvidence,
}

impl MutationInventory {
    pub fn validate(&self) -> Result<()> {
        if self.complete && (!self.problems.is_empty() || !self.omitted_evidence.is_empty()) {
            bail!("partial mutation evidence cannot be complete");
        }
        let mut ids = BTreeSet::new();
        for problem in &self.problems {
            problem.validate(&AnalysisLimits::hard_maximum())?;
        }
        for site in &self.sites {
            site.validate()?;
            if !ids.insert(&site.id) {
                bail!("duplicate mutation ID");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MutationSite {
    pub id: String,
    pub location: SourceLocation,
    pub owner: Option<FunctionLocation>,
    pub operator: MutationOperator,
    pub expected: String,
    pub replacement: String,
    pub source_sha256: String,
}

impl MutationSite {
    pub fn identity(&self) -> String {
        let bytes = serde_json::to_vec(&(
            OPERATOR_VERSION,
            &self.location.file,
            &self.source_sha256,
            self.location.start,
            self.location.end,
            &self.expected,
            &self.replacement,
        ))
        .expect("strings and integers serialize");
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    pub fn validate(&self) -> Result<()> {
        self.location.validate()?;
        validate_sha256(&self.source_sha256)?;
        validate_sha256(&self.id)?;
        if self.expected.len() > AnalysisLimits::hard_maximum().max_file_bytes
            || self.location.end - self.location.start != self.expected.len()
            || !self.operator.accepts(&self.expected, &self.replacement)
            || self.id != self.identity()
        {
            bail!("invalid mutation replacement, byte range, or full site identity");
        }
        if let Some(owner) = &self.owner {
            owner.location.validate()?;
            if owner.location.file != self.location.file
                || owner.location.start > self.location.start
                || owner.location.end < self.location.end
            {
                bail!("mutation owner must contain the site in the same source file");
            }
            if owner.name.is_empty()
                || owner.name.len() > AnalysisLimits::hard_maximum().max_name_bytes
            {
                bail!("mutation owner display name must be nonempty and bounded");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditError {
    pub kind: InvalidPlanKind,
    pub message: String,
}

impl fmt::Display for EditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.message)
    }
}
impl std::error::Error for EditError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InvalidPlanKind {
    SourceHash,
    Range,
    Utf8Boundary,
    ExpectedText,
    SiteIdentity,
    UnsafePath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase", deny_unknown_fields)]
pub enum SyntaxValidation {
    Valid,
    InvalidSyntax { diagnostics: Vec<AnalysisProblem> },
    Incomplete { problems: Vec<AnalysisProblem> },
}

impl SyntaxValidation {
    pub fn validate(&self) -> Result<()> {
        let problems = match self {
            Self::Valid => return Ok(()),
            Self::InvalidSyntax { diagnostics } => diagnostics,
            Self::Incomplete { problems } => problems,
        };
        for problem in problems {
            problem.validate(&AnalysisLimits::hard_maximum())?;
        }
        match self {
            Self::Valid => Ok(()),
            Self::InvalidSyntax { diagnostics }
                if !diagnostics.is_empty()
                    && diagnostics.iter().all(|problem| {
                        problem.kind == ProblemKind::Parse && problem.limit.is_none()
                    }) =>
            {
                Ok(())
            }
            Self::Incomplete { problems } if !problems.is_empty() => Ok(()),
            _ => bail!(
                "syntax-invalid requires parser diagnostics; incomplete requires explicit problems"
            ),
        }
    }
}
