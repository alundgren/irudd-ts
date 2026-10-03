# Archguard dryer and mutator syntax design

Design based on integration HEAD f8c2ea09f1a39ca246457e9a4b9cd982334d395c. This file proposes contracts only. No product changes, build, timing trial, branch, or remote write occurred during design.

## Recommended modules and ownership

Keep the existing crate and existing dependency pins. Add public `quality`, `dryer`, and `mutator` modules. Do not change `ProjectFacts` v1, `SemanticFacts` v1, the existing check command, or TypeScript extension protocols.

`quality` owns selected TS/TSX source loading, strict shared limits, source locations, problems, and completion reporting. It does not resolve imports or execute code. `dryer` owns function extraction, normalization, structural comparison, its configuration, and versioned result. `mutator` owns versioned mutation plans and execution results. Its private children separate facts, plan generation, execution, and persistence. Only the execution module starts trusted configured commands.

Syntax implementer owns `src/quality/**`, `src/dryer/**`, and `src/mutator/plan.rs` plus mutation plan contracts in `src/mutator/facts.rs`. This includes quality/mod.rs and dryer/mod.rs. Execution implementer owns mutator config/run/result/cache code and shared bounded process execution. Parent owns mutator/mod.rs, lib.rs, CLI, adoption examples, documentation, Cargo packaging, and self-policy changes. The two implementers agree facts before implementation; they must avoid overlapping edits to parent-owned exports.

After architecture approval, the syntax implementer first produces one standalone foundation commit containing quality public types and pure mutation plan types. Include real serde contracts, defaults, and validation needed by these types; do not add placeholder functions or unfinished modules. Parent wires quality and pure mutator exports in the same reviewed foundation snapshot. Compile that snapshot and obtain a small independent review before branching the functional implementations from its exact commit. Dryer exports wait for real dryer code. Execution may prepare its own files in parallel, but compilation waits until the reviewed foundation types exist. Both functional workers then use the same foundation commit so field and enum drift cannot silently diverge across branches. No foundation code is authorized during the present design-only phase.

## Shared public contracts

All new persisted contracts use strict unknown-field rejection, camelCase JSON, independent schemaVersion 1, and checked versions. All source ranges use UTF-8 byte offsets and exclusive end. Line fields are one-based convenience values, never edit coordinates. Canonical report paths are relative forward-slash paths; reject absolute paths, parent traversal, duplicates, NUL, and lossy path conversion.

```rust
pub struct SourceSelection { pub include: Vec<String>, pub exclude: Vec<String> }
pub struct AnalysisLimits {
    pub max_file_bytes: usize, pub max_total_bytes: usize, pub max_files: usize,
    pub max_discovery_entries: usize,
    pub max_raw_units: usize, pub max_delimiter_depth: usize,
    pub max_nodes_per_file: usize, pub max_nodes: usize,
    pub max_candidates: usize, pub max_comparisons: usize,
    pub max_comparison_entries: usize, pub max_pairs: usize,
    pub max_sites: usize, pub workers: usize,
    pub max_report_bytes: usize, pub max_name_bytes: usize,
    pub max_problem_bytes: usize, pub max_excerpt_bytes: usize,
}
pub struct SourceLocation {
    pub file: String, pub start: usize, pub end: usize,
    pub line: usize, pub end_line: usize,
}
pub struct SourceFile { pub path: String, pub bytes: usize, pub sha256: String }
pub struct FunctionLocation {
    pub name: String, pub kind: FunctionKind, pub location: SourceLocation,
    pub name_truncated: bool,
}
pub enum FunctionKind {
    FunctionDeclaration, VariableFunction, VariableArrow, Method, FieldFunction, FieldArrow,
}
pub struct AnalysisProblem {
    pub kind: ProblemKind, pub file: String, pub offset: usize, pub message: String,
    pub message_truncated: bool, pub limit: Option<AnalysisLimitKind>,
}
pub enum ProblemKind {
    Traversal, Read, InvalidUtf8, UnsupportedLanguage, Parse, Binding,
    ChangedSource, UnsupportedSyntax, SourceComplexityLimit, AnalysisLimit, ReportLimit,
    ParserRuntime,
}
pub enum AnalysisLimitKind {
    FileBytes, TotalBytes, Files, DiscoveryEntries, RawUnits, DelimiterDepth,
    NodesPerFile, Nodes, Candidates, Comparisons, ComparisonEntries, Pairs, Sites, ReportBytes,
}
pub struct SkippedSource { pub path: String, pub reason: SkipReason }
pub enum SkipReason { ConfiguredExclusion, UnsupportedLanguage, DeclarationOnly }
pub struct SelectionReport {
    pub requested: SourceSelection, pub selected: Vec<SourceFile>,
    pub skipped: Vec<SkippedSource>, pub complete_within_selection: bool,
}
pub struct Completion { pub complete: bool, pub problems: Vec<AnalysisProblem> }
pub struct OmittedEvidence {
    pub selected_sources: usize, pub skipped_sources: usize,
    pub functions: usize, pub excluded: usize, pub pairs: usize,
    pub sites: usize, pub problems: usize,
}
```

