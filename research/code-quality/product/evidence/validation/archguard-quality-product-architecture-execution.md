# Archguard mutator execution architecture

Design-only recommendation against integration HEAD f8c2ea09f1a39ca246457e9a4b9cd982334d395c. No product files, branches, builds, or benchmark windows changed.

## Contracts and module ownership

Keep one crate. `mutator/facts.rs` and `mutator/plan.rs` own the versioned mutation inventory and checked AST edits. Execution owns `mutator/config.rs`, `mutator/run.rs`, `mutator/workspace.rs`, `mutator/state.rs`, and `mutator/result.rs`. Public re-exports stay under `archguard::mutator`. Avoid an executor trait or a scheduling framework.

Proposed public entry points:

```rust
pub fn run(
    plan: &MutationPlan,
    config: &ExecutionConfig,
    config_directory: &Path,
    cancellation: &CancellationToken,
) -> anyhow::Result<MutationReport>;

pub struct CancellationToken { /* private owned or static atomic flag */ }
impl CancellationToken {
    pub fn new() -> Self;
    pub fn from_static_flag(flag: &'static AtomicBool) -> Self;
    pub fn cancel(&self);
    pub fn is_cancelled(&self) -> bool;
}
```

`run` returns a report for analysis or execution failure after a valid invocation has started. Invalid configuration or inability to create the run directory may return an error. Callers can always distinguish complete, incomplete, and cancelled outcomes. The CLI wires SIGINT/SIGTERM to one process-wide atomic flag with existing nix signal support and passes CancellationToken::from_static_flag for that flag. The token privately holds either an owned Arc atomic for new() or a borrowed static atomic for from_static_flag; clone/cancel/is_cancelled access the same atomic. The library constructor installs no global handlers. Library callers own cancellation and do not acquire global signal handlers.

The plan contract agreed with syntax is `MutationPlan` v1 with operator version, canonical root, selection, selected source hashes, complete/problems, and deterministic full SHA256 site IDs. Each site contains file, exclusive UTF-8 byte range, owner, expected text, replacement, operator, and original source hash. Execution rejects incomplete plans, duplicate IDs, unsafe paths, missing sources, or changed source hashes. It checks every edit using the syntax-owned typed validation result. Only actual parser diagnostics on the edited source qualify as `invalidMutant`. Stale source hashes, malformed ranges, unexpected original text and invalid site metadata are invalid-plan failures and leave the run incomplete. Edited-source file-byte/raw-unit/depth/node budget exhaustion is a limit failure and leaves the run incomplete. None of these failures is a kill.

## Configuration

Use a separate strict versioned mutation configuration, consistent with the existing separate semantic configuration. Keep ProjectFacts and semantic versions unchanged. The CLI coordinator can combine source/operator options with this execution section in the quality config if the syntax architect chooses one common quality file.

The execution configuration contains:

- Exact trusted command argument array and relative working directory. No shell expansion, package-script discovery, automatic installs, or command guessing.
- Explicit inherited environment key list and explicit environment values. Build the command environment with `env_clear`, then add only configured values plus the assigned workspace, temporary directory, request/result locations and task identities. Resolve and hash the executable before running; use that resolved absolute path for every task. Environment values enter a digest and must never be printed as report evidence.
- Workspace include/exclude patterns, and explicit dependency copies with source and relative destination. Execution selection is independent of TS/TSX source selection and must not silently apply source discovery's ignore rules. Default execution inventory is the full root except `.git`, with installed dependencies included. Explicit exclusions narrow the test execution environment and must appear in the report.
- Limits for workers, mutant count, wall time per command and entire run, stdin/stdout/stderr/report bytes, file count/path bytes/file bytes, initial workspace bytes, total owned workspace bytes, retained log bytes, open files, CPU seconds and generated-file size. Validate every value and every multiplication before allocating or spawning. Default one worker; document a finite upper bound.
- Optional state directory outside the source root. Default result reuse is off. Opt-in declared-input reuse includes explicit external filesystem inputs and a stated contract that test outcomes do not depend on undeclared network, time, host files, or mutable services.

An arbitrary trusted command can read outside the copy. A fresh baseline alone cannot prove deterministic mutation outcomes. When that execution contract cannot be established, execute all mutations again while retaining the resumable report for inspection.

## Workspace isolation

Create one immutable owned input snapshot, hash it while copying, then re-enumerate/revalidate original inputs before execution. Inventory records normalized relative paths, file kinds, byte hashes, relevant executable bits, directory membership, symlink text and targets, dependency-copy origins, and explicit exclusions. Added/deleted entries invalidate the inventory. Reject special files, cyclic links, root escape, missing declared dependencies and unreadable files. Reject aliasing between source, state, template, result and dependency destinations.

Internal relative symlinks may survive only if their complete resolved targets exist inside the copied inventory. Explicitly declared outside dependency targets must be copied into the assigned destination and their content included in identity; no worker link may lead back into source or an outside writable dependency installation. A copied dependency may contain executables needed by the explicitly configured command. Copying it does not authorize Archguard to discover and launch commands from it.

The baseline and every mutant use independent ordinary file copies of the template. Never use writable hardlinks. Preserve executable bits, not ownership. Each task gets its own HOME/TMPDIR inside its copy. Recreate a worker copy after every task so one test's rewrites or generated artifacts cannot change the next task's inputs. Mutation replacement only occurs inside the copy. Source preservation comes from the execution path never writing original files. Revalidate original inventory after the run to detect outside changes, but do not try to restore or overwrite files another process changed.

