use crate::{
    config::{Config, Matcher},
    facts::*,
    rust, typescript,
};
use anyhow::{Context, Result};
use ignore::WalkBuilder;
use oxc_resolver::{ResolveOptions, Resolver, TsconfigDiscovery};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub fn analyze(root: &Path, config: &Config) -> Result<ProjectFacts> {
    config.validate()?;
    let root = root.canonicalize()?;
    let include = Matcher::new(&config.include)?;
    let exclude = Matcher::new(&config.exclude)?;
    let manifests = Matcher::new(&config.package_manifests)?;
    let mut project = ProjectFacts {
        schema_version: SCHEMA_VERSION,
        root: root.to_string_lossy().into(),
        files: vec![],
        packages: vec![],
        problems: vec![],
        resolution: ResolutionProfile {
            mode: "source".into(),
            conditions: config.conditions.clone(),
            extensions: config.extensions.clone(),
            extension_aliases: config.extension_aliases.clone(),
            tsconfig: "nearest tsconfig paths and inheritance via Oxc; no compiler mode parity"
                .into(),
        },
    };
    let mut paths = BTreeMap::new();
    for entry in WalkBuilder::new(&root)
        .hidden(false)
        .git_ignore(true)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|e| {
            !e.file_type().is_some_and(|t| t.is_dir())
                || !matches!(
                    e.file_name().to_str(),
                    Some(".git" | "node_modules" | "target")
                )
        })
        .build()
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                project.problems.push(Problem {
                    file: ".".into(),
                    offset: 0,
                    message: format!("file traversal: {error}"),
                });
                continue;
            }
        };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = relative(&root, entry.path()).context("file outside root")?;
        if path
            .split('/')
            .any(|part| matches!(part, ".git" | "node_modules" | "target"))
            || exclude.matches(&path)
        {
            continue;
        }
        if manifests.matches(&path) {
            match package(&path, entry.path()) {
                Ok(Some(p)) => project.packages.push(p),
                Ok(None) => {}
                Err(e) => project.problems.push(Problem {
                    file: path.clone(),
                    offset: 0,
                    message: format!("package metadata: {e:#}"),
                }),
            }
        }
        if !include.matches(&path) {
            continue;
        }
        let extension = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        if !matches!(
            extension,
            "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" | "rs"
        ) {
            continue;
        }
        let result = std::fs::read_to_string(entry.path())
            .map_err(anyhow::Error::from)
            .and_then(|source| {
                if extension == "rs" {
                    rust::parse(&path, &source)
                } else {
                    typescript::parse(&path, &source)
                }
            });
        match result {
            Ok(file) => {
                paths.insert(path.clone(), entry.path().to_owned());
                project.files.push(file);
            }
            Err(e) => project.problems.push(Problem {
                file: path,
                offset: 0,
                message: format!("source analysis: {e:#}"),
            }),
        }
    }
    if project.files.is_empty() {
        project.problems.push(Problem {
            file: ".".into(),
            offset: 0,
            message: "no source files selected".into(),
        });
    }
    project.files.sort_by(|a, b| a.path.cmp(&b.path));
    project.packages.sort_by(|a, b| a.path.cmp(&b.path));
    let mut seen = BTreeSet::new();
    for p in &project.packages {
        if !seen.insert(&p.name) {
            project.problems.push(Problem {
                file: p.path.clone(),
                offset: 0,
                message: format!("duplicate workspace package {}", p.name),
            });
        }
    }
    let resolver = Resolver::new(ResolveOptions {
        cwd: Some(root.clone()),
        tsconfig: Some(TsconfigDiscovery::Auto),
        condition_names: config.conditions.clone(),
        extensions: config.extensions.clone(),
        extension_alias: config.extension_aliases.clone(),
        ..ResolveOptions::default()
    });
    let available: BTreeSet<_> = paths.keys().cloned().collect();
    for file in &mut project.files {
        let absolute = &paths[&file.path];
        for edge in &mut file.imports {
            resolve_edge(
                &root,
                absolute,
                edge,
                &project.packages,
                &resolver,
                &available,
            )?;
            if matches!(
                edge.status,
                ResolutionStatus::Unresolved
                    | ResolutionStatus::Excluded
                    | ResolutionStatus::OutsideRoot
                    | ResolutionStatus::Unsupported
            ) {
                project.problems.push(Problem {
                    file: file.path.clone(),
                    offset: edge.offset,
                    message: format!(
                        "{:?} dependency {}: {}",
                        edge.status,
                        edge.specifier.as_deref().unwrap_or("<nonliteral>"),
                        edge.detail.as_deref().unwrap_or("no detail")
                    ),
                });
            }
        }
    }
    project
        .problems
        .sort_by(|a, b| (&a.file, a.offset, &a.message).cmp(&(&b.file, b.offset, &b.message)));
    Ok(project)
}
fn package(path: &str, absolute: &Path) -> Result<Option<PackageFacts>> {
    let source = std::fs::read_to_string(absolute)?;
    let value: serde_json::Value = serde_json::from_str(&source)?;
    let Some(name) = value.get("name").and_then(|n| n.as_str()) else {
        return Ok(None);
    };
    let mut dependencies = BTreeSet::new();
    for kind in ["dependencies", "peerDependencies", "optionalDependencies"] {
        if let Some(values) = value.get(kind).and_then(|v| v.as_object()) {
            dependencies.extend(values.keys().cloned());
        }
    }
    Ok(Some(PackageFacts {
        path: path.into(),
        bytes: source.len(),
        name: name.into(),
        dependencies: dependencies.into_iter().collect(),
        exports: value
            .get("exports")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    }))
}
fn resolve_edge(
    root: &Path,
    source: &Path,
    edge: &mut ImportFact,
    packages: &[PackageFacts],
    resolver: &Resolver,
    available: &BTreeSet<String>,
) -> Result<()> {
    let Some(specifier) = edge.specifier.as_deref() else {
        edge.status = ResolutionStatus::Unsupported;
        edge.detail = Some("nonliteral dependency target".into());
        return Ok(());
    };
    if edge.kind == "rustUse" {
        if let Some(rest) = specifier.strip_prefix("crate::") {
            let module = rest.split("::").next().unwrap_or_default();
            let parent = if source.components().any(|c| c.as_os_str() == "src") {
                root.join("src")
            } else {
                source.parent().unwrap_or(root).to_owned()
            };
            let candidates = [
                parent.join(format!("{module}.rs")),
                parent.join(module).join("mod.rs"),
            ];
            if let Some(target) = candidates.iter().find(|p| p.is_file()) {
                set_target(root, target, edge, available)?;
            } else {
                edge.status = ResolutionStatus::Unresolved;
                edge.detail = Some(
                    "Rust crate module unavailable; macro and inline modules are unsupported"
                        .into(),
                );
            }
        } else if specifier.starts_with("self::") || specifier.starts_with("super::") {
            edge.status = ResolutionStatus::Unsupported;
            edge.detail = Some("relative Rust module imports are unsupported".into());
        } else {
            edge.status = ResolutionStatus::External;
        }
        return Ok(());
    }
    if specifier.starts_with("node:") {
        edge.status = ResolutionStatus::External;
        return Ok(());
    }
    let workspace = packages
        .iter()
        .filter(|p| specifier == p.name || specifier.starts_with(&format!("{}/", p.name)))
        .max_by_key(|p| p.name.len());
    let result = if let Some(p) = workspace {
        let subpath = if specifier == p.name {
            ".".to_owned()
        } else {
            format!("./{}", &specifier[p.name.len() + 1..])
        };
        let directory = root
            .join(&p.path)
            .parent()
            .context("package directory")?
            .to_owned();
        match export_target(&p.exports, &subpath, &resolver.options().condition_names) {
            Some(target) if valid_export_target(&target) => resolver
                .resolve(&directory, &target)
                .map(|r| r.path().to_owned()),
            _ => {
                edge.status = ResolutionStatus::Unresolved;
                edge.detail = Some(format!(
                    "workspace package {} does not export {subpath} under configured conditions",
                    p.name
                ));
                return Ok(());
            }
        }
    } else {
        resolver
            .resolve_file(source, specifier)
            .map(|r| r.path().to_owned())
    };
    match result {
        Ok(target) => set_target(root, &target, edge, available)?,
        Err(error) => {
            let tsconfig = match resolver.find_tsconfig(source) {
                Ok(config) => config,
                Err(error) => {
                    edge.status = ResolutionStatus::Unsupported;
                    edge.detail = Some(format!("tsconfig analysis unavailable: {error}"));
                    return Ok(());
                }
            };
            let alias = tsconfig.is_some_and(|config| {
                config.compiler_options.paths.as_ref().is_some_and(|paths| {
                    paths.keys().any(|pattern| match pattern.split_once('*') {
                        Some((prefix, suffix)) => {
                            specifier.starts_with(prefix) && specifier.ends_with(suffix)
                        }
                        None => specifier == pattern,
                    })
                })
            });
            let internal = alias
                || workspace.is_some()
                || specifier.starts_with('.')
                || specifier.starts_with('/')
                || specifier.starts_with('#')
                || specifier.starts_with("@/")
                || specifier.starts_with("~/");
            edge.status = if internal {
                ResolutionStatus::Unresolved
            } else {
                ResolutionStatus::External
            };
            edge.detail = Some(error.to_string());
        }
    }
    Ok(())
}
fn set_target(
    root: &Path,
    target: &Path,
    edge: &mut ImportFact,
    available: &BTreeSet<String>,
) -> Result<()> {
    let target = target.canonicalize().unwrap_or_else(|_| target.to_owned());
    if target.components().any(|c| c.as_os_str() == "node_modules") {
        edge.status = ResolutionStatus::External;
        return Ok(());
    }
    let Some(path) = relative(root, &target) else {
        edge.status = ResolutionStatus::OutsideRoot;
        edge.detail = Some(target.display().to_string());
        return Ok(());
    };
    let extension = target
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    edge.target = Some(path.clone());
    if available.contains(&path) {
        edge.status = ResolutionStatus::Internal;
    } else if matches!(
        extension,
        "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" | "rs"
    ) {
        edge.status = ResolutionStatus::Excluded;
        edge.detail =
            Some("source target was ignored, excluded, unselected or failed parsing".into());
    } else {
        edge.status = ResolutionStatus::External;
        edge.detail = Some("non-source asset; graph traversal stops here".into());
    }
    Ok(())
}
fn valid_export_target(target: &str) -> bool {
    target.starts_with("./")
        && !target.contains('\\')
        && !target.to_ascii_lowercase().contains("%2e")
        && !target.to_ascii_lowercase().contains("%2f")
        && !target.to_ascii_lowercase().contains("%5c")
        && target[2..]
            .split('/')
            .all(|part| part != ".." && part != "." && part != "node_modules")
}
fn export_target(
    value: &serde_json::Value,
    subpath: &str,
    conditions: &[String],
) -> Option<String> {
    enum Selection {
        Target(String),
        Blocked,
        Unmatched,
    }
    fn select(value: &serde_json::Value, conditions: &[String]) -> Selection {
        match value {
            serde_json::Value::String(s) => Selection::Target(s.clone()),
            serde_json::Value::Null => Selection::Blocked,
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if key == "default" || conditions.contains(key) {
                        match select(value, conditions) {
                            Selection::Unmatched => {}
                            selected => return selected,
                        }
                    }
                }
                Selection::Unmatched
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    match select(value, conditions) {
                        Selection::Target(target) if valid_export_target(&target) => {
                            return Selection::Target(target);
                        }
                        _ => {}
                    }
                }
                Selection::Unmatched
            }
            _ => Selection::Blocked,
        }
    }
    fn choose(value: &serde_json::Value, conditions: &[String]) -> Option<String> {
        match select(value, conditions) {
            Selection::Target(target) => Some(target),
            _ => None,
        }
    }
    if let Some(map) = value
        .as_object()
        .filter(|m| m.keys().any(|k| k.starts_with('.')))
    {
        if let Some(value) = map.get(subpath) {
            return choose(value, conditions);
        }
        let mut matches = map
            .iter()
            .filter_map(|(pattern, value)| {
                let (prefix, suffix) = pattern.split_once('*')?;
                let middle = subpath.strip_prefix(prefix)?.strip_suffix(suffix)?;
                Some((prefix.len(), suffix.len(), middle, value))
            })
            .collect::<Vec<_>>();
        matches.sort_by_key(|a| std::cmp::Reverse((a.0, a.1)));
        matches.first().and_then(|(_, _, middle, value)| {
            choose(value, conditions).map(|s| s.replace('*', middle))
        })
    } else if subpath == "." {
        choose(value, conditions)
    } else {
        None
    }
}
pub fn relative(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
}