ProblemKind names distinguish traversal, read, invalid UTF-8, unsupported extension, parse, binding analysis, changed source, unsupported syntax, source complexity, and exhausted analysis limits. SkippedSource reasons distinguish configured exclusion, unsupported language, declaration-only file, minimum candidate size, and a configured candidate filter. A requested unsupported language is an incomplete request, while paths outside requested selection do not affect completeness. Configured limits exhausted during analysis are incomplete, even when useful partial output exists. An intentional minimum function size excludes that candidate without marking the entire selected syntax analysis incomplete. An empty selected source set is incomplete. A complete selected set never implies whole-repository coverage.

Discovery uses existing ignore::WalkBuilder and config::Matcher without dependency resolution. Do not follow symbolic links or select node_modules, .git, or target. Report requested symlinks conservatively rather than silently treating them as regular files. V1 analyzes ts, tsx, mts, and cts; declaration-only files are explicitly skipped. Source bytes, discovery paths, selectors, and symlink observations are revalidated before accepting cached analysis. Keep this cache separate from source graph persistence.

## Public API and CLI division

```rust
pub fn dryer::analyze(root: &Path, config: &DryerConfig) -> Result<DryerReport>;
pub fn dryer::analyze_cached(root: &Path, config: &DryerConfig, cache: &Path)
    -> Result<DryerReport>;
pub fn dryer::extract(path: &str, source: &str, options: &NormalizationOptions,
    limits: &AnalysisLimits) -> Result<FunctionInventory>;
pub fn mutator::plan(root: &Path, config: &MutationPlanConfig) -> Result<MutationPlan>;
pub fn mutator::inventory(path: &str, source: &str, config: &MutationPlanConfig)
    -> Result<MutationInventory>;
pub fn mutator::apply_edit(source: &str, site: &MutationSite)
    -> std::result::Result<String, EditError>;
pub fn mutator::validate_mutant(path: &str, source: &str, limits: &AnalysisLimits)
    -> Result<SyntaxValidation>;
pub fn mutator::run(plan: &MutationPlan, config: &ExecutionConfig, config_dir: &Path,
    cancellation: &CancellationToken)
    -> Result<MutationReport>;
```

Library return values carry partial evidence and completeness; Result errors identify unusable configuration or system setup. Ordinary parse/read/selection failures remain visible in the report. The CLI uses dryer and mutator commands with root, tool-specific config, JSON, optional explicit cache, and mutator plan-only mode. A complete result exits 0 even when pairs or survivors exist. Incomplete or invalid analysis exits 2. Existing architecture check exit 1 remains unchanged. There are no aggregate mutation scores or required similarity gates.

