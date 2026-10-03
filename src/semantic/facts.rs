use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const BACKEND_VERSION: &str = "7.0.2";
pub const API_ID: &str = "typescript/unstable/sync";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertySite {
    pub offset: usize,
    pub member: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestedFile {
    pub path: String,
    pub bytes: usize,
    pub sha256: String,
    pub sites: Vec<PropertySite>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestedContext {
    pub id: String,
    pub tsconfig: String,
    pub config_sha256: String,
    pub files: Vec<RequestedFile>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticRequest {
    pub schema_version: u32,
    pub root: String,
    pub backend: Backend,
    pub contexts: Vec<RequestedContext>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Backend {
    pub name: String,
    pub version: String,
    pub api: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticFacts {
    pub schema_version: u32,
    pub root: String,
    pub backend: Backend,
    pub complete: bool,
    pub contexts: Vec<ContextFacts>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextFacts {
    pub id: String,
    pub tsconfig: String,
    pub config_sha256: String,
    pub complete: bool,
    pub files: Vec<SemanticFile>,
    pub diagnostics: Vec<CompilerDiagnostic>,
    pub problems: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticFile {
    pub path: String,
    pub bytes: usize,
    pub sha256: String,
    pub available: bool,
    pub properties: Vec<PropertyFact>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyFact {
    pub offset: usize,
    pub member: String,
    pub receiver: Receiver,
    pub status: MemberStatus,
    pub symbol: Option<MemberSymbol>,
    pub detail: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receiver {
    pub state: ReceiverState,
    pub display: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReceiverState {
    Known,
    Any,
    Unknown,
    Error,
    Unavailable,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MemberStatus {
    Present,
    Missing,
    Unavailable,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemberSymbol {
    pub name: String,
    pub declarations: Vec<DeclarationLocation>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeclarationLocation {
    pub file: String,
    pub offset: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompilerDiagnostic {
    pub phase: String,
    pub category: CompilerCategory,
    pub code: u32,
    pub file: Option<String>,
    pub offset: usize,
    pub end: usize,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CompilerCategory {
    Error,
    Warning,
    Suggestion,
    Message,
}
impl SemanticFacts {
    pub fn file(&self, context: &str, path: &str) -> Option<&SemanticFile> {
        self.contexts
            .iter()
            .find(|c| c.id == context)?
            .files
            .iter()
            .find(|f| f.path == path)
    }
    pub fn problems(&self) -> Vec<String> {
        let mut result = vec![];
        for context in &self.contexts {
            result.extend(
                context
                    .problems
                    .iter()
                    .map(|p| format!("compiler context {}: {p}", context.id)),
            );
            for d in &context.diagnostics {
                if d.category == CompilerCategory::Error {
                    result.push(format!(
                        "compiler context {} {} TS{} {}:{}: {}",
                        context.id,
                        d.phase,
                        d.code,
                        d.file.as_deref().unwrap_or("."),
                        d.offset,
                        d.message
                    ));
                }
            }
            for file in &context.files {
                if !file.available {
                    result.push(format!(
                        "compiler context {}: {} is unavailable",
                        context.id, file.path
                    ));
                }
                for fact in &file.properties {
                    if fact.status == MemberStatus::Unavailable {
                        result.push(format!(
                            "compiler context {}: {}:{} member {} unavailable ({:?})",
                            context.id, file.path, fact.offset, fact.member, fact.receiver.state
                        ));
                    }
                }
            }
        }
        result
    }
}
