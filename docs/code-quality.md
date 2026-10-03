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

Parsing has explicit byte, node, raw-unit and recursion budgets. Conservative preflight checks also count punctuation in strings and comments, so unusually large literals or flat expressions can produce an incomplete report even when a compiler accepts them. The report identifies that limit instead of claiming the file was analyzed. Inspect or narrow the source selection; do not treat skipped analysis as clean evidence.

An optional `--cache /outside/project/dryer-cache.json` reuses complete extraction after checking configuration, source contents, discovery and implementation identity. Policies and comparisons still run. A cache hit does not establish a faster run.

## Inspect mutations before executing tests

```sh
archguard mutator plan --root . --config examples/code-quality/plan.json --json > /tmp/domain-plan.json
archguard mutator run --root . --config examples/code-quality/mutator-weak.json --plan /tmp/domain-plan.json
archguard mutator run --root . --config examples/code-quality/mutator-strong.json
```

Planning parses selected source and lists exact UTF-8 byte edits. It executes no repository commands. Runtime operators cover comparisons, equality, arithmetic, logical operators, prefix/postfix update operations, boolean literals and zero/one literals. Type-only literals are excluded. The plan identifies source hashes and mutation ownership. Stored plans are revalidated before execution.

Running requires an explicit trusted command and Linux or macOS. Archguard never discovers or installs a test runner. It copies configured source and dependencies into private workspaces, runs a fresh baseline, and then applies one verified edit per fresh worker copy. The original source stays untouched. File selection determines analysis scope; workspace selection determines the command's available inputs. Declare all required tests, helpers, configuration and dependency copies. Exclude the original installed dependency and build directories when supplying separate dependency copies.

The [test protocol](mutator-test-protocol.md) records executed tests and failure kinds. Use the supplied Node-builtin SDK and Vitest reporter with an already installed compatible Vitest. A nonzero process status alone cannot kill a mutant. Assertion failures can kill it; missing metadata, imports, teardown failures, timeouts and resource failures remain separate outcomes. Syntax-invalid edits are identified before execution. A survivor names the original expression, replacement and source context for a reviewer.

The authored domain example deliberately starts with weak tests. The strengthened set adds exact shipping boundaries, an international shipment, nonzero tax, the promised approval flag and an empty total. `Math.abs(amount) + 0` changing to `Math.abs(amount) - 0` is an intentional equivalent control. Decide whether a survivor exposes missing behavior, a missing specification or an equivalent change before adding tests.

## Bound execution and reuse

Execution configuration sets worker count, per-command and whole-run deadlines, output/result/report budgets, workspace file and byte ceilings, open-file limits, CPU limits and generated-file limits. Linux can also enforce an address-space limit. The report records effective limits and selected workspace inputs. SIGINT and SIGTERM request cancellation and bounded cleanup.

Reuse is disabled by default. An explicit state directory outside source and dependency inputs can enable `declaredInputs` reuse. Archguard hashes the full configured workspace, tests, helpers, package metadata, dependency copies, executable, environment and declared external inputs. Changed inputs invalidate previous results conservatively. Every run still executes the baseline. Only complete killed or survived results with confirmed cleanup can be reused.

Reuse assumes that you declared every relevant input. Network services, wall-clock time and other host state are not automatically reproducible. Keep reuse off for tests that depend on those inputs. Corrupt state is reported and rerun; unresolved cleanup prevents unsafe reuse. State uses an exclusive lock and atomic checksummed writes.

Worker copies and resource limits protect ordinary trusted test execution. They do not sandbox arbitrary code. A command that deliberately escapes its process group or accesses external paths can exceed those protections. macOS has different memory controls from Linux.

## Use the reports with an agent

Give the reviewer the selected scope, complete/incomplete status and concrete findings. For a similar pair, ask whether duplication is intentional or hides a reusable concept. For a survivor, ask which promised behavior changed without an assertion noticing. Keep equivalent mutations and intended parallel implementations as documented examples when they help explain a decision.

Source normalization uses the pinned Oxc parser and binding analysis already in Archguard. These commands do not supply compiler types or prove runtime parity. Existing compiler-provider APIs remain separate. No new product dependency or automatic quality gate is required.