DryerConfig contains schemaVersion, selection, minimumLines, minimumNodes, normalization, similarityThreshold, and limits. Use one threshold to select bounded review evidence, not policy status. NormalizationOptions contains normalizationVersion 1, localIdentifiers `bindings` by default or explicitly requested `erase`, properties `preserve` by default or `erase`, and literals `kind` by default or `value`. Calls preserve written operation names in both property modes. Configuration captures actual options in each report and cache identity.

```rust
pub struct DryerConfig {
    pub schema_version: u32, pub selection: SourceSelection,
    pub minimum_lines: usize, pub minimum_nodes: usize,
    pub normalization: NormalizationOptions,
    pub similarity_threshold: f64, pub limits: AnalysisLimits,
}
pub struct NormalizationOptions {
    pub normalization_version: u32,
    pub local_identifiers: LocalIdentifiers,
    pub properties: PropertyNames, pub literals: LiteralValues,
}
pub enum LocalIdentifiers { Bindings, Erase }
pub enum PropertyNames { Preserve, Erase }
pub enum LiteralValues { Kind, Value }
pub struct MutationPlanConfig {
    pub schema_version: u32, pub selection: SourceSelection,
    pub limits: AnalysisLimits, pub operators: Vec<MutationOperator>,
}
pub struct MutatorConfig {
    pub schema_version: u32, pub plan: MutationPlanConfig,
    pub execution: ExecutionConfig,
}
pub enum MutationOperator { Comparison, Equality, Arithmetic, Logical, Update, Boolean, ZeroOne }
```

SourceSelection defaults to include `**/*.ts`, `**/*.tsx`, `**/*.mts`, `**/*.cts` and an empty configured exclude list. Declaration-only files receive an intrinsic declarationOnly skip, not an implicit user filter. Dryer defaults minimumLines 4, minimumNodes 20, similarityThreshold 0.82, and the normalization modes above. MutationPlanConfig defaults to all seven V1 operator groups. LocalIdentifiers, PropertyNames, LiteralValues, and MutationOperator serialize as the camelCase enum variant names. All structs use serde defaults for omitted optional settings and strict unknown-field rejection; validate nonzero counts, finite threshold in [0,1], supported versions, and duplicate/empty operator lists. MutatorConfig.execution is required because runtime command authorization must be explicit. The public CancellationToken comes from execution, and library code installs no global signal handlers.

```rust
pub struct FunctionFacts {
    pub function: FunctionLocation, pub nodes: usize, pub opaque_nodes: usize,
}
pub struct FunctionExclusion {
    pub function: FunctionLocation, pub reason: String, pub nodes: usize,
}
pub struct SimilarityValues { pub set: f64, pub multiset: f64, pub weighted: f64 }
pub struct ClonePair {
    pub left: FunctionLocation, pub right: FunctionLocation,
    pub similarity: SimilarityValues, pub exact_normalized_match: bool,
    pub left_opaque_nodes: usize, pub right_opaque_nodes: usize,
}
pub struct DryerReport {
    pub schema_version: u32, pub normalization_version: u32, pub parser_version: String,
    pub root: String, pub configuration: DryerConfig, pub selection: SelectionReport,
    pub files: Vec<SourceFile>, pub complete: bool,
    pub functions: Vec<FunctionFacts>, pub excluded: Vec<FunctionExclusion>,
    pub pairs: Vec<ClonePair>, pub problems: Vec<AnalysisProblem>,
    pub omitted_evidence: OmittedEvidence,
    pub elapsed_ms: f64,
}
```

Pair selection uses set similarity against the configured review threshold while returning all three formulas. FunctionInventory exposes the same function/exclusion/problem evidence plus normalization options, with crate-private flat normalized nodes; these nodes are not a public JSON protocol. Location names never establish identity. Source ranges and file hashes do. FunctionLocation and FunctionKind belong to quality because both dryer and mutation planning use them without creating a dependency between the two product commands.

## Concrete resource limits

Recommended starting defaults and immutable hard maxima for implementation review:

