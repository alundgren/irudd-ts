use crate::{
    dryer::{
        facts::{NodeKey, NormalizedFunction},
        *,
    },
    quality::{
        load::{digest, issue, load, push_evidence},
        syntax::PARSER_VERSION,
        *,
    },
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheEntry {
    file: SourceFile,
    functions: Vec<NormalizedFunction>,
    nodes: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachePayload {
    version: u32,
    identity: String,
    root: String,
    entries: Vec<CacheEntry>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheDocument {
    sha256: String,
    payload: CachePayload,
}
static TEMP_ID: AtomicU64 = AtomicU64::new(0);
static EXECUTABLE_ID: OnceLock<String> = OnceLock::new();
static EXECUTABLE_ID_LOCK: Mutex<()> = Mutex::new(());

pub fn analyze(root: &Path, config: &DryerConfig) -> Result<DryerReport> {
    analyze_inner(root, config, None)
}
pub fn analyze_cached(root: &Path, config: &DryerConfig, cache: &Path) -> Result<DryerReport> {
    analyze_inner(root, config, Some(cache))
}

fn executable_digest() -> Result<String> {
    if let Some(identity) = EXECUTABLE_ID.get() {
        return Ok(identity.clone());
    }
    let _guard = EXECUTABLE_ID_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(identity) = EXECUTABLE_ID.get() {
        return Ok(identity.clone());
    }
    let mut file = File::open(std::env::current_exe()?)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let identity: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let _ = EXECUTABLE_ID.set(identity.clone());
    Ok(identity)
}
fn cache_identity(config: &DryerConfig) -> Result<String> {
    Ok(digest(&serde_json::to_vec(&(
        PARSER_VERSION,
        NORMALIZATION_VERSION,
        &config.normalization,
        &config.limits,
        executable_digest()?,
    ))?))
}
fn validate_entry(entry: &CacheEntry, limits: &AnalysisLimits) -> Result<()> {
    entry.file.validate()?;
    ensure!(
        entry.file.bytes <= limits.max_file_bytes
            && entry.nodes <= limits.max_nodes_per_file
            && entry.functions.len() <= limits.max_candidates,
        "cached inventory exceeds limits"
    );
    let mut total_nodes = 0usize;
    let mut total_scalars = 0usize;
    for function in &entry.functions {
        function.facts.function.location.validate()?;
        ensure!(
            function.facts.function.location.file == entry.file.path
                && function.facts.function.location.end <= entry.file.bytes,
            "cached function has an invalid source range"
        );
        ensure!(
            !function.facts.function.name.is_empty()
                && function.facts.function.name.len() <= limits.max_name_bytes,
            "cached function has an invalid display name"
        );
        ensure!(
            !function.tree.is_empty()
                && function.tree.len() == function.facts.nodes
                && function.facts.opaque_nodes <= function.tree.len(),
            "cached function node counts disagree"
        );
        total_nodes = total_nodes.saturating_add(function.tree.len());
        let mut parents = vec![0usize; function.tree.len()];
        let mut opaque = 0usize;
        for (index, node) in function.tree.iter().enumerate() {
            total_scalars = total_scalars.saturating_add(node.scalar.len());
            ensure!(
                node.kind.len() <= 128
                    && !node.kind.is_empty()
                    && node.children.iter().all(|child| *child < index),
                "cached tree is not valid postorder data"
            );
            if node.kind.starts_with("Opaque:") {
                opaque += 1;
                ensure!(
                    node.children.is_empty(),
                    "cached opaque nodes must be leaves"
                );
            }
            for child in &node.children {
                parents[*child] += 1;
            }
        }
        ensure!(
            parents[..parents.len() - 1].iter().all(|count| *count == 1)
                && parents[parents.len() - 1] == 0
                && opaque == function.facts.opaque_nodes,
            "cached tree is disconnected or its opaque count disagrees"
        );
    }
    ensure!(
        total_nodes <= entry.nodes && total_scalars <= limits.max_file_bytes * 8,
        "cached normalized inventory exceeds source bounds"
    );
    Ok(())
}
fn read_cache(
    path: &Path,
    identity: &str,
    root: &str,
    limits: &AnalysisLimits,
) -> Result<CachePayload> {
    ensure!(
        fs::symlink_metadata(path)?.is_file(),
        "dryer cache must be a regular file"
    );
    let mut bytes = vec![];
    File::open(path)?
        .take((limits.max_report_bytes + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limits.max_report_bytes,
        "dryer cache exceeds its encoded byte limit"
    );
    let document: CacheDocument = serde_json::from_slice(&bytes)?;
    ensure!(
        digest(&serde_json::to_vec(&document.payload)?) == document.sha256,
        "dryer cache checksum mismatch"
    );
    if document.payload.version != 1
        || document.payload.identity != identity
        || document.payload.root != root
    {
        return Ok(CachePayload {
            version: 1,
            identity: identity.into(),
            root: root.into(),
            entries: vec![],
        });
    }
    ensure!(
        document.payload.entries.len() <= limits.max_files,
        "dryer cache exceeds selected-file limit"
    );
    let mut previous: Option<&str> = None;
    for entry in &document.payload.entries {
        validate_entry(entry, limits)?;
        ensure!(
            previous.is_none_or(|path| path < entry.file.path.as_str()),
            "dryer cache entries must be sorted and unique"
        );
        previous = Some(&entry.file.path);
    }
    Ok(document.payload)
}
fn write_cache(path: &Path, payload: CachePayload, limits: &AnalysisLimits) -> Result<()> {
    let checksum = digest(&serde_json::to_vec(&payload)?);
    let document = CacheDocument {
        sha256: checksum,
        payload,
    };
    validate_report_size(&document, limits)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    ensure!(
        !fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_file()),
        "dryer cache must be a regular file"
    );
    let temporary = parent.join(format!(
        ".archguard-dryer-{}-{}.tmp",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let outcome = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        serde_json::to_writer(&mut file, &document)?;
        file.flush()?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(temporary);
    }
    outcome
}

struct Bag {
    facts: FunctionFacts,
    root: usize,
    counts: BTreeMap<usize, u64>,
}
fn intern<S: std::hash::BuildHasher>(
    function: NormalizedFunction,
    interner: &mut HashMap<NodeKey, usize, S>,
    weights: &mut Vec<u64>,
) -> Bag {
    let mut local = vec![];
    let mut counts = BTreeMap::new();
    for mut key in function.tree {
        for child in &mut key.children {
            *child = local[*child];
        }
        let weight = key
            .children
            .iter()
            .fold(1u64, |sum, child| sum.saturating_add(weights[*child]));
        let next = interner.len();
        let global = *interner.entry(key).or_insert_with(|| {
            weights.push(weight);
            next
        });
        local.push(global);
        *counts.entry(global).or_insert(0) += 1;
    }
    Bag {
        facts: function.facts,
        root: *local.last().expect("function root"),
        counts,
    }
}
fn similarity(left: &Bag, right: &Bag, weights: &[u64]) -> SimilarityValues {
    let mut distinct_intersection = 0u64;
    let mut distinct_union = 0u64;
    let mut count_intersection = 0u64;
    let mut count_union = 0u64;
    let mut weighted_intersection = 0u64;
    let mut weighted_union = 0u64;
    let mut left = left.counts.iter().peekable();
    let mut right = right.counts.iter().peekable();
    while left.peek().is_some() || right.peek().is_some() {
        let (key, a, b) = match (left.peek(), right.peek()) {
            (Some((a, _)), Some((b, _))) if a == b => {
                let (key, a) = left.next().unwrap();
                let (_, b) = right.next().unwrap();
                (*key, *a, *b)
            }
            (Some((a, _)), Some((b, _))) if a < b => {
                let (key, a) = left.next().unwrap();
                (*key, *a, 0)
            }
            (Some(_), None) => {
                let (key, a) = left.next().unwrap();
                (*key, *a, 0)
            }
            _ => {
                let (key, b) = right.next().unwrap();
                (*key, 0, *b)
            }
        };
        distinct_union += 1;
        if a > 0 && b > 0 {
            distinct_intersection += 1;
        }
        count_intersection += a.min(b);
        count_union += a.max(b);
        weighted_intersection += a.min(b) * weights[key];
        weighted_union += a.max(b) * weights[key];
    }
    SimilarityValues {
        set: distinct_intersection as f64 / distinct_union.max(1) as f64,
        multiset: count_intersection as f64 / count_union.max(1) as f64,
        weighted: weighted_intersection as f64 / weighted_union.max(1) as f64,
    }
}

fn analyze_inner(root: &Path, config: &DryerConfig, cache: Option<&Path>) -> Result<DryerReport> {
    config.validate()?;
    let started = Instant::now();
    let loaded = load(root, &config.selection, &config.limits)?;
    let mut result = DryerReport {
        schema_version: SCHEMA_VERSION,
        normalization_version: NORMALIZATION_VERSION,
        parser_version: PARSER_VERSION.into(),
        root: loaded.root.clone(),
        configuration: config.clone(),
        selection: loaded.selection,
        files: vec![],
        complete: false,
        functions: vec![],
        excluded: vec![],
        pairs: vec![],
        problems: loaded.problems,
        omitted_evidence: loaded.omitted,
        elapsed_ms: 0.0,
    };
    result.files = result.selection.selected.clone();
    let mut cache_payload = CachePayload {
        version: 1,
        identity: String::new(),
        root: result.root.clone(),
        entries: vec![],
    };
    if let Some(path) = cache {
        let full = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let cache_parent = full.parent().unwrap_or_else(|| Path::new("/"));
        if let Ok(parent) = cache_parent.canonicalize() {
            let candidate = parent.join(
                full.file_name()
                    .ok_or_else(|| anyhow::anyhow!("cache requires a file name"))?,
            );
            ensure!(
                !result
                    .files
                    .iter()
                    .any(|file| Path::new(&result.root).join(&file.path) == candidate),
                "dryer cache cannot replace a selected source"
            );
            if let Ok(relative) = candidate.strip_prefix(&result.root) {
                ensure!(
                    !relative.to_str().is_some_and(is_source_path),
                    "dryer cache cannot use a TypeScript source path"
                );
            }
        }
        cache_payload.identity = cache_identity(config)?;
        if path.exists() || path.is_symlink() {
            match read_cache(path, &cache_payload.identity, &result.root, &config.limits) {
                Ok(payload) => cache_payload = payload,
                Err(error) => result.problems.push(issue(
                    ProblemKind::Read,
                    ".",
                    0,
                    &format!("dryer cache rejected: {error}"),
                    None,
                    &config.limits,
                )),
            }
        }
    }
    let mut entries: BTreeMap<_, _> = cache_payload
        .entries
        .into_iter()
        .map(|entry| (entry.file.path.clone(), entry))
        .collect();
    let mut cache_used: usize = entries
        .values()
        .map(|entry| {
            crate::quality::encoded_size(entry, config.limits.max_report_bytes)
                .unwrap_or(config.limits.max_report_bytes)
        })
        .sum();
    let mut interner = HashMap::new();
    let mut weights = vec![];
    let mut bags = vec![];
    let mut nodes = 0usize;
    let mut facts_used = 0;
    let mut considered = 0usize;
    let mut excluded_used = 0;
    let mut problem_used = result
        .problems
        .iter()
        .map(|p| serde_json::to_vec(p).unwrap().len() + 1)
        .sum();
    'files: for sources in loaded.sources.chunks(config.limits.workers) {
        let inventories = crate::quality::workers::batch(sources, |file, source| {
            if let Some(entry) = entries.get(&file.path).filter(|entry| entry.file == *file) {
                Ok(FunctionInventory {
                    normalization: config.normalization.clone(),
                    functions: entry
                        .functions
                        .iter()
                        .map(|function| function.facts.clone())
                        .collect(),
                    excluded: vec![],
                    complete: true,
                    problems: vec![],
                    omitted_evidence: OmittedEvidence::default(),
                    normalized: entry.functions.clone(),
                    nodes: entry.nodes,
                })
            } else {
                extract(&file.path, source, &config.normalization, &config.limits)
            }
        })?;
        for ((file, _), inventory) in sources.iter().zip(inventories) {
            let inventory = inventory?;
            nodes = nodes.saturating_add(inventory.nodes);
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
            if cache.is_some() && inventory.complete {
                let old_bytes = entries.get(&file.path).map_or(0, |entry| {
                    crate::quality::encoded_size(entry, config.limits.max_report_bytes).unwrap_or(0)
                });
                let entry = CacheEntry {
                    file: file.clone(),
                    functions: inventory.normalized.clone(),
                    nodes: inventory.nodes,
                };
                let remaining = (config.limits.max_report_bytes / 2)
                    .saturating_sub(cache_used.saturating_sub(old_bytes));
                if let Ok(bytes) = crate::quality::encoded_size(&entry, remaining) {
                    cache_used = cache_used.saturating_sub(old_bytes) + bytes;
                    entries.insert(file.path.clone(), entry);
                }
            }
            result.omitted_evidence.functions += inventory.omitted_evidence.functions;
            result.omitted_evidence.problems += inventory.omitted_evidence.problems;
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
            for function in inventory.normalized {
                if considered >= config.limits.max_candidates {
                    result.omitted_evidence.functions += 1;
                    result.problems.push(issue(
                        ProblemKind::AnalysisLimit,
                        &file.path,
                        0,
                        "total function candidate limit reached",
                        Some(AnalysisLimitKind::Candidates),
                        &config.limits,
                    ));
                    break 'files;
                }
                considered += 1;
                let facts = &function.facts;
                if facts.nodes < config.minimum_nodes
                    || facts.function.location.end_line - facts.function.location.line + 1
                        < config.minimum_lines
                {
                    let exclusion = FunctionExclusion {
                        function: facts.function.clone(),
                        reason: "minimumSize".into(),
                        nodes: facts.nodes,
                    };
                    if !push_evidence(
                        &mut result.excluded,
                        exclusion,
                        &mut excluded_used,
                        &config.limits,
                    ) {
                        result.omitted_evidence.excluded += 1;
                    }
                    continue;
                }
                if bags.len() >= config.limits.max_candidates
                    || !push_evidence(
                        &mut result.functions,
                        facts.clone(),
                        &mut facts_used,
                        &config.limits,
                    )
                {
                    result.omitted_evidence.functions += 1;
                    continue;
                }
                bags.push(intern(function, &mut interner, &mut weights));
            }
        }
    }
    let mut comparisons = 0usize;
    let mut compared_entries = 0usize;
    let mut pairs_used = 0;
    'compare: for left in 0..bags.len() {
        for right in left + 1..bags.len() {
            let work = bags[left]
                .counts
                .len()
                .saturating_add(bags[right].counts.len());
            let limit = if comparisons >= config.limits.max_comparisons {
                Some(AnalysisLimitKind::Comparisons)
            } else if compared_entries.saturating_add(work) > config.limits.max_comparison_entries {
                Some(AnalysisLimitKind::ComparisonEntries)
            } else {
                None
            };
            if let Some(limit) = limit {
                result.problems.push(issue(
                    ProblemKind::AnalysisLimit,
                    ".",
                    0,
                    "structural comparison work limit reached",
                    Some(limit),
                    &config.limits,
                ));
                break 'compare;
            }
            comparisons += 1;
            compared_entries += work;
            let similarity = similarity(&bags[left], &bags[right], &weights);
            if similarity.weighted < config.similarity_threshold {
                continue;
            }
            let pair = ClonePair {
                left: bags[left].facts.function.clone(),
                right: bags[right].facts.function.clone(),
                similarity,
                exact_normalized_match: bags[left].root == bags[right].root,
                left_opaque_nodes: bags[left].facts.opaque_nodes,
                right_opaque_nodes: bags[right].facts.opaque_nodes,
            };
            if result.pairs.len() >= config.limits.max_pairs
                || !push_evidence(&mut result.pairs, pair, &mut pairs_used, &config.limits)
            {
                result.omitted_evidence.pairs += 1;
                result.problems.push(issue(
                    ProblemKind::ReportLimit,
                    ".",
                    0,
                    "pair evidence limit reached",
                    Some(AnalysisLimitKind::ReportBytes),
                    &config.limits,
                ));
                break 'compare;
            }
        }
    }
    result.pairs.sort_by(|a, b| {
        b.similarity
            .weighted
            .total_cmp(&a.similarity.weighted)
            .then_with(|| {
                (
                    &a.left.location.file,
                    a.left.location.start,
                    &a.right.location.file,
                    a.right.location.start,
                )
                    .cmp(&(
                        &b.left.location.file,
                        b.left.location.start,
                        &b.right.location.file,
                        b.right.location.start,
                    ))
            })
    });
    if let Some(path) = cache {
        // Remove files no longer discovered; retain earlier complete entries for
        // current files that failed, which can never match changed byte hashes.
        entries.retain(|path, _| result.files.iter().any(|file| file.path == *path));
        let payload = CachePayload {
            version: 1,
            identity: cache_payload.identity,
            root: result.root.clone(),
            entries: entries.into_values().collect(),
        };
        if let Err(error) = write_cache(path, payload, &config.limits) {
            result.problems.push(issue(
                ProblemKind::Read,
                ".",
                0,
                &format!("dryer cache could not be written: {error}"),
                None,
                &config.limits,
            ));
        }
    }
    for file in &result.files {
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
    result.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    fit_report(&mut result);
    validate_report_size(&result, &config.limits)?;
    Ok(result)
}

fn fit_report(result: &mut DryerReport) {
    if validate_report_size(result, &result.configuration.limits).is_ok() {
        return;
    }
    result.complete = false;
    result.problems.push(issue(
        ProblemKind::ReportLimit,
        ".",
        0,
        "dryer report byte budget reached",
        Some(AnalysisLimitKind::ReportBytes),
        &result.configuration.limits,
    ));
    while validate_report_size(result, &result.configuration.limits).is_err() {
        if result.pairs.pop().is_some() {
            result.omitted_evidence.pairs += 1;
        } else if result.functions.pop().is_some() {
            result.omitted_evidence.functions += 1;
        } else if result.excluded.pop().is_some() {
            result.omitted_evidence.excluded += 1;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::{BuildHasherDefault, Hasher};
    #[derive(Default)]
    struct ConstantHash;
    impl Hasher for ConstantHash {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, _: &[u8]) {}
    }
    fn facts() -> FunctionFacts {
        FunctionFacts {
            function: FunctionLocation {
                name: "f".into(),
                kind: FunctionKind::FunctionDeclaration,
                location: SourceLocation {
                    file: "src/f.ts".into(),
                    start: 0,
                    end: 10,
                    line: 1,
                    end_line: 1,
                },
                name_truncated: false,
            },
            nodes: 2,
            opaque_nodes: 0,
        }
    }
    fn function(operation: &str) -> NormalizedFunction {
        NormalizedFunction {
            facts: facts(),
            tree: vec![
                NodeKey {
                    kind: "IdentifierReference".into(),
                    scalar: operation.into(),
                    children: vec![],
                },
                NodeKey {
                    kind: "Function".into(),
                    scalar: "flags".into(),
                    children: vec![0],
                },
            ],
        }
    }
    #[test]
    fn hash_collisions_do_not_establish_exact_normalized_equality() {
        let mut interner: HashMap<NodeKey, usize, BuildHasherDefault<ConstantHash>> =
            HashMap::default();
        let mut weights = vec![];
        let fetch = intern(function("fetch"), &mut interner, &mut weights);
        let save = intern(function("save"), &mut interner, &mut weights);
        assert_ne!(fetch.root, save.root);
        assert_eq!(interner.len(), 4);
        let same = intern(function("fetch"), &mut interner, &mut weights);
        assert_eq!(fetch.root, same.root);
        assert_eq!(interner.len(), 4);
    }
    #[test]
    fn jaccard_formulas_preserve_distinct_counts_and_subtree_weights() {
        let left = Bag {
            facts: facts(),
            root: 0,
            counts: BTreeMap::from([(0, 3), (1, 1)]),
        };
        let right = Bag {
            facts: facts(),
            root: 0,
            counts: BTreeMap::from([(0, 1), (1, 2)]),
        };
        let actual = similarity(&left, &right, &[1, 4]);
        assert_eq!(actual.set, 1.0);
        assert_eq!(actual.multiset, 2.0 / 5.0);
        assert_eq!(actual.weighted, 5.0 / 11.0);
        let same = similarity(&left, &left, &[1, 4]);
        assert_eq!(
            same,
            SimilarityValues {
                set: 1.0,
                multiset: 1.0,
                weighted: 1.0
            }
        );
        let disjoint = Bag {
            facts: facts(),
            root: 2,
            counts: BTreeMap::from([(2, 1)]),
        };
        assert_eq!(
            similarity(&left, &disjoint, &[1, 4, 1]),
            SimilarityValues {
                set: 0.0,
                multiset: 0.0,
                weighted: 0.0
            }
        );
    }
}
