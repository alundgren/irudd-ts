use anyhow::{Context, Result, bail};
use globset::{GlobBuilder, GlobMatcher};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    #[serde(default = "default_include")]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default = "default_conditions")]
    pub conditions: Vec<String>,
    #[serde(default = "default_extensions")]
    pub extensions: Vec<String>,
    #[serde(default = "default_extension_aliases")]
    pub extension_aliases: Vec<(String, Vec<String>)>,
    #[serde(default = "default_packages")]
    pub package_manifests: Vec<String>,
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
    #[serde(default)]
    pub plugins: Vec<PluginConfig>,
}
fn default_include() -> Vec<String> {
    vec![
        "**/*.ts".into(),
        "**/*.tsx".into(),
        "**/*.js".into(),
        "**/*.jsx".into(),
        "**/*.mts".into(),
        "**/*.cts".into(),
        "**/*.mjs".into(),
        "**/*.cjs".into(),
    ]
}
fn default_packages() -> Vec<String> {
    vec![
        "package.json".into(),
        "apps/*/package.json".into(),
        "packages/*/package.json".into(),
        "infra/*/package.json".into(),
    ]
}
fn default_conditions() -> Vec<String> {
    vec!["types".into(), "import".into(), "default".into()]
}
fn default_extensions() -> Vec<String> {
    [".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs", ".json"].into_iter().map(String::from).collect()
}
fn default_extension_aliases() -> Vec<(String, Vec<String>)> {
    vec![(".js".into(), vec![".ts".into(), ".tsx".into(), ".js".into()]), (".mjs".into(), vec![".mts".into(), ".mjs".into()]), (".cjs".into(), vec![".cts".into(), ".cjs".into()])]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuleConfig {
    pub id: String,
    pub kind: RuleKind,
    #[serde(default = "all_files")]
    pub files: Vec<String>,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub specifiers: Vec<String>,
    #[serde(default)]
    pub origins: Vec<String>,
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default)]
    pub transitive: bool,
    #[serde(default = "yes")]
    pub include_types: bool,
    #[serde(default)]
    pub exceptions: Vec<String>,
}
fn all_files() -> Vec<String> {
    vec!["**".into()]
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RuleKind {
    ForbiddenDependency,
    ForbiddenImport,
    ForbiddenCall,
    NoCycles,
    ServiceNamespace,
    UniqueServiceId,
    ServiceLayer,
    RequiredExport,
    RequiredFile,
    PackageDependency,
    PublicEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginConfig {
    pub name: String,
    pub command: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}
fn default_timeout() -> u64 {
    5000
}

pub fn read(path: &Path) -> Result<(Config, PathBuf)> {
    let absolute = path
        .canonicalize()
        .with_context(|| format!("configuration {}", path.display()))?;
    let config: Config = serde_json::from_str(&std::fs::read_to_string(&absolute)?)?;
    config.validate()?;
    Ok((
        config,
        absolute
            .parent()
            .context("config has no parent")?
            .to_owned(),
    ))
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            bail!(
                "unsupported configuration schemaVersion {}",
                self.schema_version
            );
        }
        if self.include.is_empty() {
            bail!("include must select at least one file pattern");
        }
        if self.extensions.is_empty() || self.extensions.iter().chain(self.extension_aliases.iter().flat_map(|(key,values)|std::iter::once(key).chain(values))).any(|extension| !extension.starts_with('.') || extension.len()<2 || extension.contains('/') || extension.contains('\\')) {
            bail!("resolution extensions must be nonempty dot-prefixed suffixes");
        }
        let mut ids = std::collections::BTreeSet::new();
        for r in &self.rules {
            if r.id.trim().is_empty() || !ids.insert(&r.id) {
                bail!("empty or duplicate rule id {}", r.id);
            }
            Matcher::new(&r.files)?;
            Matcher::new(&r.targets)?;
            Matcher::new(&r.specifiers)?;
            Matcher::new(&r.origins)?;
            Matcher::new(&r.exceptions)?;
            match r.kind {
                RuleKind::ForbiddenDependency | RuleKind::PublicEntry if r.targets.is_empty() => {
                    bail!("{} requires targets", r.id)
                }
                RuleKind::ForbiddenImport | RuleKind::PackageDependency
                    if r.specifiers.is_empty() =>
                {
                    bail!("{} requires specifiers", r.id)
                }
                RuleKind::ForbiddenCall if r.origins.is_empty() => {
                    bail!("{} requires origins", r.id)
                }
                RuleKind::RequiredExport if r.names.is_empty() => bail!("{} requires names", r.id),
                _ => {}
            }
        }
        let mut names = std::collections::BTreeSet::new();
        for p in &self.plugins {
            if p.name.trim().is_empty()
                || !names.insert(&p.name)
                || p.command.is_empty()
                || p.command.iter().any(|s| s.is_empty())
                || p.timeout_ms == 0
            {
                bail!("invalid or duplicate plugin {}", p.name);
            }
        }
        Matcher::new(&self.include)?;
        Matcher::new(&self.exclude)?;
        Matcher::new(&self.package_manifests)?;
        Ok(())
    }
}

pub struct Matcher(Vec<GlobMatcher>);
impl Matcher {
    pub fn new(patterns: &[String]) -> Result<Self> {
        Ok(Self(
            patterns
                .iter()
                .map(|p| {
                    GlobBuilder::new(p)
                        .literal_separator(true)
                        .build()
                        .map(|g| g.compile_matcher())
                })
                .collect::<std::result::Result<_, _>>()?,
        ))
    }
    pub fn matches(&self, value: &str) -> bool {
        self.0.iter().any(|m| m.is_match(value))
    }
}