| Limit | Default | Hard maximum |
| --- | ---: | ---: |
| Selected source bytes per file | 1 MiB | 1 MiB |
| Selected total source bytes | 32 MiB | 128 MiB |
| Selected source count | 2,000 | 10,000 |
| Visited discovery entries | 100,000 | 1,000,000 |
| Raw preflight units per file | 8,192 | 16,384 |
| Raw delimiter nesting | 128 | 256 |
| AST nodes per file | 50,000 | 100,000 |
| AST nodes across a run | 500,000 | 2,000,000 |
| Candidate functions | 1,000 | 5,000 |
| Compared pairs | 250,000 | 2,000,000 |
| Compared inventory entries | 20,000,000 | 100,000,000 |
| Returned pairs | 5,000 | 50,000 |
| Mutation sites | 10,000 | 50,000 |
| Syntax workers | 1 | 4 |
| Serialized report bytes | 8 MiB | 128 MiB |
| Display name bytes | 256 | 1,024 |
| Problem message bytes | 2,048 | 16,384 |
| Display excerpt bytes | 512 | 4,096 |

Use checked arithmetic before allocation. Reaching the returned-pair cap records reportLimit and incomplete output. Do not retain every below-threshold pair. Comparison order follows deterministic source path/range order and report ordering uses descending similarity with source/range tie breakers. Limits bound work independently of candidate counts because a small number of very large inventories still costs substantial work. Hard maxima cannot be increased by configuration. Final ceilings may be reduced after stress testing; increasing them needs new pressure evidence.

Count caps do not bound serialized bytes. Apply maxReportBytes across configuration, source/skip metadata, functions, exclusions, pairs, sites, diagnostics, and all strings before adding an evidence record. A bounded counting serde writer computes the record's JSON byte cost without first allocating an unbounded serialized vector. Maintain checked aggregate byte accounting, including JSON escaping and list/envelope overhead. Reserve room for valid incomplete-envelope metadata and omittedEvidence counters. The report limit minimum is 64 KiB. Reject configurations whose encoded effective settings exceed the fixed 16 KiB configuration ceiling, and reject canonical paths longer than the fixed 4,096-byte path ceiling. These independent fixed ceilings keep a valid minimal envelope representable. Validate the requested report budget against the envelope before analysis starts.

Function display names, problem messages, and display-only excerpts truncate at UTF-8 boundaries with explicit truncation metadata. Add `name_truncated: bool` to FunctionLocation and `message_truncated: bool` to AnalysisProblem; any additional display excerpt carries its own flag. Truncation of a display string does not alter identity or turn otherwise complete analysis incomplete. Exact interning keys, source paths, site IDs, expected source text, replacements, and hashes never truncate. If an exact record cannot fit, omit that evidence, increment the corresponding omittedEvidence counter, record ReportLimit, and mark the result incomplete. Do not collect oversized evidence and trim it only during final serialization. Bound the in-memory evidence collection as well as rendered bytes.

The terminal renderer and compact/pretty JSON renderers must respect the aggregate budget. At final serialization use a counting writer to verify the complete encoding, including indentation if requested, then write only a complete valid representation. If actual formatting exceeds reserved accounting, remove trailing evidence deterministically, increase omittedEvidence, and produce a complete JSON object with complete=false. Never emit truncated JSON. Tests include long identifiers repeated across many pairs, heavily escaped names, many long diagnostics, huge opaque excerpts, and compact/pretty differences, while retaining exact internal identifiers and valid parseable incomplete output.

