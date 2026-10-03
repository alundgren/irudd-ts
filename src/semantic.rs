//! Separate compiler capability. Legacy ProjectFacts and plugin protocol remain version 1.
use crate::{
    config::{Matcher, PluginConfig},
    facts::{Diagnostic, ProjectFacts},
};
use anyhow::{Context, Result, bail};
use oxc_allocator::Allocator;
use oxc_ast::ast::StaticMemberExpression;
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::SourceType;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
pub const SCHEMA_VERSION: u32 = 1;
pub const BACKEND_VERSION: &str = "7.0.2";
pub const API_ID: &str = "typescript/unstable/sync";
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
pub trait SemanticRule {
    fn check(&self, facts: &SemanticFacts) -> Result<Vec<Diagnostic>>;
}
impl SemanticRule for MemberRule {
    fn check(&self, facts: &SemanticFacts) -> Result<Vec<Diagnostic>> {
        let matcher = Matcher::new(&self.files)?;
        let mut findings = vec![];
        for context in &facts.contexts {
            for file in &context.files {
                if !matcher.matches(&file.path) {
                    continue;
                }
                for fact in &file.properties {
                    if fact.receiver.state == ReceiverState::Known
                        && fact.status == MemberStatus::Missing
                    {
                        let mut d = Diagnostic::new(
                            &self.id,
                            &file.path,
                            fact.offset,
                            format!(
                                "member {} is absent from the inferred receiver type",
                                fact.member
                            ),
                        );
                        d.evidence = vec![
                            format!("compiler context {}", context.id),
                            fact.receiver.display.clone().unwrap_or_default(),
                        ];
                        findings.push(d);
                    }
                }
            }
        }
        findings.sort();
        findings.dedup();
        Ok(findings)
    }
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
pub fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn inventory(path: &str, source: &str) -> Result<Vec<PropertySite>> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::from_path(path)?).parse();
    if !parsed.diagnostics.is_empty() {
        bail!(
            "semantic site inventory parse errors: {}",
            parsed
                .diagnostics
                .iter()
                .map(|d| d.message.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    struct Sites(Vec<PropertySite>);
    impl<'a> Visit<'a> for Sites {
        fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
            self.0.push(PropertySite {
                offset: it.property.span.start as usize,
                member: it.property.name.to_string(),
            });
            walk::walk_static_member_expression(self, it);
        }
    }
    let mut sites = Sites(vec![]);
    sites.visit_program(&parsed.program);
    sites.0.sort_by_key(|s| s.offset);
    Ok(sites.0)
}
pub fn request(project: &ProjectFacts, config: &SemanticConfig) -> Result<SemanticRequest> {
    config.validate()?;
    if project.schema_version != crate::facts::SCHEMA_VERSION {
        bail!("unsupported source graph facts version");
    }
    let root = Path::new(&project.root).canonicalize()?;
    let mut contexts = vec![];
    for context in &config.contexts {
        let matcher = Matcher::new(&context.files)?;
        let tsconfig = root
            .join(&context.tsconfig)
            .canonicalize()
            .with_context(|| format!("compiler context {} tsconfig", context.id))?;
        let config_sha256 = hash(&std::fs::read(&tsconfig)?);
        let mut files = vec![];
        for file in &project.files {
            if !matcher.matches(&file.path) {
                continue;
            }
            if file.language != "typescript" {
                bail!(
                    "semantic context {} selected unsupported file {}",
                    context.id,
                    file.path
                );
            }
            let source = std::fs::read_to_string(root.join(&file.path))?;
            if source.len() != file.bytes {
                bail!("source changed since graph analysis: {}", file.path);
            }
            files.push(RequestedFile {
                path: file.path.clone(),
                bytes: source.len(),
                sha256: hash(source.as_bytes()),
                sites: inventory(&file.path, &source)?,
            });
        }
        if files.is_empty() {
            bail!(
                "compiler context {} selects no analyzed source files",
                context.id
            );
        }
        contexts.push(RequestedContext {
            id: context.id.clone(),
            tsconfig: tsconfig.to_string_lossy().into(),
            config_sha256,
            files,
        });
    }
    Ok(SemanticRequest {
        schema_version: SCHEMA_VERSION,
        root: root.to_string_lossy().into(),
        backend: Backend {
            name: "typescript".into(),
            version: BACKEND_VERSION.into(),
            api: API_ID.into(),
        },
        contexts,
    })
}
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
pub fn validate(request: &SemanticRequest, facts: &SemanticFacts) -> Result<()> {
    if request.schema_version != SCHEMA_VERSION
        || request.backend.name != "typescript"
        || request.backend.version != BACKEND_VERSION
        || request.backend.api != API_ID
        || facts.schema_version != SCHEMA_VERSION
        || facts.root != request.root
        || facts.backend != request.backend
    {
        bail!("semantic protocol/backend/root mismatch");
    }
    let expected: BTreeMap<_, _> = request
        .contexts
        .iter()
        .map(|c| (c.id.as_str(), c))
        .collect();
    if expected.is_empty() || expected.len() != request.contexts.len() {
        bail!("invalid requested compiler contexts");
    }
    let mut seen = BTreeSet::new();
    let mut all_complete = true;
    for context in &facts.contexts {
        let wanted = expected
            .get(context.id.as_str())
            .context("unknown semantic context")?;
        if !seen.insert(&context.id)
            || context.tsconfig != wanted.tsconfig
            || context.config_sha256 != wanted.config_sha256
        {
            bail!("duplicate or mismatched semantic context");
        }
        if hash(&std::fs::read(&context.tsconfig)?) != wanted.config_sha256 {
            bail!("compiler configuration changed during analysis");
        }
        let files: BTreeMap<_, _> = wanted.files.iter().map(|f| (f.path.as_str(), f)).collect();
        let mut seen_files = BTreeSet::new();
        let mut complete = context.problems.is_empty()
            && !context
                .diagnostics
                .iter()
                .any(|d| d.category == CompilerCategory::Error);
        for diagnostic in &context.diagnostics {
            if !["config", "program", "global", "syntax", "bind", "semantic"]
                .contains(&diagnostic.phase.as_str())
                || diagnostic.code == 0
                || diagnostic.message.trim().is_empty()
                || diagnostic.end < diagnostic.offset
                || diagnostic
                    .file
                    .as_ref()
                    .is_some_and(|f| f.trim().is_empty())
            {
                bail!("invalid compiler diagnostic");
            }
            if let Some(file) = &diagnostic.file {
                validate_location(&request.root, file, diagnostic.offset, diagnostic.end)?;
            }
        }
        for file in &context.files {
            let wanted = files
                .get(file.path.as_str())
                .context("extra semantic file")?;
            if !seen_files.insert(&file.path)
                || file.bytes != wanted.bytes
                || file.sha256 != wanted.sha256
            {
                bail!("duplicate or mismatched semantic file");
            }
            let source = std::fs::read(Path::new(&request.root).join(&file.path))?;
            if hash(&source) != wanted.sha256 {
                bail!("source changed during semantic analysis");
            }
            let sites: BTreeMap<_, _> = wanted
                .sites
                .iter()
                .map(|s| (s.offset, s.member.as_str()))
                .collect();
            let mut seen_sites = BTreeSet::new();
            complete &= file.available;
            for property in &file.properties {
                if sites.get(&property.offset) != Some(&property.member.as_str())
                    || !seen_sites.insert(property.offset)
                {
                    bail!("extra, duplicate or mismatched semantic property site");
                }
                let known = property.receiver.state == ReceiverState::Known;
                let usable = property.status != MemberStatus::Unavailable;
                if known != usable
                    || known
                        && property
                            .receiver
                            .display
                            .as_ref()
                            .is_none_or(|s| s.trim().is_empty())
                    || !file.available && usable
                    || property.status == MemberStatus::Present
                        && property
                            .symbol
                            .as_ref()
                            .is_none_or(|s| s.name != property.member)
                    || property.status != MemberStatus::Present && property.symbol.is_some()
                    || !usable && property.detail.as_ref().is_none_or(|s| s.trim().is_empty())
                {
                    bail!("contradictory semantic property fact");
                }
                if let Some(symbol) = &property.symbol {
                    for declaration in &symbol.declarations {
                        if declaration.file.trim().is_empty() {
                            bail!("invalid semantic declaration location");
                        }
                        validate_location(
                            &request.root,
                            &declaration.file,
                            declaration.offset,
                            declaration.offset,
                        )?;
                    }
                }
                complete &= usable;
            }
            if seen_sites.len() != sites.len() {
                bail!("provider omitted semantic property sites");
            }
        }
        if seen_files.len() != files.len() {
            bail!("provider omitted semantic files");
        }
        if context.complete != complete {
            bail!("forged semantic context completion");
        }
        all_complete &= complete;
    }
    if seen.len() != expected.len() {
        bail!("provider omitted compiler contexts");
    }
    if facts.complete != all_complete {
        bail!("forged semantic completion");
    }
    Ok(())
}
fn validate_location(root: &str, file: &str, offset: usize, end: usize) -> Result<()> {
    let source = std::fs::read_to_string(Path::new(root).join(file))
        .with_context(|| format!("semantic location source {file}"))?;
    if end < offset
        || end > source.len()
        || !source.is_char_boundary(offset)
        || !source.is_char_boundary(end)
    {
        bail!("invalid semantic UTF-8 location in {file}");
    }
    Ok(())
}
pub fn check(facts: &SemanticFacts, config: &SemanticConfig) -> Result<Vec<Diagnostic>> {
    let mut result = vec![];
    for rule in &config.rules {
        result.extend(rule.check(facts)?);
    }
    result.sort();
    result.dedup();
    Ok(result)
}
