use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectFacts {
    pub schema_version: u32,
    pub root: String,
    pub files: Vec<FileFacts>,
    pub packages: Vec<PackageFacts>,
    pub problems: Vec<Problem>,
    pub resolution: ResolutionProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolutionProfile {
    pub mode: String,
    pub conditions: Vec<String>,
    pub extension_aliases: Vec<(String, Vec<String>)>,
    pub tsconfig: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileFacts {
    pub path: String,
    pub bytes: usize,
    pub language: String,
    pub imports: Vec<ImportFact>,
    pub exports: Vec<ExportFact>,
    pub calls: Vec<CallFact>,
    pub services: Vec<ServiceFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportFact {
    pub specifier: Option<String>,
    pub kind: String,
    pub type_only: bool,
    pub offset: usize,
    pub bindings: Vec<ImportBinding>,
    pub status: ResolutionStatus,
    pub target: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportBinding {
    pub local: String,
    pub imported: String,
    pub type_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResolutionStatus {
    Internal,
    External,
    Unresolved,
    Excluded,
    OutsideRoot,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportFact {
    pub name: String,
    pub type_only: bool,
    pub local: Option<String>,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallFact {
    pub callee: String,
    pub origin: Option<String>,
    pub offset: usize,
    pub string_arguments: Vec<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceFact {
    pub name: String,
    pub identifier: Option<String>,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageFacts {
    pub path: String,
    pub bytes: usize,
    pub name: String,
    pub dependencies: Vec<String>,
    pub exports: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Problem {
    pub file: String,
    pub offset: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Diagnostic {
    pub rule: String,
    pub file: String,
    pub offset: usize,
    pub message: String,
    #[serde(default)]
    pub evidence: Vec<String>,
}

impl Diagnostic {
    pub fn new(rule: &str, file: &str, offset: usize, message: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            file: file.into(),
            offset,
            message: message.into(),
            evidence: vec![],
        }
    }
}

pub trait ProjectRule {
    fn check(&self, project: &ProjectFacts) -> anyhow::Result<Vec<Diagnostic>>;
}

impl ProjectFacts {
    pub fn file(&self, path: &str) -> Option<&FileFacts> {
        self.files
            .binary_search_by(|file| file.path.as_str().cmp(path))
            .ok()
            .map(|i| &self.files[i])
    }
}
