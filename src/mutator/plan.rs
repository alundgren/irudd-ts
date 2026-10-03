use crate::{
    mutator::*,
    quality::{
        functions::{candidates, runtime_node},
        load::{digest, issue, load_guarded, location, push_evidence},
        syntax::parse,
        *,
    },
};
use anyhow::Result;
use oxc_ast::AstKind;
use oxc_span::GetSpan;
use std::path::Path;

fn operator_offset(source: &str, start: usize, end: usize, operator: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut cursor = start;
    while cursor < end {
        if bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
            continue;
        }
        if source[cursor..end].starts_with("/*") {
            cursor += 2 + source[cursor + 2..end].find("*/")? + 2;
            continue;
        }
        if source[cursor..end].starts_with("//") {
            cursor += source[cursor..end].find('\n')? + 1;
            continue;
        }
        return source[cursor..end].starts_with(operator).then_some(cursor);
    }
    None
}

pub fn inventory(
    path: &str,
    source: &str,
    config: &MutationPlanConfig,
) -> Result<MutationInventory> {
    Ok(inventory_with_nodes(path, source, config)?.0)
}

fn inventory_with_nodes(
    path: &str,
    source: &str,
    config: &MutationPlanConfig,
) -> Result<(MutationInventory, usize)> {
    config.validate()?;
    let owned = config.clone();
    let path_owned = path.to_owned();
    let parsed = parse(path, source, &config.limits, move |semantic, source| {
        let limits = &owned.limits;
        let owners = candidates(semantic, &path_owned, source, limits);
        let hash = digest(source.as_bytes());
        let mut result = MutationInventory {
            sites: vec![],
            complete: true,
            problems: vec![],
            omitted_evidence: OmittedEvidence::default(),
        };
        let mut used = 0;
        if owners.len() > limits.max_candidates {
            result.complete = false;
            result.problems.push(issue(
                ProblemKind::AnalysisLimit,
                &path_owned,
                0,
                "mutation owner candidate limit reached",
                Some(AnalysisLimitKind::Candidates),
                limits,
            ));
            return result;
        }
        for node in semantic.nodes().iter() {
            if !runtime_node(semantic, node.id()) {
                continue;
            }
            let span = node.span();
            let edit = match node.kind() {
                AstKind::BinaryExpression(binary) => {
                    let before = binary.operator.as_str();
                    let alternative = match before {
                        "<" => Some((MutationOperator::Comparison, "<=")),
                        "<=" => Some((MutationOperator::Comparison, "<")),
                        ">" => Some((MutationOperator::Comparison, ">=")),
                        ">=" => Some((MutationOperator::Comparison, ">")),
                        "==" => Some((MutationOperator::Equality, "!=")),
                        "!=" => Some((MutationOperator::Equality, "==")),
                        "===" => Some((MutationOperator::Equality, "!==")),
                        "!==" => Some((MutationOperator::Equality, "===")),
                        "+" => Some((MutationOperator::Arithmetic, "-")),
                        "-" => Some((MutationOperator::Arithmetic, "+")),
                        "*" => Some((MutationOperator::Arithmetic, "/")),
                        "/" => Some((MutationOperator::Arithmetic, "*")),
                        _ => None,
                    };
                    alternative.map(|(operator, replacement)| {
                        (
                            operator,
                            operator_offset(
                                source,
                                binary.left.span().end as usize,
                                binary.right.span().start as usize,
                                before,
                            ),
                            before,
                            replacement,
                        )
                    })
                }
                AstKind::LogicalExpression(logical) => {
                    let before = logical.operator.as_str();
                    let replacement = match before {
                        "&&" => Some("||"),
                        "||" => Some("&&"),
                        _ => None,
                    };
                    replacement.map(|replacement| {
                        (
                            MutationOperator::Logical,
                            operator_offset(
                                source,
                                logical.left.span().end as usize,
                                logical.right.span().start as usize,
                                before,
                            ),
                            before,
                            replacement,
                        )
                    })
                }
                AstKind::UpdateExpression(update) => {
                    let before = update.operator.as_str();
                    let (start, end) = if update.prefix {
                        (span.start, update.argument.span().start)
                    } else {
                        (update.argument.span().end, span.end)
                    };
                    Some((
                        MutationOperator::Update,
                        operator_offset(source, start as usize, end as usize, before),
                        before,
                        if before == "++" { "--" } else { "++" },
                    ))
                }
                AstKind::BooleanLiteral(boolean) => Some((
                    MutationOperator::Boolean,
                    Some(span.start as usize),
                    if boolean.value { "true" } else { "false" },
                    if boolean.value { "false" } else { "true" },
                )),
                AstKind::NumericLiteral(_) => {
                    if matches!(semantic.nodes().parent_kind(node.id()), AstKind::UnaryExpression(unary) if unary.operator.as_str() == "-")
                    {
                        continue;
                    }
                    let before = &source[span.start as usize..span.end as usize];
                    let replacement = if MutationOperator::ZeroOne.accepts(before, "1") {
                        Some("1")
                    } else if MutationOperator::ZeroOne.accepts(before, "0") {
                        Some("0")
                    } else {
                        None
                    };
                    replacement.map(|replacement| {
                        (
                            MutationOperator::ZeroOne,
                            Some(span.start as usize),
                            before,
                            replacement,
                        )
                    })
                }
                _ => None,
            };
            let Some((operator, start, expected, replacement)) = edit else {
                continue;
            };
            if !owned.operators.contains(&operator) {
                continue;
            }
            let Some(start) = start else {
                result.complete = false;
                result.problems.push(issue(
                    ProblemKind::UnsupportedSyntax,
                    &path_owned,
                    span.start as usize,
                    "operator token could not be located exactly",
                    None,
                    limits,
                ));
                continue;
            };
            let end = start + expected.len();
            let mut site = MutationSite {
                id: String::new(),
                location: location(&path_owned, source, start, end),
                owner: owners
                    .iter()
                    .find(|(_, owner)| owner.location.start <= start && owner.location.end >= end)
                    .map(|(_, owner)| owner.clone()),
                operator,
                expected: expected.into(),
                replacement: replacement.into(),
                source_sha256: hash.clone(),
            };
            site.id = site.identity();
            let count_limit = result.sites.len() >= limits.max_sites;
            if count_limit || !push_evidence(&mut result.sites, site, &mut used, limits) {
                result.complete = false;
                result.omitted_evidence.sites += 1;
                result.problems.push(issue(
                    if count_limit {
                        ProblemKind::AnalysisLimit
                    } else {
                        ProblemKind::ReportLimit
                    },
                    &path_owned,
                    start,
                    "mutation site collection limit reached",
                    Some(if count_limit {
                        AnalysisLimitKind::Sites
                    } else {
                        AnalysisLimitKind::ReportBytes
                    }),
                    limits,
                ));
                break;
            }
        }
        result.sites.sort_by(|a, b| {
            (a.location.start, &a.replacement).cmp(&(b.location.start, &b.replacement))
        });
        result
    })?;
    let mut result = parsed.value.unwrap_or(MutationInventory {
        sites: vec![],
        complete: false,
        problems: vec![],
        omitted_evidence: OmittedEvidence::default(),
    });
    result.problems.extend(parsed.problems);
    result.complete &= result.problems.is_empty();
    result.validate_with_limits(&config.limits)?;
    Ok((result, parsed.nodes))
}