Bound copy reads and bytes as they happen. The scheduler reserves initial copy bytes before assigning work and accounts for template, baseline, worker copies, retained reports and logs in the total disk budget. A failed copy or cleanup is an incomplete result. Cleanup only owned, verified, nonsymlink directories beneath the dedicated run parent.

## Shared command transport

Extend `subprocess.rs` with an internal bounded raw-command function. Keep current `run<T,R>` as a JSON serialization/validation wrapper with current plugin/provider behavior and limits. Every caller shares process creation, nonblocking bounded I/O, timeout, cancellation and process-group cleanup. Do not add a second mutation-only subprocess implementation.

Raw-command output contains exit status, bounded stdout/stderr, elapsed time, and a typed stop reason. Nonzero exit is data at this level. The JSON wrapper still rejects nonzero exit and malformed protocol output.

Replace detached blocking reader/writer threads with nonblocking Unix descriptors and one poll loop per command, using existing nix feature flags as needed. Drain stdout/stderr concurrently; stop on the first exceeded limit rather than waiting for command exit. Poll cancellation and run/command deadlines. A pipe EOF, child exit, limit, cancellation and inherited descendant pipes must all be handled without leaking threads or descriptors. After the command exits or stops, kill its process group, reap the direct child, bound final draining and report any cleanup failure. Use owned guards so Rust unwinding also invokes cleanup.

Set finite supported child resource limits through existing nix: CPU time, individual generated-file size, open files, core dumps, and Linux address-space limits when requested and supported. Keep any necessary `pre_exec` operation allocation-free and narrowly scoped. Do not pretend a per-user process limit or a per-process memory limit is a strict aggregate group limit. macOS support must report which limits it actually enforces; unsupported requested hard bounds fail configuration validation rather than becoming silent no-ops.

Linux/macOS ordinary process groups are cleanup for cooperative trusted commands, not an OS sandbox. A process that creates a new session may escape group cleanup. File-size limits do not impose a total filesystem quota. Periodically scan only owned worker directories with bounded traversal to stop generated disk/file-count growth, document possible transient overshoot, and mark observed accounting or cleanup uncertainty incomplete. Strict copied bytes, retained output and worker concurrency can be enforced exactly. Do not advertise escaped-descendant containment or hard filesystem quotas.

## Test result protocol

Run a direct explicitly configured test command in the workspace through shared raw transport. Give it an assigned JSON request file and assigned result file using controlled `ARCHGUARD_MUTATION_*` environment values. The request contains schemaVersion, run/task identity, input digest, baseline or mutation phase, and site identity where applicable. Product-generated request/result paths stay outside mutated source paths.

A Node-builtins-only SDK defines the protocol, bounded result serialization and atomic file replacement. The supplied Vitest reporter implements Vitest's reporter callbacks without importing Vitest or adding a package dependency. The user's explicit command enables that reporter. Other trusted adapters can emit the same typed contract. The SDK may be `.ts` for existing SDK conventions and Node 24 direct execution.

The reporter response contains schemaVersion, request/run/task/input identity, completion, test counts, passed/failed/skipped counts, bounded assertion failure details, suite errors, hook errors, unhandled errors, runtime errors, interruption and completion reason. It writes one assigned bounded regular file atomically. Rust rejects unknown versions, unknown fields, wrong identities, duplicate/missing records, unsafe result paths, contradictory counts/status, excessive arrays/strings, and truncated JSON. Read the result only after command cleanup, with a limit applied before allocation. Stdout/stderr are logs and never determine kill classification.

A passing baseline requires successful command exit, complete reporter execution, at least one executed test, consistent counts, no assertion failures and no suite/hook/runtime/unhandled errors. Baseline always executes before reuse or scheduling, even for a cache hit. Any failed or ambiguous baseline stops scheduling and marks all remaining candidates `notRun` with the baseline reason.

A mutant is `survived` only for an exit-0 complete passing test result. It is `killed` only for complete recognized assertion failures, matching nonzero assertion-failure exit status, and zero suite/hook/runtime/unhandled/import/configuration errors. Every failed test's error must be a recognized assertion failure; mixed failures are `executionError`. A timeout is `timedOut`; output/resource limit is `executionError` with its stop reason; cancellation is `cancelled`; missing or contradictory metadata is `executionError`. Never classify a compiler/import error, arbitrary exit 1, signal, or timeout as killed. The protocol records no coverage claim unless a separately validated adapter actually provides coverage facts.

## Scheduler and reporting

Use a finite number of Rust worker threads with a shared deterministic candidate queue. Workers share the immutable template and cancellation token. Each runs at most one command at a time. A single coordinator accepts results, persists records and advances the queue; workers never concurrently overwrite state files. Stop scheduling on cancellation, global run deadline, source/external input changes, or cleanup uncertainty. Already-running tasks receive cancellation and group cleanup.

