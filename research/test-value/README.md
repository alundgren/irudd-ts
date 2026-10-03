# Individual test value experiment

This prototype records a full mutant-by-test matrix for an explicitly selected test pool. It calculates per-test removal loss against a fixed set of dynamic subsuming requirements, reports overlap, and groups identical nonempty requirement profiles. Cost stays separate. Zero individual loss is a reason to inspect shared responsibility, not a deletion instruction.

The optional tools have no product dependency or protocol changes. Python uses its standard library; the JavaScript reporters use Archguard's existing mutation SDK and the explicitly supplied installed runner. The Rust slice uses cargo-mutants 27.1.0, installed separately under its MIT license. Source copies retain their upstream notices. T3 Code and Scope dependencies are copied from explicit installations and hashed; they are never discovered or executed by Archguard's scanned-repository analysis.

## Run and inspect

Build the product CLI with the pinned Rust toolchain before measurement:

```sh
cargo build --release --locked --bins --examples
```

Run meaningful analytical, experiment-identity and real native classification controls:

```sh
RUSTC_FOR_TESTS=/absolute/pinned/rustc python3 -m unittest discover \
  -s research/test-value -p 'test_*.py' -v
```

The optional sensitivity controls also need `TEST_VALUE_EVIDENCE` pointing to an experiment root with prepared `history/t3code-dependencies.json` and its owned dependency store, and `TEST_VALUE_NODE` naming the pinned T3 executable. Set `TEST_VALUE_CONTROLS_OUTPUT` to a new directory to retain the actual failing, corrected and negative-control events. Without the two Vitest inputs, that optional control is explicitly skipped.

[runner.md](runner.md) documents the owned-source runner, configuration and real Vitest/Node controls. Install dependencies independently before a run. Each run has fresh source and private home, temporary and cache directories. Workspace dependency imports point into that execution's source, and external dependency bytes are copied once into an owned store. Missing, duplicate, skipped, retried or unfinished tests make the column incomplete. Runtime and hook errors are unknown outcomes, not assertion kills.

```sh
python3 research/test-value/runner.py run --config /absolute/config.json \
  --output research/local/new-matrix
python3 research/test-value/analyze.py research/local/new-matrix/matrix.json \
  --output research/local/new-matrix/analysis.json
```

Current TypeScript slices and historical source-reversion experiments can share owned dependency stores. First prepare them with `history.py --prepare-only` and the same explicit dependency and runtime arguments used below. Then run the declared current slices with `experiments.py --stores STORES --output NEW_OUTPUT --archguard CLI --t3-repository T3_SOURCE --scope-repository SCOPE_SOURCE --t3-node T3_NODE --scope-node SCOPE_NODE`. Each Node path must name the actual executable; a runtime-manager launcher is rejected. The copied Vite+ package selects its matching Vitest entrypoint, which runs directly under that pin. Reporter evidence must agree with the executable path, version and hash. This also avoids downloading a separate runtime in every private cache.

```sh
python3 research/test-value/history.py \
  --candidates /absolute/candidates.json \
  --output research/local/new-history \
  --archguard target/release/archguard \
  --t3-dependencies /absolute/installed-t3 \
  --scope-dependencies /absolute/installed-scope \
  --t3-node /absolute/actual-t3-node \
  --scope-node /absolute/actual-scope-node \
  --mutant-limit 50
```

The candidate file has a `candidates` array. Each entry names `id`, `subject` of `t3code` or `scope`, `repository`, exact `fix` and first `parent` commit hashes, `sourceFiles`, `testFiles`, `title`, and `relatedGroup`. The operator set and prefix limit are declared before outcomes are observed. Reusing an output directory with changed candidates, tools, operators, limits or dependencies fails. Start a new directory for a changed experiment.

A declared `preflightExclusion` records a candidate that the configured harness cannot safely run, before archiving or test execution. Such candidates stay in the cohort ledger with `executionAttempted: false`. The overnight Node-only cohort excludes one Electron/Playwright integration harness after inspecting its launch helper.

The native command requires explicit `--repository`, `--revision`, `--cargo`, `--rustc`, `--cargo-mutants`, and a new `--output` directory. It selects binary/unary mutations in production lines 1 through 211 of `src/mutator/result.rs` and runs three frozen inline tests. Its assertion classification requires one actual panic at a verified assertion statement in those tests. Other panics and incomplete runner output are unknown. Build duration is not a per-test cost measurement.