pub fn plan(root: &Path, config: &MutationPlanConfig) -> Result<MutationPlan> {
    crate::mutator::plan_guarded(root, config, &|| Ok(()))
}

/// Build a mutation plan with caller-controlled cancellation or deadline checks.
///
/// The guard runs during discovery, before and after each bounded file parse, and
/// during final source verification. Its errors stop planning and are returned
/// unchanged. An individual parser call finishes within the configured source
/// limits before the next guard check.
pub fn plan_with_guard(
    root: &Path,
    config: &MutationPlanConfig,
    guard: &(impl Fn() -> Result<()> + Sync),
) -> Result<MutationPlan> {
    guard()?;
    config.validate()?;
    let loaded = load_guarded(root, &config.selection, &config.limits, guard)?;
    let mut result = MutationPlan {
        schema_version: SCHEMA_VERSION,
        operator_version: OPERATOR_VERSION,
        configuration: config.clone(),
        root: loaded.root,
        files: loaded.selection.selected.clone(),
        selection: loaded.selection,
        sites: vec![],
        complete: false,
        problems: loaded.problems,
        omitted_evidence: loaded.omitted,
    };
    let mut sites_used = 0;
    let mut site_limit = None;
    let mut problem_used = result
        .problems
        .iter()
        .map(|p| serde_json::to_vec(p).expect("serializable problem").len() + 1)
        .sum();
    let mut nodes = 0usize;
    'files: for sources in loaded.sources.chunks(config.limits.workers) {
        guard()?;
        let inventories = crate::quality::workers::batch(sources, |file, source| {
            guard()?;
            let inventory = inventory_with_nodes(&file.path, source, config)?;
            guard()?;
            Ok::<_, anyhow::Error>(inventory)
        })?;
        for ((file, _), inventory) in sources.iter().zip(inventories) {
            guard()?;
            let (inventory, count) = inventory?;
            nodes = nodes.saturating_add(count);
            if nodes > config.limits.max_nodes {
                result.problems.push(issue(
                    ProblemKind::AnalysisLimit,
                    &file.path,
                    0,
                    "total syntax node limit reached",
                    Some(AnalysisLimitKind::Nodes),
                    &config.limits,
                ));
                break 'files;
            }
            result.omitted_evidence.sites += inventory.omitted_evidence.sites;
            for problem in inventory.problems {
                if !push_evidence(
                    &mut result.problems,
                    problem,
                    &mut problem_used,
                    &config.limits,
                ) {
                    result.omitted_evidence.problems += 1;
                }
            }
            for site in inventory.sites {
                let count_limit = result.sites.len() >= config.limits.max_sites;
                if count_limit
                    || !push_evidence(&mut result.sites, site, &mut sites_used, &config.limits)
                {
                    result.omitted_evidence.sites += 1;
                    site_limit.get_or_insert(if count_limit {
                        (ProblemKind::AnalysisLimit, AnalysisLimitKind::Sites)
                    } else {
                        (ProblemKind::ReportLimit, AnalysisLimitKind::ReportBytes)
                    });
                }
            }
        }
    }
    if let Some((kind, limit)) = site_limit {
        result.problems.push(issue(
            kind,
            ".",
            0,
            "mutation site collection limit reached",
            Some(limit),
            &config.limits,
        ));
    }
    for file in &result.files {
        guard()?;
        if !crate::quality::load::unchanged(&result.root, file, &config.limits) {
            result.problems.push(issue(
                ProblemKind::ChangedSource,
                &file.path,
                0,
                "selected source bytes changed during analysis",
                None,
                &config.limits,
            ));
        }
    }
    result.complete = result.selection.complete_within_selection
        && result.problems.is_empty()
        && result.omitted_evidence.is_empty();
    fit_report(&mut result);
    result.validate()?;
    Ok(result)
}

