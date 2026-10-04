# Budgeted mutation campaign

Start after static findings are checkpointed in the draft PR or local patch.
Mutation detects fault sensitivity; it is not a probability of detecting real
bugs. Publish the actual inventory, selection, completion, and exclusions.

## Establish execution

`archguard mutator plan --root PROJECT --config PLAN_CONFIG --json` lists
source-hashed edits and executes no tests. `archguard mutator run --root PROJECT
--config RUN_CONFIG --json` requires a trusted command and declared workspace
inputs. Read the current code-quality/protocol guides for schemas. The product
handles fresh baselines, isolated copies, verified edits, execution limits, and
failure categories. Source selection does not choose tests or their inputs.

Inventory the framework's projects and entry points. All tests means every
active test on the chosen platform, with unavailable native integrations named
separately. Freeze stable IDs, project, file, full name, outcome, and optional
duration. Keep declared platform skips as exclusions. A newly skipped, missing,
extra, retried, or unfinished test makes a column incomplete. A failing or
unstable original baseline stops mutation scheduling, preserving static results.

Use the supplied Vitest reporter with a verified compatible installed runner.
The product protocol supplies counts and failure IDs, not a full passing-test
inventory or general test-value report. The checkout's experimental
`research/test-value/vitest-reporter.ts`, `runner.py`, and `analyze.py` supply
inventory, execution, and attribution. Inspect their limits before reuse. The
standalone loop selects a prefix and lacks a whole-campaign deadline/incremental
disk quota. Its analyzer expects complete columns and emits all pairwise test
overlaps. A project adapter and bounded scheduling may be necessary; count that
setup cost. Unavailable attribution remains unavailable instead of being
manufactured from counts.

Identify what tests launch and rebuild changed Electron main/preload/renderer,
CLI, hub, or other bundles after each mutation. Hash declared build inputs and
outputs; verify imports resolve to fresh worker packages. Reuse unchanged outputs
only with verified relevant inputs. Stale bundles produce false survivors.
Validate a failing edit, restored passing source, and unaffected tests before
accepting measurements.

Install once. Use an owned dependency snapshot with worker-local workspace
links and private profile/cache/temp directories. Keep dependencies read-only
where supported; hash before/after and invalidate reuse if they change. A live
`node_modules` pointing back to developer source is unsuitable. Remove settled
workers after retaining their compact evidence.

## Spend the budget

Reserve time for new-patch validation, PR updates, and cleanup. Start with one
outer worker and safe project test concurrency. Measure full build/test/cleanup
time and peak disk in a small pilot. Increase concurrency only when measured
throughput improves within CPU, memory, display, and disk limits. Avoid outer
workers multiplied by unrestricted test-file workers.

Estimate affordable sample size from measured throughput. Stop starting columns
that cannot finish before the reserved deadline. Use command timeouts and
workspace/log/report budgets, plus monitoring or quotas for the entire run's
downloads, dependency store, profiles, and evidence. Workspace limits alone
are not a total disk budget. Stop at the user budget or free-space threshold;
monitor host storage too when a local VM grows its disk image.

Record a deterministic sample spread across source areas and mutation operators.
Include important boundaries discovered during inspection; keep targeted and
broader samples distinct. Avoid exhausting an alphabetical prefix. Record each
site's scheduled, completed, unknown, or not-run status.

For a full-pool test-value column, execute every active baseline test and retain
all assertion killers. Disable early bail and retries. Timeouts, missing
inventory, build/import/hook/runtime errors, and uncertain cleanup are unknown
evidence. Intended errors can be tested through assertions on their behavior.
Coverage can guide scheduling, but is not the usefulness score. Focused slices
can investigate survivors cheaply; keep them separate from full-pool columns.

Store baseline inventory once and killer IDs per accepted mutant, binding
source, tool, input, and configuration identities. Infer non-kills only for
confirmed executed tests in complete columns. Keep compact records, capped
failure logs, and useful nearest overlaps instead of a quadratic review report.
Checkpoint completed columns/batches. Changed patches need a fresh baseline
and matrix identity; combine only compatible evidence.

## Make useful changes

Derive subsuming requirements from the fixed baseline test pool and accepted
sample. Group identical nonempty kill signatures and retain minimal nonempty
killer sets. Empty signatures do not subsume killed mutants. Hold that target
fixed for exclusive kills, removal loss, and test overlap.

These measurements describe the observed sample. Zero contribution can mean
overlap, insufficient mutants, unsupported behavior, or a different contract.
Earlier irudd-ts historical trials included real-fault-detecting tests with zero
exclusive contribution. Keep cost separate and retain shared requirements when
considering a group. Removal needs contract review and rerun evidence for the
remaining suite, beyond a green original run. Report candidates when this
cannot be established.

Add survivor tests for established observable behavior. Show the original
passing, motivating mutant failing by an assertion, and unaffected controls.
Equivalent changes and uncertain requirements remain explained survivors.
Validate a few useful improvements rather than optimize a scalar score.

Checkpoint into the existing draft before a longer batch. Keep compact
before/after evidence in the description and raw artifacts outside Git. Validate
source changes before pushing. A deadline preserves the last validated patch
and unfinished schedule; static improvements have already been delivered.