`MutationReport` v1 includes selected scope and execution inventory digest, a non-secret execution summary with effective limits/workspace selection/dependency destinations/reuse mode, baseline outcome, per-site outcomes, problems, completeness, executed/reused counts and elapsed time. Results sort by site ID or source position independently of completion order. Include source excerpt and replacement for survivors with the config/site ID needed to rerun. The report must never repeat arbitrary command arguments or environment values because either can contain credentials. Bound excerpt strings and aggregate report/log bytes during collection, before final JSON serialization. Never retain full command output per site without an aggregate cap. A report budget failure produces a valid incomplete report with omitted-record counts, not truncated invalid JSON or a misleading complete report.

Decided CLI exits: 0 for a complete review report regardless of survivors or duplicate candidates; 2 for invalid config or incomplete analysis/execution/cancellation. Existing `archguard check` exit 1 for policy violations remains unchanged. No score calculation, threshold or CI gate. Known `invalidMutant` candidates are reported with a count; do not invent semantic invalidity from runtime errors.

## State and result reuse

State is independent of existing source-graph `cache.rs`. Use versioned JSON with strict fields and SHA256 checksum. Key identity includes product executable/build and locked dependency identities; plan/operator/protocol versions; canonical source root and config directory; full plan/site identity; full workspace and dependency inventories; executable/explicit command support-file hashes; exact arguments and working directory; controlled environment digest; platform/architecture; explicit outside input inventories; exclusions; and execution/resource settings. Include selected-source and test/config/helper/package/lock/dependency edits through full inventory. Revalidate input inventory before cache read, after snapshot, and before publishing reusable records. Uncertainty disables reuse.

Hold a nonblocking exclusive advisory `Flock` on a stable state lock file for the whole run, using existing nix `fs` feature. Never delete the lock inode or rely on PID files as locks. Concurrent invocation with the same state directory fails clearly and spawns nothing. A kernel-released lock handles crashed Archguard processes.

Keep a checksummed manifest and per-site completed records, or one bounded snapshot rewritten by the coordinator. Prefer one bounded snapshot initially unless measurements justify a journal. Write with `create_new` to a unique temporary file in the same directory, flush/sync, atomic rename, then sync the directory. Failed writes are reported. Reject unsupported state versions, bad checksum, duplicate sites, records inconsistent with the plan, unsafe paths and oversized files. Corruption does not prevent a fresh execution, but it must be visible and never counted as reuse.

Only complete, cleanup-confirmed `killed` and `survived` records from unchanged declared execution inputs are reusable. Never cache failed baseline, in-flight, timeout, cancelled, missing-result or ambiguous-error outcomes. Reusing a record still requires a freshly passing baseline and exact plan/input identity. Changing one selected source changes its site identity; changing tests, configs, helpers or dependencies changes the whole execution identity and forces all applicable candidates to rerun. Existing completed results can remain useful after an interrupted run only when the complete declared inputs remain unchanged and each record was fully validated and atomically saved.

Record active task/workspace ownership before command launch so normal cancellation can clean every owned task. Hard parent SIGKILL can leave test descendants behind. Do not kill saved numeric PIDs after restart without verifying process-start identity on that OS, and do not delete an uncertain live workspace. Refuse false-clean recovery and report the remaining task/workspace identity. A supervisor or OS sandbox would be separate work if a hard-kill containment guarantee becomes necessary.

## Failure and correction tests

- Passing baseline, assertion-only kill, survived mutation, checked invalid mutant. Correct the test to kill a previous survivor; retain a negative control that remains unaffected.
- Passing assertion mixed with suite import, hook setup/teardown, unhandled rejection or runtime failure; each stays execution error. Arbitrary nonzero exits, stale/missing/wrong-identity/duplicate/oversized reporter results never kill.
- Fresh baseline on all-cache-hit resume; failed baseline invokes no mutations. Test, helper, config, manifest, lock, dependency byte/mode/symlink, executable, env, operator or limit changes invalidate appropriately. Added/deleted input file changes invalidate inventory. Undeclared-input mode never reuses.
- One worker versus several yields the same sorted outcomes. Pressure tests exercise many tiny tasks, chatty stdout/stderr, unread stdin, descendants retaining pipes, timeout, cancellation, CPU/file-size limits, bounded disk growth, failed copy, no-space/write failures, and cleanup failures. Track open descriptors/threads and ordinary child processes before/after repeated trials; no monotonic growth.
- Commands rewrite copied source, helper and test files or generate artifacts; following tasks start pristine, and original source/dependency bytes remain unchanged. Reject worker/state links into originals, special files and outside dependency paths without declarations.
- Corrupt/truncated/unsupported state, atomic write interruption, competing state locks, crash with in-flight task and restart. Completed valid records resume; unfinished/uncertain tasks rerun or report incomplete. Never delete a live or unverified workspace.
- Keep existing plugin/provider transport tests passing, including large unread input and inherited pipes. Add direct raw-command limit/cancel tests rather than replacing protocol behavior with mutation assumptions.
- Validate the supplied reporter against T3's actual vite-plus 1.0.0 bundled Vitest 5.0.1 runner. The prior Vitest 4 research environment is an optional regression control. Dogfood selected TypeScript SDK, new reporter and TypeScript examples. The compiler provider is `.mjs` and outside the initial TS/TSX selection; initial mutation support also does not establish Rust runtime mutation coverage.

## Parallel implementation division