```sh
python3 research/test-value/report.py --root research/local/EXPERIMENT \
  --output research/local/EXPERIMENT/report.html \
  --secondary-root research/local/SEPARATE_POSTHOC_ANALYSIS
```

The report verifies each retained analysis against its matrix digest. An optional `report-metadata.json` under the experiment root supplies `handoff`, `pullRequest`, `validation`, `evidenceRoot`, `disk`, and `review`. It cannot override measured subjects or historical results.

Execution scripts stop at 12% free disk space, ahead of the requested 10% boundary. Unknown process ownership or cleanup preserves source copies and stops further execution. The runner supports Linux and macOS process-group observations. These experiment commands do not launch the desktop app.

## Interpret the output

`analysis.json` includes completed raw kills, unexplained survivors, minimal nonempty kill-signature classes, exclusive contributions, individual removal loss, Jaccard overlap, identical-profile groups, and an exploratory greedy core. Equal kill signatures count as one requirement. A strict subset of killing tests is the stronger observed requirement. Empty signatures cannot subsume killed mutants.

The baseline retains all its own requirements by construction. The report calls this baseline requirement retention. It does not reinterpret that 100% as general subsuming mutation adequacy or real-bug probability. Only independently established compiler-rejected mutations can be excluded while retaining matrix completeness. Invalid inputs, stale source hashes and failed execution remain incomplete.

The historical runner requires fixed pass, parent-source reversion assertion failure, restored pass, and unaffected tests. It uses fixed-revision tests and modern installed dependencies, so its output is an adapted source-reversion replay. It records parent test-file availability and matching test identities, but equal identities do not establish unchanged test bodies. Fix-added or changed regressions make the cohort retrospective.

Ranking trials simulate equal test-count subsets from the matrix and fault labels. They do not rerun those subsets. This assumes full-pool per-test failure outcomes remain applicable to the selected subsets; interactions and order effects can violate that assumption. Seeds are averaged within faults before aggregation. The bootstrap resamples related-fault groups, not seeds. No selector is claimed to be scientifically validated as a test-retention score.

Historical mutants target the same reverted files selected from the known fix. Both that known-location selection and the fix-revision tests can favor detection of the recorded change. This experiment does not establish prospective whole-repository ranking ability. Partial current matrices expose contribution on completed columns only; unknown columns can conceal further unique contribution.

Historical trials retain selection counts and digests instead of repeating every test ID thousands of times. Digests bind the ordered selection to its budget, seed and strategy; matrix, test inventory and selector-code hashes are fixed alongside them. `reconstruct_selections(evaluation, matrix)` reproduces and checks the exact selections. Compact storage preserves the detection and retention outcomes and keeps large parameterized inventories within the JSON byte budget.

## Posthoc sensitivity analysis

The primary policy counts assertions only. `sensitivity.py` provides a separately labeled analysis specified after observing its exclusions. It accepts complete Vitest test-body failure records while rejecting global, hook, import, unhandled, identity and cleanup problems. The Node adapter is excluded because its retained records lack the failure origin needed for this distinction. For the native slice, only `Result::unwrap` failures at exact frozen test-source lines are added. Assertion and other failure identities remain separate.

```sh
python3 research/test-value/sensitivity.py --root PRIMARY_SLICE --output NEW_SECONDARY_SLICE
python3 research/test-value/sensitivity.py --root PRIMARY_NATIVE --output NEW_SECONDARY_NATIVE --native
```

The derived matrix binds original event hashes and the primary matrix hash. It calculates its own fixed requirement set without changing primary bytes. Compare denominators as well as kills. Exceptions can reflect defensive checks or incidental crashes; more kills do not establish stronger assertions. [Du, Palepu and Jones, ISSTA 2023](https://superhangdu.com/publications/issta-23/conference-paper.pdf) studies this distinction. [Stryker's outcome definitions](https://stryker-mutator.io/docs/mutation-testing-elements/mutant-states-and-metrics/) distinguish individually failed tests from test-runner errors. This secondary analysis does not replace the historical assertion-regression eligibility controls.

Runtime fields exclude shared setup and startup. Repeated clean baselines do not prove zero flakiness. Historical churn and ownership cost were not measured. A minimal observed core remains exploratory until actual subsets, requirement coverage outside sampled mutants, and historical fault behavior are checked independently.