Oxc 0.152 ParseOptions has no parser depth limit. A post-parse node counter cannot protect recursive parser or SemanticBuilder calls. Before parsing, count raw ASCII identifier/number runs and every other non-whitespace byte as units, without excluding string/comment/template contents. This deliberately conservative guard can reject complicated literal text; report sourceComplexityLimit. Also reject excessive raw delimiter nesting and long raw unary/assignment/conditional/arrow chains. Raw counts require no second parser and must never reset the whole-file budget at a semicolon. Execute parser plus binding analysis on a dedicated std::thread::Builder thread with a fixed 128 MiB stack, bounded concurrent workers, and no caller-controlled stack increase. After binding analysis, build owned inventories from semantic.nodes().iter() and parent IDs without recursive normalization or recursive owned trees. Fixed stack allocation alone is insufficient; the hard input guards are mandatory. Stress tests must validate the hard accepted envelope in both debug and release, including patterns that exceed defaults, and verify excess input returns an incomplete report before entering Oxc. This design is a bounded, tested contract rather than a claim that Oxc itself provides stack protection.

Read-only raw-unit counts for the scout-named existing T3 files at /tmp/archguard-quality-research/t3code show the defaults are practical for that initial sample: dateTime.ts 465, gitPatchPath.ts 1,276, providerSkills.ts 846, web/state/usage.ts 1,120, web/routes/usage.tsx 50. These are complexity counts, not runtime or timing observations; corpus ownership remains with the scout.

## Function inventory and normalization

Select functions with bodies: function declarations, object/class methods, variable-initialized arrows/function expressions, and class field arrows/function expressions. Exclude overloads, ambient declarations, callbacks in calls, and function-valued object properties without method syntax. Nested functions and callbacks stay inside their enclosing selected function; do not return overlapping nested candidates. Keep the selected declaration range, display name, kind, source hash, lines, node count, and reported conservative fallback count. Document these units explicitly; they preserve the existing research detector's candidate intent without claiming identical parser node counts.

Use already-installed Oxc SemanticBuilder for binding identity, never compiler types. Assign function-local binding ordinals in deterministic binding order, including parameter and nested callback bindings. Keep scoped shadowing distinct. Number captured bound identifiers consistently within the candidate; preserve free-reference names. Preserve written callees, member callees, constructor targets, and tags so map/filter and fetch/save remain different. The explicit erase mode abstracts identifiers more aggressively and must say so. Preserve the distinction between a+a and a+b in default mode.

Represent normalized trees as owned flat postorder nodes. Each exact NodeKey contains node kind, scalar attributes, and ordered child IDs. Run-global exact interning matches equal NodeKeys; HashMap hashes can choose a bucket but equality compares the full key. Per-file persistence uses local IDs and validates children precede parents before rebuilding the global interner. No hash digest or truncated identifier establishes tree equality. This avoids serializing the full text of every subtree, which grows quadratically for long chains.

Audited nodes explicitly retain binary/logical/assignment/unary/update operators, update prefix/postfix, function async/generator, optional call/member access, computed/private access, property getter/setter/shorthand flags, declaration kind, and all optional child positions. Preserve slot identity or presence masks: for(a;;b) and for(;a;b) must differ even when both have the same number of identifier children. Preserve JSX tag/attribute names and child order. Literal kind mode retains numeric/string/boolean/null/bigint/regex/template distinctions while abstracting chosen values; regex flags and behavior-bearing syntax need explicit retention or exact fallback. Parentheses may remain explicit; do not claim normalized equality means equal runtime behavior.

Unaudited syntax gets a conservative opaque exact-source leaf and an explicit fallback record with kind and location. Do not descend into that leaf for abstracted subtree comparison. This prevents a new Oxc scalar field or unhandled TS/JSX form from silently becoming kind-only equality. A source slice failure or unsupported node that cannot be represented conservatively marks analysis incomplete. Opaque counts appear in reports and pair evidence; documentation explains lower comparison coverage. TS type positions do not become runtime mutation sites. Default free type identifiers remain preserved or opaque; Oxc scope analysis does not establish compiler compatibility.