fn fit_report(result: &mut MutationPlan) {
    if validate_report_size(result, &result.configuration.limits).is_ok() {
        return;
    }
    result.complete = false;
    result.problems.push(issue(
        ProblemKind::ReportLimit,
        ".",
        0,
        "mutation report byte budget reached",
        Some(AnalysisLimitKind::ReportBytes),
        &result.configuration.limits,
    ));
    while validate_report_size(result, &result.configuration.limits).is_err() {
        if result.sites.pop().is_some() {
            result.omitted_evidence.sites += 1;
        } else if result.problems.len() > 1 {
            result.problems.remove(0);
            result.omitted_evidence.problems += 1;
        } else if result.files.pop().is_some() {
            result.selection.selected.pop();
            result.selection.complete_within_selection = false;
            result.omitted_evidence.selected_sources += 1;
        } else if result.selection.skipped.pop().is_some() {
            result.omitted_evidence.skipped_sources += 1;
        } else {
            break;
        }
    }
}

pub fn apply_edit(source: &str, site: &MutationSite) -> std::result::Result<String, EditError> {
    let fail = |kind, message: &str| EditError {
        kind,
        message: message.into(),
    };
    if site.validate().is_err() {
        return Err(fail(
            InvalidPlanKind::SiteIdentity,
            "mutation site failed structural validation",
        ));
    }
    if digest(source.as_bytes()) != site.source_sha256 {
        return Err(fail(
            InvalidPlanKind::SourceHash,
            "source bytes differ from the planned input",
        ));
    }
    let range = site.location.start..site.location.end;
    if range.end > source.len() {
        return Err(fail(
            InvalidPlanKind::Range,
            "mutation range exceeds source bytes",
        ));
    }
    if !source.is_char_boundary(range.start) || !source.is_char_boundary(range.end) {
        return Err(fail(
            InvalidPlanKind::Utf8Boundary,
            "mutation range divides a UTF-8 character",
        ));
    }
    if source[range.clone()] != site.expected {
        return Err(fail(
            InvalidPlanKind::ExpectedText,
            "mutation range does not contain expected text",
        ));
    }
    let mut changed =
        String::with_capacity(source.len() - site.expected.len() + site.replacement.len());
    changed.push_str(&source[..range.start]);
    changed.push_str(&site.replacement);
    changed.push_str(&source[range.end..]);
    Ok(changed)
}

pub fn validate_mutant(
    path: &str,
    source: &str,
    limits: &AnalysisLimits,
) -> Result<SyntaxValidation> {
    let parsed = parse(path, source, limits, |_, _| ())?;
    let result = if parsed.value.is_some() {
        SyntaxValidation::Valid
    } else if !parsed.problems.is_empty()
        && parsed.problems.iter().all(|p| p.kind == ProblemKind::Parse)
    {
        SyntaxValidation::InvalidSyntax {
            diagnostics: parsed.problems,
        }
    } else {
        SyntaxValidation::Incomplete {
            problems: parsed.problems,
        }
    };
    result.validate_with_limits(limits)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[test]
    fn guarded_regeneration_can_stop_during_source_traversal_and_recover() {
        let root = tempfile::TempDir::new().unwrap();
        for index in 0..8 {
            std::fs::write(root.path().join(format!("{index}.ts")), "const f=()=>true").unwrap();
        }
        let calls = AtomicUsize::new(0);
        let interrupted = plan_with_guard(root.path(), &MutationPlanConfig::default(), &|| {
            anyhow::ensure!(
                calls.fetch_add(1, Ordering::Relaxed) < 5,
                "cancelled regeneration"
            );
            Ok(())
        });
        assert!(interrupted.is_err());
        assert!(calls.load(Ordering::Relaxed) >= 6);
        let corrected =
            plan_with_guard(root.path(), &MutationPlanConfig::default(), &|| Ok(())).unwrap();
        assert!(corrected.complete);
        assert_eq!(corrected.sites.len(), 8);
        assert!(
            plan(root.path(), &MutationPlanConfig::default())
                .unwrap()
                .complete
        );
    }
}
