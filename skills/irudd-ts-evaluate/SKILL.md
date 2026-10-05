---
name: irudd-ts-evaluate
description: Evaluate whether irudd-ts (Archguard) is useful for an existing repository through architecture policies, duplication review, and budgeted mutation testing, with an optional draft PR updated as evidence arrives.
---

# Evaluate irudd-ts

Find concrete improvements in the target repository using Archguard, the CLI
from `alundgren/irudd-ts`: enforceable architecture rules, reduced unintended
duplication, stronger tests, and test redundancy worth reviewing. An honest
finding that a capability is unsuitable is useful.

## Agree on the run

Start with read-only discovery of repository guidance, scripts, language,
installed tools, Git state, and available disk. Establish:

- The target repository, revision, execution machine, and test isolation.
  Preserve existing work in a separate checkout.
- A wall-clock deadline and an incremental disk budget. Count setup, downloads,
  builds, dependencies, workers, temporary data, caches, logs, and reports.
  Begin above 12% free disk and stop at 12%, or a stricter user limit. For a
  local VM, account for host storage as well as guest storage.
- Permission to create a draft PR in the exact target repository and push
  subsequent updates to that same branch. Ask explicitly if missing. A
  declined PR leaves a local patch and report; continue evaluation.
- A useful completion point and evidence retention choice. Complete the
  declared sample and review useful follow-ups within the budget; exhausting
  every site or spending the whole allowance is unnecessary. Agree on a compact
  evidence bundle, its location, and whether to retain it or remove it at finish.

Reuse answers and permissions already supplied. Propose reasonable budgets
when missing, state which setup/reporting time they include, and wait for
required answers before expensive execution. Agree on the outcome and
permitted changes. PR permission does not authorize merging, new CI,
production changes, or installing system services.

Record a small manifest outside Git: revisions, tool identity, commands,
budgets, deadline, owned directories, baseline disk use, and phase status.
Inspect launch helpers before tests; GUI tests need an isolated desktop. Put
profiles, HOME, TMPDIR, caches, and generated files in owned directories. Use
quotas where available and verify monitoring and command deadlines before
expensive work. Stop scheduling with enough time for validation, publication,
and cleanup. Cancel only owned processes; retain uncertain cleanup for
inspection. Keep reports and build artifacts out of commits.

Read [capabilities.md](references/capabilities.md) before selecting checks.
Use the installed revision's CLI and documentation; support varies by language
and command. Install/build Archguard once if needed within budget. Optional
providers and framework adapters need explicit setup; their absence leaves
that capability unavailable rather than block all work.

## Gather evidence and publish before mutation

Read project guidance and existing enforcement first. Inspect a bounded sample
of historical fixes using changed files, tests, and commit/PR explanations.
Prioritize repeated failures and documented ownership rules. Retain exact
commit references; stop history investigation when it no longer yields
actionable checks. Full historical replay is a separate experiment.

For each architecture proposal, record its requirement source, selected files,
supported rule/configuration, current violations, and limitations. Verify a
violating case, correction, and allowed case in an owned fixture. Include
relevant package roots, subpaths, and configured aliases in import controls.
A historical claim needs evidence that the rule would catch the relevant
pre-fix pattern; a synthetic example establishes only a proposed guard.
Preserve existing enforcement and explain the added value. Runtime bugs that
cannot be checked statically remain test suggestions.

Run Dryer over the requested authored source scope. Include supported tests
and tools for an entire-project request, but review those findings separately
from production duplication. Record language/generated-file exclusions,
candidate thresholds, analysis limits, and incomplete results. Inspect groups
and actual pair scores; indirect matches do not make members interchangeable.
Prefer a few useful refactors that keep distinct contracts explicit.

Prepare a small patch with validated rules, duplication improvements, or both.
If a change is not yet justified, include a concise evaluation record or
actionable proposal in the target's permitted location. Respect repositories
that keep proposals outside tracked docs. Avoid an empty PR; checkpoint locally
until a reviewable patch exists.

Run applicable checks within budget. Publish the authorized PR as a **draft
before any mutation execution**. Describe validated changes, unimplemented
suggestions, skipped checks, costs so far, and mutation as pending. If required
checks cannot finish, use the repository's permitted draft workflow and disclose
the limitation; never claim they passed. Use `gh` when available, otherwise an
authorized GitHub interface. Register PRs with thread-linking tools when exposed.
Without PR permission or available publication, checkpoint the same content
locally and continue within the agreed scope.

## Run mutations last and update incrementally

Read [mutation-campaign.md](references/mutation-campaign.md) when a supported
runner and enough budget remain. Default to every active test across declared
projects on the chosen platform, including suites excluded from routine checks.
Fit the budget by reducing the mutant sample first. A narrower test pool needs
a recorded task, measured budget, or platform reason and a claim limited to
that pool. Record source discovery, file/site sampling, test selection, and
required patch validation separately before mutation. Check whether selected
scenarios exercise the chosen sites; unknown execution remains unknown.

Prefer the product `archguard mutator run` for aggregate campaigns. Use the
experimental research runner only when per-test attribution is required, with
its setup and limitations disclosed. Compare aggregate active/skip counts with
the frozen pool; mismatches are unknown evidence even if the product reports
a kill or survivor. Counts alone do not prove matching test identities.
Follow the reference's selection and inventory rules for focused verification.

Measure a small pilot, then execute diverse, complete batches. Reserve time to
verify useful survivors, validate patches, publish results, and clean artifacts.
The draft's earlier useful changes survive later failure or a budget stop.

After each useful batch, checkpoint evidence and update the draft description.
Push new source/test commits only after required checks pass or the target's
documented draft exception applies. Avoid raw matrices, logs, and evidence-only
empty commits. The same PR can receive multiple mutation-driven updates. Later
failures leave the last validated patch intact and add incomplete results to
its description.

Add tests for survivors only when intended observable behavior is established.
Show original and restored source passing, the relevant mutant failing by an
assertion, and unaffected controls. Build/import errors, timeouts, changed
inventories, and runtime failures are separate from assertion kills. Explain
mutant-caused runtime failures separately from setup, build, environment, or
cleanup failures; uncertain causes stay uncertain.

Test-removal suggestions need complete per-test evidence, fixed subsumption
targets, overlap analysis, and separate runtime costs. Zero exclusive kills
does not justify deletion. Preserve shared requirements, review distinct
contracts, and verify any proposed removal. When attribution is unavailable or
the sample is too small, focus on duplication and missing tests. One run does
not measure flake rates or maintenance cost.

## Finish with a resumable result

Report changes and their reasons, useful/unsuitable capabilities, validation,
actual source/test scope, planned/executed/unknown mutants, elapsed time, and
disk peak/final use. Separate measurements from estimates. Retain compact
external evidence only as agreed, bound to exact inputs; rebaseline when inputs
change. Store shared inventories once rather than retaining repeated source
snapshots. Remove settled owned workers, builds, and caches and account for
retained files. Describe regression fixtures and stale documentation corrections
separately from production migrations or newly introduced schema versions.
Leave the draft open with the next useful action.
A budget stop is a partial result, not permission to overrun or present an
incomplete scan as clean.