1. Syntax worker owns `quality`, dryer, mutation plan/facts/edit validation. It exports the agreed public types and uses no execution internals.
2. Execution worker owns mutation config/results/workspaces/scheduler/state and raw subprocess extension. It depends on the agreed plan/edit API and owns portable cleanup/cancellation tests.
3. Integration/examples worker owns CLI wiring, Node protocol SDK and Vitest reporter, guides, Cargo package allowlist, import/role policies, actual T3/adoption tests and end-to-end dogfood. Share a protocol fixture with execution before implementation starts.

The execution and SDK workers must agree on protocol JSON fixtures before writing code. The integration worker owns final module re-exports and CLI exit semantics to avoid conflicting edits. Child contributions merge into integration only; final PR remains unmerged until the root's requested review/approval procedure.

## Typed configuration and protocol draft

All structs derive serde camelCase with denied unknown fields. These names are proposed as the implementation contract, with ordinary default/validation methods rather than traits.

```rust
pub struct MutatorConfig {
    pub schema_version: u32,
    pub plan: MutationPlanConfig,
    pub execution: ExecutionConfig,
}

pub struct ExecutionConfig {
    pub command: Vec<String>,
    pub working_directory: String,
    pub inherit_environment: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub workspace: WorkspaceConfig,
    pub limits: ExecutionLimits,
    pub state: Option<StateConfig>,
}

pub struct WorkspaceConfig {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub dependencies: Vec<DependencyCopy>,
}

pub struct DependencyCopy {
    pub source: PathBuf,
    pub destination: String,
}

pub struct StateConfig {
    pub directory: PathBuf,
    pub reuse: ReusePolicy,
    pub external_inputs: Vec<PathBuf>,
}

pub enum ReusePolicy { Off, DeclaredInputs }

pub struct ExecutionLimits {
    pub workers: usize,
    pub max_mutants: usize,
    pub command_timeout_ms: u64,
    pub run_timeout_ms: u64,
    pub max_stdin_bytes: u64,
    pub max_stdout_bytes: u64,
    pub max_stderr_bytes: u64,
    pub max_result_bytes: u64,
    pub max_report_bytes: u64,
    pub max_retained_log_bytes: u64,
    pub max_workspace_files: usize,
    pub max_workspace_file_bytes: u64,
    pub max_workspace_bytes: u64,
    pub max_total_workspace_bytes: u64,
    pub max_open_files: u64,
    pub max_cpu_seconds: u64,
    pub max_generated_file_bytes: u64,
    pub max_address_space_bytes: Option<u64>,
}
```

Config file location resolves state directories, declared outside inputs and dependency sources. The source root is a separate CLI/API argument. Working directory, include/exclude paths and dependency destinations refer to the copied source root. Relative command executable paths resolve against config directory; bare executables resolve against explicitly inherited/configured PATH. Other command arguments are passed verbatim. Docs must identify which file-valued arguments require a declared outside input, rather than guess arguments that resemble paths.

Shared protocol, independent of the Vitest adapter:

```rust
pub struct TestExecutionRequest {
    pub schema_version: u32,
    pub request_id: String,
    pub run_id: String,
    pub input_digest: String,
    pub phase: ExecutionPhase,
    pub mutation_id: Option<String>,
    pub result_path: PathBuf,
    pub max_result_bytes: u64,
}

pub enum ExecutionPhase { Baseline, Mutation }

pub struct TestExecutionResult {
    pub schema_version: u32,
    pub request_id: String,
    pub run_id: String,
    pub input_digest: String,
    pub complete: bool,
    pub exit_code: i32,
    pub reason: TestCompletionReason,
    pub tests: TestCounts,
    pub failures: Vec<TestFailure>,
}

pub enum TestCompletionReason { Finished, Interrupted, InfrastructureError }
pub struct TestCounts {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}
pub struct TestFailure {
    pub kind: TestFailureKind,
    pub test_id: Option<String>,
    pub file: Option<String>,
    pub message: String,
}
pub enum TestFailureKind { Assertion, Runtime, Import, Hook, Suite, Unhandled }

pub enum MutationOutcome {
    Killed,
    Survived,
    InvalidMutant,
    ExecutionError,
    TimedOut,
    Cancelled,
    NotRun,
}
```

The request/result ID is unique per task and copied into each response. Result exitCode is required integer 0 through 255; missing or null is an execution protocol error. Only raw BaselineResult/MutationResult status fields are optional for signals, timeouts and work that did not run. `test_id` identifies a complete test case, not a title assumed globally unique; each failed test must have one or more failure records and the count of distinct failed test IDs must match `tests.failed`. Every failure used for a kill must be `Assertion`, have a known test ID and come from an executed test callback. Errors from hooks/suites/unhandled callbacks keep their distinct kinds even when the error is named AssertionError. Empty executed-test count is an execution error. Reporter-limit overflow writes a small complete=false infrastructure-error response if possible; an absent response still fails safely.

Environment protocol keys are `ARCHGUARD_MUTATION_REQUEST` and `ARCHGUARD_MUTATION_RESULT`; identity fields live in the request, avoiding a set of separate loosely coordinated environment keys. Rust owns and bounds both files. SDK reads the request only from its assigned path, never discovers a request in the repository. Vitest configuration or the explicit command enables the SDK reporter.

Node and Vitest can reserve substantial virtual address space. An optional address-space bound is platform-specific and must not be sold as a physical-memory cap. If monitored aggregate resident-memory/process-count limits are added, use existing `nix::libc` OS APIs only: Linux bounded `/proc` inventory and macOS libproc APIs already declared by the locked libc. Report monitoring and transient overshoot honestly. Do not introduce an implicit `ps` command or a new dependency.

