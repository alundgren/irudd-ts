use crate::{
    config::{Config, Matcher, RuleConfig, RuleKind},
    facts::*,
};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub fn check(project: &ProjectFacts, config: &Config) -> Result<Vec<Diagnostic>> {
    let mut diagnostics = vec![];
    for rule in &config.rules {
        diagnostics.extend(rule.check(project)?);
    }
    diagnostics.sort();
    diagnostics.dedup();
    Ok(diagnostics)
}
impl ProjectRule for RuleConfig {
    fn check(&self, project: &ProjectFacts) -> Result<Vec<Diagnostic>> {
        let files = Matcher::new(&self.files)?;
        let exceptions = Matcher::new(&self.exceptions)?;
        let targets = Matcher::new(&self.targets)?;
        let specifiers = Matcher::new(&self.specifiers)?;
        let origins = Matcher::new(&self.origins)?;
        let selected = |path: &str| files.matches(path) && !exceptions.matches(path);
        let mut out = vec![];
        if self.kind == RuleKind::RequiredFile {
            for pattern in &self.files {
                let matcher = Matcher::new(std::slice::from_ref(pattern))?;
                if !project.files.iter().any(|f| matcher.matches(&f.path)) {
                    out.push(Diagnostic::new(
                        &self.id,
                        ".",
                        0,
                        format!("required source file pattern is absent: {pattern}"),
                    ));
                }
            }
            return Ok(out);
        }
        if self.kind == RuleKind::PackageDependency {
            for package in &project.packages {
                if selected(&package.path) {
                    for dependency in &package.dependencies {
                        if specifiers.matches(dependency) {
                            out.push(Diagnostic::new(
                                &self.id,
                                &package.path,
                                0,
                                format!("package {} must not depend on {dependency}", package.name),
                            ));
                        }
                    }
                }
            }
            return Ok(out);
        }
        if self.kind == RuleKind::UniqueServiceId {
            let mut ids: BTreeMap<&str, Vec<(&FileFacts, &ServiceFact)>> = BTreeMap::new();
            for f in &project.files {
                if selected(&f.path) {
                    for s in &f.services {
                        if let Some(id) = s.identifier.as_deref() {
                            ids.entry(id).or_default().push((f, s));
                        }
                    }
                }
            }
            for (id, services) in ids {
                if services.len() > 1 {
                    for (f, s) in &services {
                        let mut d = Diagnostic::new(
                            &self.id,
                            &f.path,
                            s.offset,
                            format!("service identifier {id} is declared more than once"),
                        );
                        d.evidence = services
                            .iter()
                            .map(|(f, s)| format!("{}:{}", f.path, s.name))
                            .collect();
                        out.push(d);
                    }
                }
            }
            return Ok(out);
        }
        if self.kind == RuleKind::NoCycles {
            for component in components(project, self.include_types) {
                if (component.len() > 1
                    || project.file(&component[0]).is_some_and(|f| {
                        f.imports.iter().any(|e| {
                            e.status == ResolutionStatus::Internal
                                && e.kind != "dynamicImport"
                                && e.target.as_ref() == Some(&f.path)
                                && allowed(e, self.include_types)
                        })
                    }))
                    && let Some(file) = component.iter().find(|f| selected(f))
                {
                    let mut d = Diagnostic::new(&self.id, file, 0, "module dependency cycle");
                    d.evidence = component;
                    out.push(d);
                }
            }
            return Ok(out);
        }
        for file in &project.files {
            if !selected(&file.path) {
                continue;
            }
            match self.kind {
                RuleKind::ForbiddenImport => {
                    for e in &file.imports {
                        if (self.include_types || !e.type_only)
                            && e.specifier
                                .as_deref()
                                .is_some_and(|s| specifiers.matches(s))
                        {
                            out.push(Diagnostic::new(
                                &self.id,
                                &file.path,
                                e.offset,
                                format!(
                                    "forbidden import {}",
                                    e.specifier.as_deref().unwrap_or_default()
                                ),
                            ));
                        }
                    }
                }
                RuleKind::ForbiddenCall => {
                    for c in &file.calls {
                        if c.origin.as_deref().is_some_and(|s| origins.matches(s)) {
                            out.push(Diagnostic::new(
                                &self.id,
                                &file.path,
                                c.offset,
                                format!(
                                    "forbidden imported call {}",
                                    c.origin.as_deref().unwrap_or_default()
                                ),
                            ));
                        }
                    }
                }
                RuleKind::ForbiddenDependency => {
                    for (target, offset, evidence) in
                        dependencies(project, file, self.include_types, self.transitive)
                    {
                        if targets.matches(&target) {
                            let mut d = Diagnostic::new(
                                &self.id,
                                &file.path,
                                offset,
                                format!("forbidden dependency on {target}"),
                            );
                            d.evidence = evidence;
                            out.push(d);
                        }
                    }
                }
                RuleKind::PublicEntry => {
                    for e in &file.imports {
                        if !self.include_types && e.type_only {
                            continue;
                        }
                        if let Some(target) = &e.target
                            && targets.matches(target)
                            && !specifiers.matches(e.specifier.as_deref().unwrap_or_default())
                        {
                            out.push(Diagnostic::new(
                                &self.id,
                                &file.path,
                                e.offset,
                                format!(
                                    "import {target} through its declared public package entry"
                                ),
                            ));
                        }
                    }
                }
                RuleKind::ServiceNamespace => {
                    for e in &file.imports {
                        if e.type_only || e.kind != "import" {
                            continue;
                        }
                        let Some(target) = e.target.as_deref().and_then(|t| project.file(t)) else {
                            continue;
                        };
                        for binding in &e.bindings {
                            if binding.type_only || binding.imported == "*" {
                                continue;
                            }
                            if service_export(
                                project,
                                target,
                                &binding.imported,
                                &mut BTreeSet::new(),
                            ) {
                                let mut d = Diagnostic::new(
                                    &self.id,
                                    &file.path,
                                    e.offset,
                                    format!(
                                        "import service module {} as a namespace instead of {}",
                                        target.path, binding.imported
                                    ),
                                );
                                d.evidence = vec![target.path.clone()];
                                out.push(d);
                            }
                        }
                    }
                }
                RuleKind::ServiceLayer => {
                    if !file.services.is_empty() && !file.exports.iter().any(|e| e.name == "layer")
                    {
                        out.push(Diagnostic::new(
                            &self.id,
                            &file.path,
                            file.services[0].offset,
                            "service implementation must export layer",
                        ));
                    }
                }
                RuleKind::RequiredExport => {
                    for name in &self.names {
                        let origins = export_origins(
                            project,
                            file,
                            name,
                            self.include_types,
                            &mut BTreeSet::new(),
                        );
                        if origins.len() != 1 {
                            out.push(Diagnostic::new(
                                &self.id,
                                &file.path,
                                0,
                                format!(
                                    "required export {name} is {}",
                                    if origins.is_empty() {
                                        "absent"
                                    } else {
                                        "ambiguous"
                                    }
                                ),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(out)
    }
}
fn export_origins(
    project: &ProjectFacts,
    file: &FileFacts,
    name: &str,
    include_types: bool,
    active: &mut BTreeSet<(String, String)>,
) -> BTreeSet<(String, String)> {
    let key = (file.path.clone(), name.to_owned());
    if !active.insert(key.clone()) {
        return BTreeSet::new();
    }
    let result = (|| {
        let explicit = file
            .exports
            .iter()
            .filter(|e| e.name == name)
            .collect::<Vec<_>>();
        let mut origins = BTreeSet::new();
        if !explicit.is_empty() {
            for export in explicit {
                if export.type_only && !include_types {
                    continue;
                }
                if let Some(local) = &export.local {
                    let imported = file
                        .imports
                        .iter()
                        .filter(|e| e.kind == "import" || e.kind == "importEquals")
                        .flat_map(|e| e.bindings.iter().map(move |b| (e, b)))
                        .find(|(_, b)| &b.local == local);
                    if let Some((edge, binding)) = imported {
                        if binding.type_only && !include_types {
                            continue;
                        }
                        if let Some(target) = edge.target.as_deref().and_then(|p| project.file(p)) {
                            if binding.imported == "*" {
                                origins.insert((target.path.clone(), "*".into()));
                            } else {
                                origins.extend(export_origins(
                                    project,
                                    target,
                                    &binding.imported,
                                    include_types,
                                    active,
                                ));
                            }
                        }
                    } else {
                        origins.insert((file.path.clone(), local.clone()));
                    }
                } else {
                    for edge in &file.imports {
                        if edge.type_only && !include_types {
                            continue;
                        }
                        let Some(target) = edge.target.as_deref().and_then(|p| project.file(p))
                        else {
                            continue;
                        };
                        for binding in &edge.bindings {
                            if binding.local == name && (include_types || !binding.type_only) {
                                if binding.imported == "*" {
                                    origins.insert((target.path.clone(), "*".into()));
                                } else {
                                    origins.extend(export_origins(
                                        project,
                                        target,
                                        &binding.imported,
                                        include_types,
                                        active,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            return origins;
        }
        if name != "default" {
            for edge in &file.imports {
                if edge.kind == "reExportAll"
                    && (include_types || !edge.type_only)
                    && let Some(target) = edge.target.as_deref().and_then(|p| project.file(p))
                {
                    origins.extend(export_origins(project, target, name, include_types, active));
                }
            }
        }
        origins
    })();
    active.remove(&key);
    result
}
fn service_export(
    project: &ProjectFacts,
    file: &FileFacts,
    name: &str,
    _seen: &mut BTreeSet<(String, String)>,
) -> bool {
    let origins = export_origins(project, file, name, false, &mut BTreeSet::new());
    if origins.len() != 1 {
        return false;
    }
    origins.iter().any(|(path, local)| {
        project.file(path).is_some_and(|target| {
            target.services.iter().any(|s| &s.name == local)
                || (!target.services.is_empty() && matches!(local.as_str(), "layer" | "make"))
        })
    })
}
fn allowed(edge: &ImportFact, include_types: bool) -> bool {
    edge.status == ResolutionStatus::Internal && (include_types || !edge.type_only)
}
fn dependencies(
    project: &ProjectFacts,
    start: &FileFacts,
    include_types: bool,
    transitive: bool,
) -> Vec<(String, usize, Vec<String>)> {
    let mut seen = BTreeSet::from([start.path.clone()]);
    let mut queue = VecDeque::new();
    let mut out = vec![];
    for e in &start.imports {
        if allowed(e, include_types)
            && let Some(t) = &e.target
        {
            queue.push_back((t.clone(), e.offset, vec![start.path.clone(), t.clone()]));
        }
    }
    while let Some((target, offset, evidence)) = queue.pop_front() {
        if !seen.insert(target.clone()) {
            continue;
        }
        out.push((target.clone(), offset, evidence.clone()));
        if transitive && let Some(f) = project.file(&target) {
            for e in &f.imports {
                if allowed(e, include_types)
                    && let Some(t) = &e.target
                {
                    let mut next = evidence.clone();
                    next.push(t.clone());
                    queue.push_back((t.clone(), offset, next));
                }
            }
        }
    }
    out
}
fn components(project: &ProjectFacts, include_types: bool) -> Vec<Vec<String>> {
    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut reverse: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in &project.files {
        graph.entry(f.path.clone()).or_default();
        reverse.entry(f.path.clone()).or_default();
        for e in &f.imports {
            if allowed(e, include_types)
                && e.kind != "dynamicImport"
                && let Some(t) = &e.target
            {
                graph.entry(f.path.clone()).or_default().push(t.clone());
                reverse.entry(t.clone()).or_default().push(f.path.clone());
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut order = vec![];
    for start in graph.keys() {
        let mut stack = vec![(start.clone(), false)];
        while let Some((node, expanded)) = stack.pop() {
            if expanded {
                order.push(node);
                continue;
            }
            if !seen.insert(node.clone()) {
                continue;
            }
            stack.push((node.clone(), true));
            if let Some(next) = graph.get(&node) {
                for n in next.iter().rev() {
                    if !seen.contains(n) {
                        stack.push((n.clone(), false));
                    }
                }
            }
        }
    }
    seen.clear();
    let mut result = vec![];
    for start in order.into_iter().rev() {
        if seen.contains(&start) {
            continue;
        }
        let mut group = vec![];
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            if !seen.insert(node.clone()) {
                continue;
            }
            group.push(node.clone());
            if let Some(next) = reverse.get(&node) {
                stack.extend(next.iter().cloned());
            }
        }
        group.sort();
        result.push(group);
    }
    result.sort();
    result
}
