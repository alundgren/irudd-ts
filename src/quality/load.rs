use crate::{config::Matcher, quality::*};
use anyhow::Result;
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

pub(crate) struct LoadedSources {
    pub root: String,
    pub selection: SelectionReport,
    pub sources: Vec<(SourceFile, String)>,
    pub problems: Vec<AnalysisProblem>,
    pub omitted: OmittedEvidence,
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(crate) fn truncate(text: &str, maximum: usize) -> (String, bool) {
    let mut end = text.len().min(maximum);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].into(), end < text.len())
}

pub(crate) fn issue(
    kind: ProblemKind,
    file: &str,
    offset: usize,
    message: &str,
    limit: Option<AnalysisLimitKind>,
    limits: &AnalysisLimits,
) -> AnalysisProblem {
    let (message, message_truncated) = truncate(message, limits.max_problem_bytes);
    AnalysisProblem {
        kind,
        file: file.into(),
        offset,
        message,
        message_truncated,
        limit,
    }
}

pub(crate) fn location(file: &str, source: &str, start: usize, end: usize) -> SourceLocation {
    SourceLocation {
        file: file.into(),
        start,
        end,
        line: source[..start].bytes().filter(|b| *b == b'\n').count() + 1,
        end_line: source[..end].bytes().filter(|b| *b == b'\n').count() + 1,
    }
}

// Each collection reserves most of the report for the other collections. The final
// renderer checks the entire document too; this prevents collection growth first.
pub(crate) fn push_evidence<T: serde::Serialize>(
    items: &mut Vec<T>,
    item: T,
    used: &mut usize,
    limits: &AnalysisLimits,
) -> bool {
    let maximum = limits.max_report_bytes / 8;
    let Ok(bytes) = encoded_size(&item, maximum.saturating_sub(*used)) else {
        return false;
    };
    if used.saturating_add(bytes + 1) > maximum {
        return false;
    }
    *used += bytes + 1;
    items.push(item);
    true
}

pub(crate) fn load(
    root: &Path,
    selection: &SourceSelection,
    limits: &AnalysisLimits,
) -> Result<LoadedSources> {
    load_guarded(root, selection, limits, &|| Ok(()))
}