Implemented raw transport consumes an owned Command, so reusing a value cannot accumulate child-limit pre_exec callbacks. Linux/macOS execution requires default SIGCHLD disposition, no SA_NOCLDWAIT and exclusive ownership of child reaping throughout the call; unsupported configurations fail before spawn. Every cleanup path probes WNOWAIT ownership. ECHILD or an unprovable ownership result suppresses subsequent numeric child/group signals and reports cleanup uncertain. Retain the leader PID until bounded native group-member observation confirms no non-zombie members remain. Unknown/inaccessible/oversized inventories or live members at the deadline prohibit clean completion, result reuse and automatic workspace deletion. macOS compilation has been checked with aarch64-apple-darwin; native runtime validation remains unavailable.

## Defaults, ceilings and exact protocol fixture

Defaults are deliberately conservative; these are safety bounds, not benchmark recommendations. The supplied config serializes all default values in docs so adoption examples do not rely on hidden choices. A limit must be positive, fit the platform type, and remain at or below its hard ceiling. Time arithmetic, file-byte totals, worker reservation and report allocation use checked arithmetic.

| Setting | Default | Hard ceiling |
| --- | --- | --- |
| workers | 1 | 32 |
| maxMutants | 1,000 | 100,000 |
| commandTimeoutMs | 120,000 | 3,600,000 |
| runTimeoutMs | 3,600,000 | 86,400,000 |
| maxStdinBytes | 65,536 | 67,108,864 |
| maxStdoutBytes | 1,048,576 | 8,388,608 |
| maxStderrBytes | 65,536 | 1,048,576 |
| maxResultBytes | 1,048,576 | 8,388,608 |
| maxInventoryBytes | 16,777,216 | 134,217,728 |
| maxReportBytes | 8,388,608 | 134,217,728 |
| maxRetainedLogBytes | 4,194,304 | 67,108,864 |
| maxWorkspaceFiles | 100,000 | 1,000,000 |
| maxWorkspaceFileBytes | 268,435,456 | 4,294,967,296 |
| maxWorkspaceBytes | 2,147,483,648 | 68,719,476,736 |
| maxTotalWorkspaceBytes | 8,589,934,592 | 274,877,906,944 |
| maxOpenFiles | 1,024 | 16,384, also no greater than inherited hard limit |
| maxCpuSeconds | 120 | 3,600 |
| maxGeneratedFileBytes | 67,108,864 | 4,294,967,296 |
| maxAddressSpaceBytes | null | 1,099,511,627,776 if supported |

Additional fixed validators bound command arguments to 256 items and 65,536 total UTF-8 bytes; environment to 256 keys and 65,536 total bytes; one path to 4,096 bytes; configured patterns to 1,024 items and 4,096 bytes each; dependency copies/outside input roots to 256 each; one failure message to 8,192 bytes; failure records to 1,024; and one failure/test identity to 4,096 bytes. Every response is also bounded by maxResultBytes. No arbitrary recursion-depth tree is accepted in the protocol.

Execution config defaults are workingDirectory `.`, inheritEnvironment `["PATH"]`, environment `{}`, workspace.include `["**"]`, workspace.exclude `[".git/**"]`, workspace.dependencies `[]`, and state `null`. A supplied state config defaults reuse to `off` and externalInputs to `[]`. Mutator/operator/source-selection defaults belong to the syntax contract. The source-selection defaults never narrow execution inventory implicitly. The state directory must be outside root and all dependency-copy source directories, and must not be a command support/input path.

Example explicit execution section. The command is illustrative; the examples worker must replace it with the actual vetted T3 invocation and reporter argument understood by Vite+.

```json
{
  "command": ["node", "node_modules/vite-plus/bin/vp", "test", "run", "--reporter", "/absolute/archguard/sdk/mutator-vitest-reporter.ts"],
  "workingDirectory": ".",
  "inheritEnvironment": ["PATH"],
  "environment": {"TZ": "UTC", "LANG": "C"},
  "workspace": {
    "include": ["**"],
    "exclude": [".git/**"],
    "dependencies": []
  },
  "limits": {
    "workers": 1,
    "maxMutants": 1000,
    "commandTimeoutMs": 120000,
    "runTimeoutMs": 3600000,
    "maxStdinBytes": 65536,
    "maxStdoutBytes": 1048576,
    "maxStderrBytes": 65536,
    "maxResultBytes": 1048576,
    "maxInventoryBytes": 16777216,
    "maxReportBytes": 8388608,
    "maxRetainedLogBytes": 4194304,
    "maxWorkspaceFiles": 100000,
    "maxWorkspaceFileBytes": 268435456,
    "maxWorkspaceBytes": 2147483648,
    "maxTotalWorkspaceBytes": 8589934592,
    "maxOpenFiles": 1024,
    "maxCpuSeconds": 120,
    "maxGeneratedFileBytes": 67108864,
    "maxAddressSpaceBytes": null
  },
  "state": {
    "directory": "/tmp/archguard-mutation-state/t3-calendar",
    "reuse": "declaredInputs",
    "externalInputs": ["/absolute/archguard/sdk/mutator-vitest-reporter.ts"]
  }
}
```