Compare normalized subtree sets, counted multisets, and weighted counted multisets. Set Jaccard is intersection distinct keys / union distinct keys. Counted Jaccard is sum min(counts) / sum max(counts). Weighted counted Jaccard weights each count by the subtree's checked node count. Emit names that identify these formulas, their values, both complete locations, normalization identity/options, and opaque counts. Pair similarity is review evidence, never proof of redundant behavior or a requirement to refactor. ExactNormalizedMatch, if included, means the normalized root keys are exactly equal under the recorded options, not runtime equivalence.

## Mutation plan contract and exact edits

```rust
pub struct MutationPlan {
    pub schema_version: u32, pub operator_version: u32,
    pub configuration: MutationPlanConfig,
    pub root: String, pub selection: SelectionReport, pub files: Vec<SourceFile>,
    pub sites: Vec<MutationSite>, pub complete: bool, pub problems: Vec<AnalysisProblem>,
    pub omitted_evidence: OmittedEvidence,
}
pub struct MutationInventory {
    pub sites: Vec<MutationSite>, pub complete: bool,
    pub problems: Vec<AnalysisProblem>, pub omitted_evidence: OmittedEvidence,
}
pub struct MutationSite {
    pub id: String, pub location: SourceLocation,
    pub owner: Option<FunctionLocation>, pub operator: MutationOperator,
    pub expected: String, pub replacement: String, pub source_sha256: String,
}
pub struct EditError { pub kind: InvalidPlanKind, pub message: String }
pub enum InvalidPlanKind {
    SourceHash, Range, Utf8Boundary, ExpectedText, SiteIdentity, UnsafePath,
}
pub enum SyntaxValidation {
    Valid,
    InvalidSyntax { diagnostics: Vec<AnalysisProblem> },
    Incomplete { problems: Vec<AnalysisProblem> },
}
```

Site IDs use a full SHA256 of a canonical serde array containing operatorVersion, relative file, sourceSha256, start, end, expected, and replacement. Do not concatenate ambiguous strings or truncate identity. Sort and deduplicate sites by full identity; two alternative edits at one range have different IDs. Plans contain selected source hashes and exact sites; execution maintains a separate full runtime input manifest covering tests, config, helpers, dependencies, trusted commands, and environment.

Per-file inventory returns MutationInventory, preserving all discovered partial sites, explicit completeness, problems, and omitted evidence. The outer plan merges problems and sets complete=false for any incomplete file; a partial inventory never becomes a complete plan. A Result error from inventory means invalid configuration/setup; the outer planner turns any per-file error into a visible incomplete result rather than dropping that file. SelectionReport.selected is the parent-settled Vec<SourceFile> contract. Existing MutationPlan.files remains stable for execution; validate that its sorted path/byte/hash inventory exactly matches selected metadata and count both copies against the report budget.

V1 generates comparison/equality swaps, addition/subtraction, multiplication/division, logical AND/OR, increment/decrement, boolean flips, and runtime numeric zero/one replacements. Its seven serialized operator categories are comparison, equality, arithmetic, logical, update, boolean, and zeroOne. The exact mappings are `<` to `<=` and reverse, `>` to `>=` and reverse, `==` to `!=` and reverse, `===` to `!==` and reverse, `+` to `-` and reverse, `*` to `/` and reverse, `&&` to `||` and reverse, `++` to `--` and reverse, true/false flips, and runtime numeric 0/1 flips. Record the group in site.operator and the exact mapping in expected/replacement. State the precise replacement inventory in versioned configuration and reports. Include module-level executable expressions with optional owner; nested callback expressions belong to the enclosing selected owner. Do not mutate type literals, import strings, directives, static property keys, regexes, JSX text, or ambient/declaration-only constructs. Arithmetic operators are syntax mutations; addition may be string concatenation.