pub(crate) fn load_guarded(
    root: &Path,
    selection: &SourceSelection,
    limits: &AnalysisLimits,
    guard: &(impl Fn() -> Result<()> + Sync),
) -> Result<LoadedSources> {
    guard()?;
    selection.validate()?;
    limits.validate()?;
    let root = root.canonicalize()?;
    anyhow::ensure!(root.is_dir(), "analysis root must be a directory");
    let root_text = root
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("root path is not UTF-8"))?
        .to_owned();
    anyhow::ensure!(
        root_text.len() <= MAX_PATH_BYTES,
        "root path exceeds its byte limit"
    );
    let include = Matcher::new(&selection.include)?;
    let exclude = Matcher::new(&selection.exclude)?;
    let mut result = LoadedSources {
        root: root_text,
        selection: SelectionReport {
            requested: selection.clone(),
            selected: vec![],
            skipped: vec![],
            complete_within_selection: true,
        },
        sources: vec![],
        problems: vec![],
        omitted: OmittedEvidence::default(),
    };
    let mut selected_used = 0;
    let mut skipped_used = 0;
    let mut problem_used = 0;
    let mut paths = vec![];
    let mut add_problem = |result: &mut LoadedSources, problem| {
        result.selection.complete_within_selection = false;
        if !push_evidence(&mut result.problems, problem, &mut problem_used, limits) {
            result.omitted.problems += 1;
        }
    };
    let walker = WalkBuilder::new(&root)
        .standard_filters(false)
        .follow_links(false)
        .filter_entry(|entry| {
            entry.depth() == 0
                || !matches!(
                    entry.file_name().to_str(),
                    Some(".git" | "node_modules" | "target")
                )
        })
        .build();
    for (count, entry) in walker.enumerate() {
        guard()?;
        if count >= limits.max_discovery_entries {
            add_problem(
                &mut result,
                issue(
                    ProblemKind::AnalysisLimit,
                    ".",
                    0,
                    "source discovery entry limit reached",
                    Some(AnalysisLimitKind::DiscoveryEntries),
                    limits,
                ),
            );
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                add_problem(
                    &mut result,
                    issue(
                        ProblemKind::Traversal,
                        ".",
                        0,
                        &error.to_string(),
                        None,
                        limits,
                    ),
                );
                continue;
            }
        };
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            continue;
        }
        let relative = entry.path().strip_prefix(&root)?;
        let Some(path) = relative.to_str() else {
            add_problem(
                &mut result,
                issue(
                    ProblemKind::Traversal,
                    ".",
                    0,
                    "source path is not UTF-8",
                    None,
                    limits,
                ),
            );
            continue;
        };
        if entry.file_type().is_some_and(|kind| kind.is_symlink())
            && std::fs::metadata(entry.path()).map_or(true, |metadata| metadata.is_dir())
        {
            let directory = format!("{path}/");
            let may_select = selection.include.iter().any(|pattern| {
                let prefix =
                    &pattern[..pattern.find(['*', '?', '[', '{']).unwrap_or(pattern.len())];
                prefix.starts_with(&directory) || directory.starts_with(prefix)
            });
            let excluded_subtree = selection
                .exclude
                .iter()
                .filter(|pattern| pattern.ends_with("/**"))
                .any(|pattern| {
                    Matcher::new(std::slice::from_ref(pattern))
                        .is_ok_and(|matcher| matcher.matches(&directory))
                });
            if may_select && !excluded_subtree {
                add_problem(
                    &mut result,
                    issue(
                        ProblemKind::Traversal,
                        ".",
                        0,
                        "symbolic link may contain selected source descendants",
                        None,
                        limits,
                    ),
                );
            }
            continue;
        }
        if !include.matches(path) {
            continue;
        }
        if validate_relative_path(path).is_err() {
            add_problem(
                &mut result,
                issue(
                    ProblemKind::Traversal,
                    ".",
                    0,
                    "selected path is not canonical or exceeds its byte limit",
                    None,
                    limits,
                ),
            );
            continue;
        }
        let skip = if exclude.matches(path) {
            Some(SkipReason::ConfiguredExclusion)
        } else if path.ends_with(".d.ts") || path.ends_with(".d.mts") || path.ends_with(".d.cts") {
            Some(SkipReason::DeclarationOnly)
        } else if !is_source_path(path) {
            Some(SkipReason::UnsupportedLanguage)
        } else {
            None
        };
        if let Some(reason) = skip {
            if reason == SkipReason::UnsupportedLanguage {
                add_problem(
                    &mut result,
                    issue(
                        ProblemKind::UnsupportedLanguage,
                        path,
                        0,
                        "requested source language is unsupported",
                        None,
                        limits,
                    ),
                );
            }
            if !push_evidence(
                &mut result.selection.skipped,
                SkippedSource {
                    path: path.into(),
                    reason,
                },
                &mut skipped_used,
                limits,
            ) {
                result.omitted.skipped_sources += 1;
                result.selection.complete_within_selection = false;
            }
            continue;
        }
        if entry.file_type().is_some_and(|kind| kind.is_symlink()) {
            add_problem(
                &mut result,
                issue(
                    ProblemKind::Read,
                    path,
                    0,
                    "selected symbolic link is unsupported",
                    None,
                    limits,
                ),
            );
            continue;
        }
        if paths.len() >= limits.max_files {
            result.omitted.selected_sources += 1;
            add_problem(
                &mut result,
                issue(
                    ProblemKind::AnalysisLimit,
                    path,
                    0,
                    "selected file limit reached",
                    Some(AnalysisLimitKind::Files),
                    limits,
                ),
            );
            break;
        }
        paths.push(path.to_owned());
    }
    paths.sort();
    let mut total = 0usize;
    for path in paths {
        guard()?;
        let read = (|| -> std::io::Result<Vec<u8>> {
            let full = root.join(&path);
            if full.canonicalize()? != full {
                return Err(std::io::Error::other(
                    "selected source path resolves through a symbolic link",
                ));
            }
            let metadata = std::fs::symlink_metadata(&full)?;
            if !metadata.is_file() {
                return Err(std::io::Error::other(
                    "selected path is no longer a regular file",
                ));
            }
            let mut bytes = vec![];
            File::open(full)?
                .take((limits.max_file_bytes + 1) as u64)
                .read_to_end(&mut bytes)?;
            Ok(bytes)
        })();
        let bytes = match read {
            Ok(bytes) => bytes,
            Err(error) => {
                add_problem(
                    &mut result,
                    issue(
                        ProblemKind::Read,
                        &path,
                        0,
                        &error.to_string(),
                        None,
                        limits,
                    ),
                );
                continue;
            }
        };
        let over = if bytes.len() > limits.max_file_bytes {
            Some(AnalysisLimitKind::FileBytes)
        } else if total.saturating_add(bytes.len()) > limits.max_total_bytes {
            Some(AnalysisLimitKind::TotalBytes)
        } else {
            None
        };
        if let Some(limit) = over {
            add_problem(
                &mut result,
                issue(
                    ProblemKind::AnalysisLimit,
                    &path,
                    0,
                    "selected source byte limit reached",
                    Some(limit),
                    limits,
                ),
            );
            continue;
        }
        let file = SourceFile {
            path: path.clone(),
            bytes: bytes.len(),
            sha256: digest(&bytes),
        };
        let source = match String::from_utf8(bytes) {
            Ok(source) => source,
            Err(_) => {
                add_problem(
                    &mut result,
                    issue(
                        ProblemKind::InvalidUtf8,
                        &path,
                        0,
                        "selected source is not UTF-8",
                        None,
                        limits,
                    ),
                );
                continue;
            }
        };
        if !push_evidence(
            &mut result.selection.selected,
            file.clone(),
            &mut selected_used,
            limits,
        ) {
            result.omitted.selected_sources += 1;
            add_problem(
                &mut result,
                issue(
                    ProblemKind::ReportLimit,
                    &path,
                    0,
                    "source evidence byte budget reached",
                    Some(AnalysisLimitKind::ReportBytes),
                    limits,
                ),
            );
            break;
        }
        total += source.len();
        result.sources.push((file, source));
    }
    result.selection.skipped.sort_by(|a, b| a.path.cmp(&b.path));
    if result.sources.is_empty() {
        add_problem(
            &mut result,
            issue(
                ProblemKind::Read,
                ".",
                0,
                "selection has no readable supported sources",
                None,
                limits,
            ),
        );
    }
    Ok(result)
}

pub(crate) fn unchanged(root: &str, file: &SourceFile, limits: &AnalysisLimits) -> bool {
    let path = Path::new(root).join(&file.path);
    let Ok(metadata) = std::fs::symlink_metadata(&path) else {
        return false;
    };
    if !metadata.is_file()
        || path
            .canonicalize()
            .map_or(true, |canonical| canonical != path)
    {
        return false;
    }
    let Ok(mut handle) = File::open(path) else {
        return false;
    };
    let mut bytes = vec![];
    if (&mut handle)
        .take((limits.max_file_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return false;
    }
    bytes.len() == file.bytes && digest(&bytes) == file.sha256
}