The command's first `node` is resolved to an absolute binary before hashing/spawn. The second argument is a worker-relative explicitly configured script inside the full snapshot; the absolute reporter support file is a declared external input. Product does not discover either executable command. Exclude patterns operate on path components and directory descendants; `.git` itself is also rejected from copying, not merely child paths.

Exact request fixture with illustrative digest values. Fixture IDs use strings, while validation requires the actual expected values from the runner rather than accepting their lengths alone.

```json
{
  "schemaVersion": 1,
  "requestId": "run-0001-task-0002",
  "runId": "run-0001",
  "inputDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "phase": "mutation",
  "mutationId": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "resultPath": "/tmp/archguard-mutation-state/t3-calendar/run-0001/task-0002/result.json",
  "maxResultBytes": 1048576
}
```

Exact complete killed-test response. One failed test and one assertion failure agree; the raw command must actually exit 1. The Rust runner determines `killed`, not the adapter.

```json
{
  "schemaVersion": 1,
  "requestId": "run-0001-task-0002",
  "runId": "run-0001",
  "inputDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "complete": true,
  "exitCode": 1,
  "reason": "finished",
  "tests": {"passed": 4, "failed": 1, "skipped": 0},
  "failures": [{
    "kind": "assertion",
    "testId": "calendar.test.ts::test-id-5",
    "file": "calendar.test.ts",
    "message": "AssertionError: expected 60 to equal 61"
  }]
}
```

A passing baseline uses phase `baseline`, mutationId `null`, complete `true`, exitCode `0`, reason `finished`, counts with at least one passed test and no failed tests, failures `[]`. A surviving mutant has that same passing result with phase/mutation ID in its request. Baseline assertion failures stop scheduling rather than being reported as mutation kills.

Exact mixed-error response stays `executionError` even though one test assertion failed:

```json
{
  "schemaVersion": 1,
  "requestId": "run-0001-task-0002",
  "runId": "run-0001",
  "inputDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "complete": true,
  "exitCode": 1,
  "reason": "finished",
  "tests": {"passed": 4, "failed": 1, "skipped": 0},
  "failures": [
    {"kind": "assertion", "testId": "calendar.test.ts::test-id-5", "file": "calendar.test.ts", "message": "AssertionError: expected 60 to equal 61"},
    {"kind": "hook", "testId": null, "file": "test/global-teardown.ts", "message": "Error: teardown control"}
  ]
}
```

Reporter implementation must treat onTestRunEnd as provisional. Vitest global teardown may fail later. Store task/hook/suite state there, refresh known late errors at beforeExit when available, then write the final bounded result synchronously from Node's `exit` event. `beforeExit` alone is insufficient because `process.exit()` bypasses it. Capture uncaught errors with `uncaughtExceptionMonitor`, which does not replace default termination. Do not add an `unhandledRejection` listener that suppresses Node's default failure. Obtain late handled/unhandled errors from the actual supported Vitest context state and correlate with hook/module callbacks. The final response's exitCode must match the Rust-observed command status. An early provisional file has complete=false, so it cannot become a kill if the finalizer never runs. Test the exact supported runner's assertion plus global-teardown and assertion plus late-unhandled controls before trusting this implementation. If the actual runner API cannot expose a late error reliably, return incomplete and add a narrowly scoped supported adapter rather than infer success from onTestRunEnd.

Public report types for CLI and Rust consumers:

```rust
pub struct MutationReport {
    pub schema_version: u32,
    pub operator_version: u32,
    pub root: String,
    pub selection: SelectionReport,
    pub input_digest: String,
    pub execution: ExecutionSummary,
    pub complete: bool,
    pub baseline: BaselineResult,
    pub summary: MutationSummary,
    pub results: Vec<MutationResult>,
    pub problems: Vec<ExecutionProblem>,
    pub elapsed_ms: f64,
}
pub struct ExecutionSummary {
    pub limits: ExecutionLimits,
    pub workspace: WorkspaceSummary,
    pub reuse: ReusePolicy,
    pub reuse_reason: Option<String>,
}
pub struct WorkspaceSummary {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub dependency_destinations: Vec<String>,
}
pub struct BaselineResult {
    pub outcome: BaselineOutcome,
    pub tests: Option<TestCounts>,
    pub exit_code: Option<i32>,
    pub elapsed_ms: f64,
    pub message: Option<String>,
}
pub enum BaselineOutcome { Passed, Failed, ExecutionError, TimedOut, Cancelled, NotRun }
pub struct MutationSummary {
    pub planned: usize,
    pub executed: usize,
    pub reused: usize,
    pub killed: usize,
    pub survived: usize,
    pub invalid_mutants: usize,
    pub execution_errors: usize,
    pub timed_out: usize,
    pub cancelled: usize,
    pub not_run: usize,
    pub omitted_results: usize,
    pub omitted_problems: usize,
}
pub struct MutationResult {
    pub mutation_id: String,
    pub location: SourceLocation,
    pub operator: MutationOperator,
    pub expected: String,
    pub replacement: String,
    pub context: Option<SourceContext>,
    pub outcome: MutationOutcome,
    pub reused: bool,
    pub tests: Option<TestCounts>,
    pub exit_code: Option<i32>,
    pub elapsed_ms: f64,
    pub message: Option<String>,
}
pub struct SourceContext {
    pub before: String,
    pub after: String,
    pub truncated_before: bool,
    pub truncated_after: bool,
}
pub struct ExecutionProblem {
    pub kind: ExecutionProblemKind,
    pub mutation_id: Option<String>,
    pub file: Option<String>,
    pub message: String,
}
pub enum ExecutionProblemKind {
    InvalidPlan, InputChanged, Workspace, Baseline, Command, Protocol,
    Limit, Cancellation, Cleanup, State,
}
```