Parent-owned exports should include `pub mod quality; pub mod dryer; pub mod mutator;` in lib.rs. Quality re-exports SourceSelection, AnalysisLimits, AnalysisLimitKind, SourceLocation, SourceFile, FunctionLocation, FunctionKind, AnalysisProblem, ProblemKind, SelectionReport, SkippedSource, SkipReason, OmittedEvidence, and Completion. Dryer re-exports DryerConfig, NormalizationOptions, LocalIdentifiers, PropertyNames, LiteralValues, DryerReport, FunctionInventory, FunctionFacts, FunctionExclusion, ClonePair, SimilarityValues, and analyze/analyze_cached/extract. Mutator re-exports syntax-owned MutationPlanConfig, MutationPlan, MutationInventory, MutationSite, MutationOperator, EditError, InvalidPlanKind, SyntaxValidation, and plan/inventory/apply_edit/validate_mutant; execution supplies its own agreed export list. Internal normalized node IDs and cache envelopes are not public contracts.

Literal spans come directly from AST nodes. Operators are located only inside AST-proven operand gaps or unary/update ranges. Scan those bounded gaps while recognizing whitespace and line/block comments, require exactly one matching operator token, and preserve surrounding comment bytes. A failed operator lookup is a reported incomplete inventory, not a skipped silent mutation. Never search entire source with replacement regexes. Tests must include comments containing operator text, strings containing operator text, unicode before a site, CRLF, adjacent shifts/comparisons, and nested expressions.

apply_edit performs byte-edit validation only. It checks source hash, full site identity, byte-range bounds, UTF-8 boundaries, and exact expected source slice before constructing a new owned String. It never writes a path or parses source. Its typed EditError always means InvalidPlan and incomplete analysis, including stale source, invalid ranges, mismatched expected text, or malformed site identity. Execution rejects the invalid plan before running the affected site; it never reports these failures as InvalidMutant.

After successful application, validate_mutant validates the whole changed source with the same hard-bounded Oxc parser and returns typed SyntaxValidation. Valid permits execution. InvalidSyntax requires actual Oxc parser syntax diagnostics from a parse performed within all source/complexity/resource guards and is the only syntax path that becomes InvalidMutant. Incomplete carries typed problems for sourceComplexityLimit, file bytes, other resource limits, thread startup/panic/system errors, or diagnostic-collection/report exhaustion; it marks execution incomplete and never becomes InvalidMutant. For example, replacing `<` with `<=` can cross maxRawUnits. The source is unvalidated because of a limit, not syntax-invalid. Do not use SemanticBuilder diagnostics as parser syntax failure, and do not classify an anyhow error by its text. SyntaxValidation diagnostic/problem collections obey the same byte budget and truncation metadata.

Optional compiler validation is a separate explicitly configured trusted service, with explicit pinned compiler identity when offered. Preserve compiler diagnostics separately. Passing Oxc parsing or SemanticBuilder never implies compiler validity or runtime parity.

## Cache and completion agreement with execution architect

Only cache complete validated per-file syntax inventories. Cache identity includes parser pin, normalization/operator version, all options, source path and bytes hash, selected discovery inputs, and executable/product identity. On failed/incomplete edits retain the previous complete entry but never present it as current analysis. Recompute comparison results and any current filters after reloading syntax. Parse and plan reuse counts do not establish faster runs.

Mutation execution always runs a fresh baseline, even when all candidate outcomes could be reused or no sites exist. Reuse only completed unambiguous killed/survived outcomes after validating the execution architect's full input identity and exact plan sites. Timeout, invalid mutant, assertion-plus-runtime failure, malformed reporter protocol, cleanup failure, and interrupted work cannot become kills or reusable success. Source changes invalidate exact plan-site reuse. The execution architect owns worker copies, process groups, cleanup, cancellation, bounded output, checkpointing, and runtime input inventory.

## Required implementation evidence

Carry failure/correction/negative controls from research/code-quality/dryer/detector.test.mjs: property erasure, local rename, called-member distinction, malformed/missing source, repetition counts, unary/update operators, and nested callback selection. Add binding relationship/shadowing/capture tests, missing optional slots, optional chaining, computed/private access, TSX names/order, async/generator distinctions, opaque fallback exact source, deliberate hash collision key equality, deterministic ties, bounded comparison work, and cache corruption/version/stale byte/discovery controls.

