# Duplicate and mutation evidence

Archguard provides two optional review commands for TypeScript and TSX. Neither command adds CI policy or sets a target score. A complete duplicate report or mutation run exits 0 even when it finds similar functions or surviving mutations. Incomplete analysis and invalid configuration exit 2. The existing architecture check retains its exit codes.

## Inspect structural similarities

```sh
archguard dryer --root . --config examples/code-quality/dryer.json
archguard dryer --root . --config examples/code-quality/dryer.json --json
```

Dryer compares function declarations, methods and assigned arrow functions. Nested callbacks contribute to their containing function instead of becoming independent candidates. By default, candidates need four source lines and twenty normalized nodes. Declaration files are excluded.

The normalizer preserves called operations, operators, binding relationships and property names. It removes literal values while retaining their kinds. Renaming a local variable keeps its relationship to other uses, so `a + a` differs from `a + b`. Property-name preservation reduces unrelated matches. You can explicitly erase properties or local names, or preserve literal values, to investigate how those choices affect a pair.

The default threshold is 0.82 over unique subtree fingerprints. Every reported pair also includes counted and size-weighted similarity for comparison. Those values are evidence to inspect, not confidence estimates. Repeated boilerplate and unrelated functions can still match. Unknown syntax contributes exact source and an opaque-node count so the report exposes reduced normalization.

JSON reports include `groups`, connected components of the retained similar pairs. Each group lists member function locations, zero-based `pairIndices` into `pairs`, and `allMembersMatch`. Groups and members are ordered by file and byte range; pair indices are ascending. A group can contain indirect matches: A matching B and B matching C does not imply A matches C. `allMembersMatch` is true only when every member pair appears in the retained evidence. Pair scores remain in `pairs`.

`groupsComplete` follows report completeness. When it is false, omitted pairs or unanalyzed sources may connect groups, add members or supply missing matches. Groups describe only the retained pairs, and their bytes count against the report budget. Read `problems` and `omittedEvidence` before treating separate groups as independent findings.

Binding identifiers use declaration order. Adding a declaration can change later identifier assignments and lower similarity across otherwise parallel implementations. The explicit `localIdentifiers: "erase"` option helps investigate that case, while also discarding the distinction between repeated and different variables. Compare both reports before deciding whether a pair matters.

Parsing has explicit byte, node, raw-unit and recursion budgets. Conservative preflight checks also count punctuation in strings and comments, so unusually large literals or flat expressions can produce an incomplete report even when a compiler accepts them. The report identifies that limit instead of claiming the file was analyzed. Inspect or narrow the source selection; do not treat skipped analysis as clean evidence.

### Configure analysis limits

Dryer's `limits` object accepts `maxCandidates`, `maxComparisons` and `maxComparisonEntries`. Defaults are 1,000, 250,000 and 20,000,000 respectively. Their hard ceilings are 5,000, 2,000,000 and 100,000,000. Limits are fixed work budgets, independent of selected file count or elapsed time. Values above a hard ceiling are rejected.

`maxCandidates` counts extracted functions before the `minimumLines` and `minimumNodes` filters. The text summary and JSON `functions` list count retained candidates; `excluded` lists functions below minimum size. With no omitted evidence, 897 retained candidates and 103 exclusions consume a 1,000-function budget. A candidate-limit problem means more functions remain unprocessed. `omittedEvidence.functions` counts observed omissions, not a census of functions in unprocessed files.

`maxComparisons` bounds visited pairs, including pairs rejected by the fingerprint-set size bound. For N retained candidates, allow N × (N − 1) / 2 comparisons. `maxComparisonEntries` separately bounds the sum of the two unique fingerprint-set sizes for pairs requiring structural comparison. Pairs whose set sizes cannot meet `similarityThreshold` skip that work without discarding a possible match. A low threshold reduces pruning and may require narrowing the source selection.

For a selection with about 2,000 extracted functions and 1,800 retained candidates, start with:

```json
{
  "schemaVersion": 1,
  "selection": {
    "include": ["apps/**/*.ts", "apps/**/*.tsx", "packages/**/*.ts", "packages/**/*.tsx", "tools/**/*.ts"]
  },
  "limits": {
    "maxRawUnits": 16384,
    "maxCandidates": 2500,
    "maxComparisons": 2000000,
    "maxComparisonEntries": 100000000
  }
}
```

Inspect `complete`, `problems` and `omittedEvidence` after changing limits. A larger budget does not guarantee complete analysis for every selection.

An optional `--cache /outside/project/dryer-cache.json` reuses complete extraction after checking configuration, source contents, discovery and implementation identity. Policies and comparisons still run. A cache hit does not establish a faster run.

## Inspect mutations before executing tests

```sh
archguard mutator plan --root . --config examples/code-quality/plan.json --json > /tmp/domain-plan.json
archguard mutator run --root . --config examples/code-quality/mutator-weak.json --plan /tmp/domain-plan.json
archguard mutator run --root . --config examples/code-quality/mutator-strong.json
```

Planning parses selected source and lists exact UTF-8 byte edits. It executes no repository commands. Runtime operators cover comparisons, equality, arithmetic, logical operators, prefix/postfix update operations, boolean literals and zero/one literals. Type-only literals are excluded. The plan identifies source hashes and mutation ownership. Stored plans are revalidated before execution.