Enums serialize lower camelCase. Optional fields above serialize as explicit `null` for a stable initial JSON contract. MutationResult inherits the exact agreed SourceLocation/operator format, preventing two competing site representations. The bounded report stores excerpts and failure messages only up to the aggregate budget; if complete per-site metadata cannot fit, mark complete=false and summarize omittedResults. Stored input manifests and state may contain absolute paths but never environment values or credentials. A reusable record reports reused=true while retaining its original execution duration; the new invocation's elapsedMs is measured separately. A complete report includes all planned outcomes, has baseline passed, no execution problems, and zero omitted results, execution errors, timeouts, cancelled or not-run candidates. Known invalidMutant outcomes do not imply that a test ran.

Native macOS execution cannot be validated on the current Linux host. Cross-compilation and conditional portable tests can catch build/API errors; keep macOS runtime cleanup/resource limitations explicit until those tests run on a native host.

Exact MutationReport v1 fixture for one fresh assertion-only kill. This has a complete requested one-file scope and no score. The new command exits 0.

```json
{
  "schemaVersion": 1,
  "operatorVersion": 1,
  "root": "/tmp/t3-calendar-fixture",
  "selection": {
    "requested": {"include": ["src/calendar.ts"], "exclude": []},
    "selected": [{"path": "src/calendar.ts", "bytes": 182, "sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"}],
    "skipped": [],
    "completeWithinSelection": true
  },
  "inputDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "execution": {
    "limits": {
      "workers": 1,
      "maxMutants": 1000,
      "commandTimeoutMs": 120000,
      "runTimeoutMs": 3600000,
      "maxStdinBytes": 65536,
      "maxStdoutBytes": 1048576,
      "maxStderrBytes": 65536,
      "maxResultBytes": 1048576,
      "maxInventoryBytes": 16777216,
    "maxReportBytes": 8388608,
      "maxRetainedLogBytes": 4194304,
      "maxWorkspaceFiles": 100000,
      "maxWorkspaceFileBytes": 268435456,
      "maxWorkspaceBytes": 2147483648,
      "maxTotalWorkspaceBytes": 8589934592,
      "maxOpenFiles": 1024,
      "maxCpuSeconds": 120,
      "maxGeneratedFileBytes": 67108864,
      "maxAddressSpaceBytes": null
    },
    "workspace": {
      "include": ["**"],
      "exclude": [".git/**"],
      "dependencyDestinations": []
    },
    "reuse": "off",
    "reuseReason": null
  },
  "complete": true,
  "baseline": {
    "outcome": "passed",
    "tests": {"passed": 5, "failed": 0, "skipped": 0},
    "exitCode": 0,
    "elapsedMs": 420.0,
    "message": null
  },
  "summary": {
    "planned": 1,
    "executed": 1,
    "reused": 0,
    "killed": 1,
    "survived": 0,
    "invalidMutants": 0,
    "executionErrors": 0,
    "timedOut": 0,
    "cancelled": 0,
    "notRun": 0,
    "omittedResults": 0,
    "omittedProblems": 0
  },
  "results": [{
    "mutationId": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    "location": {"file": "src/calendar.ts", "start": 91, "end": 92, "line": 4, "endLine": 4},
    "operator": "comparison",
    "expected": "<",
    "replacement": "<=",
    "context": {
      "before": "  return minute ",
      "after": " boundary;",
      "truncatedBefore": false,
      "truncatedAfter": false
    },
    "outcome": "killed",
    "reused": false,
    "tests": {"passed": 4, "failed": 1, "skipped": 0},
    "exitCode": 1,
    "elapsedMs": 410.0,
    "message": "AssertionError: expected 60 to equal 61"
  }],
  "problems": [],
  "elapsedMs": 912.0
}
```

Final unified contracts are `MutatorConfig { schemaVersion, plan: MutationPlanConfig, execution: ExecutionConfig }` and `run(&MutationPlan, &ExecutionConfig, &Path, &CancellationToken) -> Result<MutationReport>`. `MutationPlanConfig` and `MutationPlan.configuration` record the effective syntax limits/operator selection. Execution imports the syntax-owned quality types and MutationOperator, whose serialized V1 names are comparison, equality, arithmetic, logical, update, boolean and zeroOne.

Public execution re-exports are MutatorConfig, ExecutionConfig, WorkspaceConfig, DependencyCopy, StateConfig, ReusePolicy, ExecutionLimits, CancellationToken, run, MutationReport, ExecutionSummary, WorkspaceSummary, BaselineResult, BaselineOutcome, MutationSummary, MutationResult, SourceContext, MutationOutcome, ExecutionProblem, ExecutionProblemKind, TestExecutionRequest, TestExecutionResult, ExecutionPhase, TestCompletionReason, TestCounts, TestFailure and TestFailureKind. Parent owns mutator/mod.rs and lib.rs updates.