Mutation syntax tests cover each advertised operator, runtime vs type/key exclusions, exact comments and byte locations, duplicate alternatives, numeric formats and negative zero, invalid edits, stale source, multiline Unicode/CRLF preservation, malformed source correction, and no write to originals. Add explicit typed classification controls for malformed edits as InvalidPlan, a truly malformed parsed mutation as InvalidSyntax/InvalidMutant, and valid `<` to `<=` edits crossing raw-unit or byte limits as Incomplete. Retain an unchanged edit fitting those limits as the negative control. Partial per-file operator lookup failures must preserve sites and problems and make the outer plan incomplete. Rust integration pressure tests exercise maximum accepted preflight input and rejected excessive parens, object nesting, JSX nesting, unary chains, assignment/conditional/arrow chains, and left-associative binary chains. Leak/failure/process controls are execution-owned but must compare original source digests before and after.

Self-dogfood both tools on existing sdk and TS example files with explicit selection and a trusted authored execution wrapper. The Rust implementation is outside TS/TSX syntax analysis; Rust pressure/acceptance tests exercise its behavior directly. The current provider.mjs is outside V1 TS/TSX selection and must be reported honestly. Parent owns verified scout-selected T3 examples; preserve copied-source notices and exact revisions without broadening corpus selection.

Existing evidence locations: docs/development/architecture.md and README.md define one-crate module ownership, pins, and checks. src/project.rs contains current discovery and complete selected-closure behavior. src/typescript.rs uses Oxc parser plus SemanticBuilder. src/cache.rs demonstrates exact source/version/checksum invalidation and complete-entry retention. src/semantic/inventory.rs demonstrates independent versioned source-hash/site contracts. research/code-quality/dryer/detector.mjs and detector.test.mjs contain the structural controls. research/code-quality/mutator/findings.md documents stale-test/helper/dependency cache failures and incorrect killed classifications; its fixtures/account.ts gives the small operator inventory and finite-integer equivalent-mutant control. tests/architecture.rs must expand with new module import rules when parent updates archguard.json. Required final check remains scripts/check.sh and the CLI against archguard.json; no CI is added.

## Reviewed foundation corrections and implementation order

SelectionReport validation checks each declared selected path against include and exclude matchers. This verifies the submitted inventory's membership; filesystem selection performs discovery separately. MutationInventory and SyntaxValidation expose validate_with_limits and their no-argument validators apply immutable hard maximums. Both encoded bytes and site counts stay bounded. SyntaxValidation deserializes through a private strict wire enum whose valid result has no fields, preserving the public unit Valid variant while rejecting contradictory fields.

After the foundation merge, implement shared selection and guarded parsing first. Own quality/selection.rs and quality/source.rs, with bounded discovery, exact-byte reads, per-file source guards, fixed parser stack, typed problems, and standalone malformed/corrected/unchanged controls. Then implement mutator plan/edit validation using iterative semantic node traversal, deterministic exact edits, and no writes to input sources. This provides execution with real plan/apply_edit/validate_mutant APIs before dryer comparison work.

Implement dryer configuration/facts and extraction next, followed by audited normalization and counted comparison. Test a+a versus a+b, scoped shadowing, captures, written call names, operator and optional-child retention, conservative exact opaque leaves, and nested callback selection before adding the incremental cache. Reuse entries only after validating current source/configuration/parser identities and bounded encoded inventories. Cache corruption, changed discovery, option changes, and stale bytes must force fresh analysis. Comparison and collection bounds must stop before growing evidence beyond the report budget, preserving valid incomplete results and omission counts.

Keep accepted-envelope parser pressure separate from ordinary correctness checks until the parent coordinates debug/release trials. Retain exact fixture bytes and original-source digest controls throughout syntax implementation. After targeted tests, run required full checks at the child PR's exact final head; dependent execution and CLI integration use that reviewed source snapshot.
