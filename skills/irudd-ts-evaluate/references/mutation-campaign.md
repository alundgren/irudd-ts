# Budgeted mutation campaign

Start after static findings are checkpointed in the draft PR or local patch.
Mutation detects fault sensitivity; it is not a probability of detecting real
bugs. Publish the actual inventory, selection, completion, and exclusions.

## Record selection before execution

Keep these decisions separate in the external run manifest. Record them before
the first mutant, then preserve changes as new experiment identities.

| Decision | Record |
| --- | --- |
| Source discovery | Source selectors, exclusions, language support, analysis limits, completeness, and planned-site count |
| File sample | Selected responsibilities/files and why they matter, without presenting a qualitative choice as a coverage or defect ranking |
| Site sample | Exact mutation IDs, operators, sample size, reproducible selection method/seed, and targeted additions kept separate |
| Test selection | Declared projects, active tests, commands, platform exclusions, and the reason for any narrower pool |
| Site/scenario relationship | Entry point, inputs and assertions expected to distinguish each selected edit; inspected or measured evidence and remaining uncertainty |
| Required patch validation | Repository-required commands and project triggers for the actual patch, separate from the mutation test pool |

The default test pool is every active test across declared projects on the
chosen platform, including projects omitted from routine ready checks. Reduce
the mutant sample first when time is tight. A narrower pool requires a recorded
task constraint, measured budget limitation, or platform limitation. Explain
which projects/files were omitted and bound every result to the declared pool.
A routine patch-validation trigger does not prohibit broader evaluation or
prove an omitted test irrelevant.

Inspect scenarios for each sampled site before scheduling. For a CLI exit-status
edit, a test must enter that CLI command with unfinished work and assert its
exit status. A test that publishes through the same executable supplies no
evidence about that branch. For a `>` to `>=` size-limit edit, oversized input
does not distinguish the edit; input exactly at the maximum can. Use coverage
when available to check execution, while remembering that execution alone does
not establish a distinguishing assertion. Do not invent coverage from names,
imports, or functional-area similarity.

If no matching scenario is found, include another existing test file/project,
record the site as intentionally investigating an untested path, or select
another site with the revised sampling reason. Keep any later explanation
separate from the reasons recorded before execution.

## Establish execution

`archguard mutator plan --root PROJECT --config PLAN_CONFIG --json` lists
source-hashed edits and executes no tests. `archguard mutator run --root PROJECT
--config RUN_CONFIG --json` requires a trusted command and declared workspace
inputs. Read the current code-quality/protocol guides for schemas. The product
handles fresh baselines, isolated copies, verified edits, execution limits, and
failure categories. Source selection does not choose tests or their inputs.

Prefer the product `mutator run` for aggregate kill/survive results. Use its
supplied Vitest reporter with a verified compatible installed runner, or another
explicit trusted protocol adapter. The product protocol supplies counts and
failure IDs, not a full passing-test inventory or a general test-value report.
Do not manufacture per-test non-kills or overlap measurements from those counts.
The product classifier does not compare baseline and mutant test counts. For
evaluation, compare the baseline and every mutant's `passed + failed` count
with the frozen active count, and compare `skipped` with the declared skip
count. Treat any mismatch as unknown evaluation evidence, even if the raw
product result reports killed or survived. Preserve that raw result alongside
the evaluation classification. Matching counts do not prove that the same
test IDs executed; claims about full test identity/completion require an
inventory-capable adapter.

When per-test attribution is required, the matching checkout's experimental
`research/test-value/vitest-reporter.ts`, `runner.py`, and `analyze.py` supply
inventory, execution, and attribution. This is a research integration, not
product `mutator run` execution. Disclose the selected runner, adapter, setup
cost, controls, and unsupported behavior in the report. Inspect its limits
before reuse. The standalone loop selects a prefix and lacks a whole-campaign
deadline/incremental disk quota. Its analyzer expects complete columns and
emits all pairwise test overlaps. A project adapter and bounded scheduling may
be necessary; count that setup cost.

Freeze the selected projects and expected active counts before baseline. For
per-test evidence, also freeze stable IDs, project, file, full name, outcome,
and optional duration. Record declared platform skips as exclusions before
execution. The current research adapter rejects skipped inventory entries,
including tests skipped by a name filter. Use file/project-level focused
validation that produces a complete active inventory, or an adapter with
verified support for an explicit inventory. Do not drop skipped entries after
seeing outcomes or weaken completeness to accept a passing named test. If
declared platform skips still appear, attribution through this adapter remains
unavailable until selection can represent the active pool correctly.