Final shared SelectionReport.selected is Vec<SourceFile>, where each file records path, byte count and SHA256. MutationReport uses this exact shared type and has no redundant selectedFiles field. Plan files remain unchanged under the agreed plan API; any duplicated selected hash metadata is validated consistently instead of introducing an execution-only selection type.

SourceContext supplies the original line prefix before the exact mutation range and the original line suffix after it. Each side is limited to 256 UTF-8 bytes and cut at a valid character boundary; truncatedBefore/truncatedAfter show any omitted part of the line. Preserve exact source characters, including tabs and CRLF context. Human output can show before + expected + after and the replacement separately. A null context means source context was unavailable, never an invented excerpt. Generate context from verified original snapshot bytes, and include it in saved result records under the same source/input identity. Test Unicode boundary cuts, long lines, first/last-line sites, multiline sites and stale-source refusal. These bounded excerpts are display metadata; MutationPlan and apply_edit retain their existing exact-byte contracts.

ExecutionSummary is mandatory in MutationReport. It serializes the fully defaulted validated effective ExecutionLimits, configured workspace include/exclude arrays and normalized relative dependency destinations, plus the effective reuse mode. It exposes no environment values, command arguments, external-input contents, dependency source paths or state credentials. A disabled/uncertain reuse contract reports reuse off even when state persistence remains enabled; reuseReason explains that choice in at most 1,024 UTF-8 bytes without disclosing values. The initial schema has no serialized command configuration. Tests validate both explicit and defaulted limit summaries, dependency destination normalization, reuse-off fallback, and absence of sample secret values from JSON, text and persisted identity.

Final syntax/execution ownership for edit validation is explicit:

```rust
pub fn inventory(
    path: &str,
    source: &str,
    config: &MutationPlanConfig,
) -> Result<MutationInventory>;

pub struct MutationInventory {
    pub sites: Vec<MutationSite>,
    pub complete: bool,
    pub problems: Vec<AnalysisProblem>,
    pub omitted_evidence: OmittedEvidence,
}

pub fn apply_edit(source: &str, site: &MutationSite)
    -> Result<String, EditError>;

pub enum InvalidPlanKind {
    SourceHash, Range, Utf8Boundary, ExpectedText, SiteIdentity, UnsafePath,
}

pub fn validate_mutant(path: &str, mutated: &str, limits: &AnalysisLimits)
    -> Result<SyntaxValidation>;

pub enum SyntaxValidation {
    Valid,
    InvalidSyntax { diagnostics: Vec<AnalysisProblem> },
    Incomplete { problems: Vec<AnalysisProblem> },
}
```

The syntax worker owns these types and functions, including the final exact EditError fields and shared OmittedEvidence definition. apply_edit only performs checked byte editing. The executor calls validate_mutant separately with plan.configuration.limits. InvalidSyntax is reserved for actual bounded parser diagnostics and produces InvalidMutant. Incomplete from edited-source file bytes, raw units, depth or nodes produces ExecutionProblemKind::Limit and an incomplete run; parser thread/resource failure produces Command or Limit as appropriate, never InvalidMutant. Every EditError invalid-plan category produces ExecutionProblemKind::InvalidPlan and an incomplete run. A complete inventory must have no omitted evidence or unreported failure. The public plan API is otherwise unchanged. Test a boundary mutation `<` to `<=` that crosses a byte/raw-unit ceiling and confirm it is Limit/incomplete; retain an actual invalid-syntax control and stale-source control to distinguish all three cases.

Workspace metadata collection is bounded before allocation/push by maxInventoryBytes (default 16MiB, hard 128MiB), counting path/kind/link/length/hash/mode metadata conservatively. Exhaustion reports Limit and incomplete. State encoding has separate fixed caps.

The additive public run_until(&MutationPlan,&ExecutionConfig,&Path,&CancellationToken,Instant)->Result<MutationReport> uses the earlier of its supplied deadline and the configured run timeout. Existing run delegates with its own current-time deadline. CLI can share a single deadline across initial planning and execution without altering stable execution-limit identity. No library signal handlers are installed.

Execution identity reads un-copied executables/declared external inputs under fixed 1GiB/file and 8GiB aggregate streaming guards, the configured inventory metadata budget, and the same run deadline. Workspace byte caps apply to copied/generated workspaces, so tiny authored disk-budget controls remain executable. State checksum covers the exact stored JSON payload bytes before decoding (including floating duration text), with fixed 16MiB inner/32MiB outer record and 1GiB/100k aggregate state caps.

Cleanup correction: deletion uses one iterative postorder traversal with descriptor-relative directory opens, stat, unlink and rmdir. Every directory component uses O_NOFOLLOW; the owned root and queued directory identities must match before traversal/deletion. Entry/path/queue guards run before enqueue and unlink, with fixed 34,000,000-entry and 128 MiB metadata bounds. Cleanup checks a fixed 30-second deadline during traversal/deletion; exhaustion preserves the owned path and active journal as cleanup uncertainty. Ordinary blocking filesystem system calls can exceed a deadline check, so this is bounded traversal/deletion work, not a hard realtime guarantee. A directory link to an external source is unlinked without changing external bytes or permissions. No new dependencies or feature flags are required.

Final report correction: MutationSummary.omittedProblems counts every dropped/replaced execution problem. The bounded 16-problem list prioritizes cleanup and retained run-root recovery diagnostics; byte-budget reductions remove lower-priority problems first. CLI reports the omission count.
