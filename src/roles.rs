//! Repository conventions are explicit policy over source facts, not inferred types.
use crate::{config::Matcher, facts::*, rules::dependencies};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryPolicy {
    pub roles: Vec<FileRole>,
    pub rules: Vec<RepositoryRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileRole {
    pub id: String,
    pub files: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RepositoryRule {
    Classified {
        id: String,
        files: Vec<String>,
        #[serde(default)]
        exclude: Vec<String>,
    },
    ForbiddenDependency {
        id: String,
        from: String,
        to: String,
        #[serde(default)]
        transitive: bool,
        #[serde(default = "yes")]
        include_types: bool,
    },
    Companion {
        id: String,
        role: String,
        replace: [String; 2],
    },
    RegistryImport {
        id: String,
        role: String,
        registry: String,
    },
}
fn yes() -> bool {
    true
}
impl RepositoryRule {
    fn id(&self) -> &str {
        match self {
            Self::Classified { id, .. }
            | Self::ForbiddenDependency { id, .. }
            | Self::Companion { id, .. }
            | Self::RegistryImport { id, .. } => id,
        }
    }
}

impl RepositoryPolicy {
    pub fn validate<'a>(&'a self, ids: &mut BTreeSet<&'a str>) -> Result<()> {
        let mut roles = BTreeSet::new();
        for role in &self.roles {
            if role.id.trim().is_empty() || !roles.insert(role.id.as_str()) || role.files.is_empty()
            {
                bail!(
                    "empty or duplicate repository role, or empty selector: {}",
                    role.id
                );
            }
            Matcher::new(&role.files)?;
            Matcher::new(&role.exclude)?;
        }
        for rule in &self.rules {
            if rule.id().trim().is_empty() || !ids.insert(rule.id()) {
                bail!("empty or duplicate rule id {}", rule.id());
            }
            let selected_roles = match rule {
                RepositoryRule::Classified { files, exclude, .. } => {
                    if files.is_empty() {
                        bail!("{} requires classification selectors", rule.id());
                    }
                    Matcher::new(files)?;
                    Matcher::new(exclude)?;
                    vec![]
                }
                RepositoryRule::ForbiddenDependency { from, to, .. } => vec![from, to],
                RepositoryRule::Companion { role, replace, .. } => {
                    if replace[0].is_empty() || replace[1].is_empty() || replace[0] == replace[1] {
                        bail!(
                            "{} requires a nonempty, distinct path replacement",
                            rule.id()
                        );
                    }
                    vec![role]
                }
                RepositoryRule::RegistryImport { role, registry, .. } => {
                    if !source_path(registry) {
                        bail!("{} requires a relative registry source path", rule.id());
                    }
                    vec![role]
                }
            };
            for role in selected_roles {
                if !roles.contains(role.as_str()) {
                    bail!("{} references unknown repository role {role}", rule.id());
                }
            }
        }
        Ok(())
    }

    /// Every analyzed source remains visible, including unassigned and overlapping roles.
    pub fn assignments(&self, project: &ProjectFacts) -> Result<BTreeMap<String, Vec<String>>> {
        self.validate(&mut BTreeSet::new())?;
        let selectors = self
            .roles
            .iter()
            .map(|role| {
                Ok((
                    &role.id,
                    Matcher::new(&role.files)?,
                    Matcher::new(&role.exclude)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(project
            .files
            .iter()
            .map(|file| {
                let roles = selectors
                    .iter()
                    .filter(|(_, files, exclude)| {
                        files.matches(&file.path) && !exclude.matches(&file.path)
                    })
                    .map(|(id, _, _)| (*id).clone())
                    .collect();
                (file.path.clone(), roles)
            })
            .collect())
    }
}

fn source_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.contains(':')
        && !path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
}

impl ProjectRule for RepositoryPolicy {
    fn check(&self, project: &ProjectFacts) -> Result<Vec<Diagnostic>> {
        let assignments = self.assignments(project)?;
        let members: BTreeMap<&str, BTreeSet<&str>> = self
            .roles
            .iter()
            .map(|role| {
                (
                    role.id.as_str(),
                    assignments
                        .iter()
                        .filter(|(_, roles)| roles.contains(&role.id))
                        .map(|(path, _)| path.as_str())
                        .collect(),
                )
            })
            .collect();
        let mut out = vec![];
        for rule in &self.rules {
            match rule {
                RepositoryRule::Classified { files, exclude, .. } => {
                    let files = Matcher::new(files)?;
                    let exclude = Matcher::new(exclude)?;
                    for (path, roles) in &assignments {
                        if files.matches(path) && !exclude.matches(path) && roles.len() != 1 {
                            let mut d = Diagnostic::new(
                                rule.id(),
                                path,
                                0,
                                format!(
                                    "expected exactly one repository role, found {}",
                                    roles.len()
                                ),
                            );
                            d.evidence = roles.clone();
                            out.push(d);
                        }
                    }
                }
                RepositoryRule::ForbiddenDependency {
                    from,
                    to,
                    transitive,
                    include_types,
                    ..
                } => {
                    for path in &members[from.as_str()] {
                        let file = project
                            .file(path)
                            .expect("assignment is an analyzed source");
                        for (target, offset, evidence) in
                            dependencies(project, file, *include_types, *transitive)
                        {
                            if members[to.as_str()].contains(target.as_str()) {
                                let mut d = Diagnostic::new(
                                    rule.id(),
                                    path,
                                    offset,
                                    format!("role {from} must not depend on role {to}: {target}"),
                                );
                                d.evidence = evidence;
                                out.push(d);
                            }
                        }
                    }
                }
                RepositoryRule::Companion { role, replace, .. } => {
                    for path in &members[role.as_str()] {
                        if path.matches(&replace[0]).count() != 1 {
                            bail!(
                                "{}: replacement must occur exactly once in {path}",
                                rule.id()
                            );
                        }
                        let target = path.replacen(&replace[0], &replace[1], 1);
                        if !source_path(&target) {
                            bail!(
                                "{}: companion is not a relative source path: {target}",
                                rule.id()
                            );
                        }
                        if project.file(&target).is_none() {
                            let mut d = Diagnostic::new(
                                rule.id(),
                                path,
                                0,
                                format!("required analyzed companion is absent: {target}"),
                            );
                            d.evidence = vec![(*path).into(), target];
                            out.push(d);
                        }
                    }
                }
                RepositoryRule::RegistryImport { role, registry, .. } => {
                    let Some(file) = project.file(registry) else {
                        out.push(Diagnostic::new(
                            rule.id(),
                            ".",
                            0,
                            format!("required analyzed registry is absent: {registry}"),
                        ));
                        continue;
                    };
                    let registered: BTreeSet<&str> = file
                        .imports
                        .iter()
                        .filter(|edge| {
                            edge.status == ResolutionStatus::Internal
                                && !edge.type_only
                                && matches!(edge.kind.as_str(), "import" | "importEquals")
                        })
                        .filter_map(|edge| edge.target.as_deref())
                        .collect();
                    for path in &members[role.as_str()] {
                        if !registered.contains(path) {
                            let mut d = Diagnostic::new(
                                rule.id(),
                                path,
                                0,
                                format!(
                                    "role {role} requires a direct static value import in {registry}"
                                ),
                            );
                            d.evidence = vec![registry.clone(), (*path).into()];
                            out.push(d);
                        }
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        Ok(out)
    }
}