A newly skipped, missing, extra, retried, or unfinished test makes a per-test
column incomplete. A failing or unstable original baseline stops mutation
scheduling, preserving static results. Finish dependency setup and expensive
tool builds before measuring the baseline so resource contention does not
become an apparent production defect.

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

Use the agreed completion point as well as the budget ceiling. A completed
declared sample, reviewed useful survivors, validated changes, and a clear
account of remaining sites can finish a useful evaluation before time runs
out. State that completion reason. A deadline stop, an unfinished declared
sample, or unsupported required analysis remains partial. Do not call a
full-pool campaign complete after silently replacing its pool with a narrow one.

Record a deterministic sample spread across source areas and mutation operators.
Include important boundaries discovered during inspection; keep targeted and
broader samples distinct. Avoid exhausting an alphabetical prefix. Record each
site's scheduled, completed, unknown, or not-run status.

For a full-pool test-value column, execute every active baseline test and retain
all assertion killers. Disable early bail and retries. Focused slices can
investigate survivors cheaply; keep their inventories and results separate from
full-pool columns. They do not rewrite the original matrix or justify a reduced
routine test suite.

## Interpret outcomes and retain evidence

Require a complete clean original baseline and validated execution evidence.
Keep strict assertion evidence separate from other test-run failures.

| Measurement | Required evidence |
| --- | --- |
| Assertion kill | Valid completed run with assertion failures, executed failure IDs, frozen active/skip counts, and no invalidating execution errors; per-test columns also require the full matching inventory |
| Survivor | Valid completed run with all reported active tests passing and frozen active/skip counts; per-test columns also require the full matching inventory |
| Unknown | Runtime/import/hook/unhandled failures, timeout, setup/build/environment problems, count mismatch, missing or changed inventory, invalid metadata, or uncertain cleanup |

For unknown results, report whether tests failed, execution never reached a
valid test run, or evidence/cleanup was incomplete. Separately describe failures
observably caused by a mutant, such as startup rejecting a changed token
condition, and adapter/build/environment failures. The research runner's
`infrastructureErrors` field includes non-assertion runtime failures; that field
name does not prove an infrastructure defect. Where the cause is uncertain,
say so. A mutant-caused runtime failure remains unknown for assertion-based
attribution, even when it breaks behavior. A generic nonzero exit is no kill.
Intended errors can be tested through assertions on their behavior. Mixed
assertion/runtime failures remain unknown under the current contract.

Store baseline inventory once and killer IDs per accepted mutant, binding
source, tool, input, and configuration identities. Infer non-kills only for
confirmed executed tests in complete columns. Keep compact records, capped
failure logs, and useful nearest overlaps instead of a quadratic review report.
Checkpoint completed columns/batches. Changed patches need a fresh baseline
and matrix identity; combine only compatible evidence.

Follow the agreed retention choice. A compact resumable bundle can contain the
manifest, configs/adapter identity, exact sample and remaining schedule, one
baseline inventory per experiment, outcomes/killer IDs, and capped diagnostic
logs. Keep sources recoverable by commit or one snapshot plus small patches;
do not retain full source snapshots per mutant or verification run. Deduplicate
or compress shared evidence when useful. Record retained paths and sizes, and
remove owned worker sources, profiles, dependency copies, executables, builds,
and caches when no longer needed. If evidence is removed, report what remains
and which measurements can no longer be independently reproduced from retained
artifacts. Do not recreate evidence the user asked to delete.

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
and restored source passing, motivating mutant failing by an assertion, and
unaffected controls.
Explain suspected equivalent changes as code-review inferences unless stronger
evidence exists. Uncertain requirements and uninvestigated survivors remain
unresolved; they are not automatically equivalent or untestable.
Validate a few useful improvements rather than optimize a scalar score.

Checkpoint into the existing draft before a longer batch. Keep compact
before/after evidence in the description and raw artifacts outside Git. Validate
source changes before pushing. Describe what the patch actually changes. SQL
creating an already-supported schema in a test is a regression fixture, not
a production migration. Correcting a stale version number in docs does not
introduce a new schema version. A deadline preserves the last validated patch
and unfinished schedule; static improvements have already been delivered.