Running requires an explicit trusted command and Linux, macOS, or Windows. Archguard never discovers or installs a test runner. It copies configured source and dependencies into private workspaces, runs a fresh baseline, and then applies one verified edit per fresh worker copy. The original source stays untouched. File selection determines analysis scope; workspace selection determines the command's available inputs. Declare all required tests, helpers, configuration and dependency copies. Exclude the original installed dependency and build directories when supplying separate dependency copies.

The [test protocol](mutator-test-protocol.md) records executed tests and failure kinds. Use the supplied Node-builtin SDK and Vitest reporter with an already installed compatible Vitest. A nonzero process status alone cannot kill a mutant. Assertion failures can kill it; missing metadata, imports, teardown failures, timeouts and resource failures remain separate outcomes. Syntax-invalid edits are identified before execution. A survivor names the original expression, replacement and source context for a reviewer.

The authored domain example deliberately starts with weak tests. The strengthened set adds exact shipping boundaries, an international shipment, nonzero tax, the promised approval flag and an empty total. `Math.abs(amount) + 0` changing to `Math.abs(amount) - 0` is an intentional equivalent control. Decide whether a survivor exposes missing behavior, a missing specification or an equivalent change before adding tests.

## Bound execution and reuse

Execution configuration sets worker count, per-command and whole-run deadlines, output/result/report budgets, workspace file and byte ceilings, open-file limits, CPU limits and generated-file limits. Linux can also enforce an address-space limit. The report records effective limits and selected workspace inputs. SIGINT and SIGTERM on Unix, and Ctrl-C or Ctrl-Break on Windows, request cancellation and bounded cleanup. Unix library callers must keep the default SIGCHLD disposition and must not reap Archguard-owned child processes. An incompatible SIGCHLD handler fails closed because Archguard must confirm process ownership and cleanup.

The executable and declared external inputs are hashed without copying them into the worker workspace. Their read limits are one GiB per file and eight GiB in total, separate from copied and generated workspace bytes. The metadata budget and shared run deadline still apply.

Reuse is disabled by default. An explicit state directory outside source and dependency inputs can enable `declaredInputs` reuse. Archguard hashes the full configured workspace, tests, helpers, package metadata, dependency copies, executable, environment and declared external inputs. Changed inputs invalidate previous results conservatively. Every run still executes the baseline. Only complete killed or survived results with confirmed cleanup can be reused.

Reuse assumes that you declared every relevant input. Network services, wall-clock time and other host state are not automatically reproducible. Keep reuse off for tests that depend on those inputs. Corrupt state is reported and rerun; unresolved cleanup prevents unsafe reuse. State uses an exclusive lock and atomic checksummed writes.

Reuse invalidates the run's results when a declared input changes. It does not retain killed mutations by function alone. You can narrow source selection to changed files, but tests and helpers still belong in the declared workspace. The current runner does not use coverage to skip mutation sites.

Worker copies and resource limits protect ordinary trusted test execution. They do not sandbox arbitrary code. A command that deliberately escapes its process group or accesses external paths can exceed those protections.

On Windows, an owned Job Object provides descendant cleanup. Its CPU bound counts user-mode CPU time per process. Windows does not support `maxAddressSpaceBytes`, Unix open-file limits, or per-file generated-size limits. `maxOpenFiles` and `maxGeneratedFileBytes` default to zero on Windows, and a nonzero request is rejected before execution. Workspace file-count and aggregate-byte ceilings, deadlines, and output/result/report budgets still apply.

Windows commands must resolve to an `.exe`; configure `node` with a script or explicitly configure a command interpreter when needed. For `cmd.exe`, use `/s` and supply exactly one command body after `/c` or `/k`, for example `["cmd.exe", "/d", "/s", "/c", "npm.cmd test"]`. The command body uses `cmd.exe` quoting rules and is preserved verbatim.

Windows directory links are copied as junctions inside the owned workspace. Copying file symlinks requires Windows Developer Mode or symlink privileges; unavailable privileges produce incomplete analysis.

Process-group cleanup allows five seconds after sending SIGKILL to confirm that the owned group has no live members. Pipe draining after executable exit has a separate 250 ms limit. Cleanup uncertainty preserves the workspace and stops the run, even when the command originally timed out. Cleanup checks a separate thirty-second deadline while traversing owned workspaces. Individual kernel filesystem calls can still block; these deadlines are not hard wall-clock guarantees for an unresponsive kernel or filesystem.

## Use the reports with an agent

Run reports include bounded failure messages, source context and effective execution limits. Incomplete test commands retain short stderr/stdout excerpts; validated runtime/import/hook failures retain their kind, test identity and error text. Configured command/environment values are redacted from those messages. Output omissions are explicit and do not change an error into a kill.

The report retains at most sixteen diagnostic problems. `summary.omittedProblems` counts additional problems, and terminal cleanup/stop diagnostics take priority so retained workspace locations remain visible. Individual mutation results still record their own outcomes and messages. A nonzero omission count means the problem list is a bounded summary, not a complete list of causes.

Give the reviewer the selected scope, complete/incomplete status and concrete findings. For a similar pair, ask whether duplication is intentional or hides a reusable concept. For a survivor, ask which promised behavior changed without an assertion noticing. Keep equivalent mutations and intended parallel implementations as documented examples when they help explain a decision.

Source normalization uses the pinned Oxc parser and binding analysis already in Archguard. These commands do not supply compiler types or prove runtime parity. Existing compiler-provider APIs remain separate. No new product dependency or automatic quality gate is required.
