# Resume the historical experiment

The user asked to investigate the seven unavailable baselines and wrap up for later continuation. The larger historical mutation experiment has not started. Keep the final PR open and do not merge into `main` without new authorization.

## Completed evidence

The working prototype measures full per-test kill matrices, fixed-baseline dynamic subsumption, removal loss, overlap, identical nonempty requirement groups and an exploratory greedy core. Current selected slices cover Archguard, T3 Code and Scope: 104 tests and all 199 declared mutations, with 144 usable columns and 30 observed requirements. The 25-test greedy core preserves those recorded requirements across the four separate slices. Reduced suites were not executed.

The original historical profile has five verified adapted source-reversion faults and three rankable matrices. The requested 30–50 verified historical pairs remain unfinished. The separate frozen 78-candidate profile has 46 prepared and independently rehashed dependency stores. Its baseline-only validation attempted 75 candidates: 68 passed, seven were unavailable, and three declared preflight exclusions remained. It produced no new fault labels or mutation results.

Read [the result record](README.md), [the baseline summary](provenance/closeout-v1/lock-matched78-baseline-summary.json), and [the seven-failure investigation](provenance/closeout-v1/historical-seven-failure-handoff.md). The accompanying JSON records exact historical commits, source files and evidence hashes. Every attempted baseline confirmed process cleanup and its frozen runtime and runner identities.

## The seven unavailable baselines

| Candidates | Cause | Proposed next step |
| --- | --- | --- |
| Scope `c78a8c5f0fb1`, `392d29f5779d`, `48708ad54e8b`, `c152601dfe9c`, `59ed3a2a3ee4` | Missing generated `packages/cli/dist/main.mjs` | Audit and configure the exact package's `vp pack` command in each fresh execution source. |
| Scope `e65e8dd2c3af` | Missing generated `apps/plan-web/dist/server/server-main.mjs` | Audit and configure server packing in `apps/plan-web`; the selected test constructs its own frontend static fixtures. |
| T3 Code `a9c9995352e5` | Missing `kiro_unknown_model/kiro_transcript.ndjson` fixture in the fix and first parent | Try a separately declared canonical fixture overlay from descendant `4ddea52c8283cda3b8df9218ed24c3c6293fc1b3`, blob `8e944bc546b4febb8741fb48f78ff182474657d6`. The test-file blob is unchanged there. |

These are investigated repair proposals, not successful repairs. No build, fixture overlay or replacement test assertion was executed during diagnosis. Retain the original seven unavailable records.

Building the fixed template once is insufficient. Compile after every source reversion or mutation, inside that execution's owned source, before running tests. Otherwise the tests can execute an unchanged compiled bundle. Record the configured command, working directory, actual compiler/runtime identities, output digests and effective source hashes. Preserve source and dependency checks, deadlines, publication checks and owned process cleanup. Build failure must remain incomplete evidence.

The package scripts name `vp pack`, and the full Plan Web build script additionally names `vp build`. Those commands have not been validated here. Inspect their actual launch helpers before host execution, as `/Users/alun/AGENTS.md` requires. Putting Node first on `PATH` alone does not prove that Vite+ retains that runtime or avoids downloads. Resolve and pin the actual build entry and runtime, then validate them. Keep commands explicit; do not discover and execute arbitrary repository scripts. The selected tests use local fake services, not live providers or Kiro sessions.

A fixture overlay changes the original execution inputs even when it uses a canonical Git blob. Record its exact bytes, source revision and upstream notice in a distinct setup profile, while preserving the original archive hash and unavailable result. Test that profile before fault replay. Do not claim whole historical environment fidelity.

## Local state to preserve

The implementation branch is `t3code/test-value-benchmark` in `/Users/alun/.t3/worktrees/irudd-ts/t3code-6f4260d3`. Historical baseline validation froze source revision `4a96d4e76bb3b1be0b8e37b456a02301ad299e14` and runner fingerprint `6ea7bd34ee830feed10ebfb844797ee0488df79ef95470626c40187bdc9a3546`. Later closeout changes add report wording and a payload regression control; they do not change measured matrices.

The prepared stores are under `research/local/test-value-20261003-lock-matched/`. That directory contains preparation evidence and all 46 owned stores. It has no historical mutation registration or results. Baseline-only records are separately under `research/local/test-value-20261003-lock-matched-baseline-validation/`. Current measurements are under `research/local/test-value-20261003-final/`; current posthoc analysis is under `research/local/test-value-20261003-sensitivity/`.

Keep `/tmp/archguard-test-value-20261003/`, the prepared stores and their original donor installations. The frozen candidate file is `history-lock-matched-78-final.json`, SHA-256 `2a01d9d364071b7bc1b315f89a117d698a03693f74f6c8c647ce92461cb5b0a0`. The installation map is `dependency-installations-lock-matched-78.json`, SHA-256 `81ed6f1a0048b5819212c8817e1d0a724a3365e66128e8d9d3985521b72416a0`. Byte-preserved copies are under `provenance/setup-final-v1/`. Keep source archives, audited installer signatures, receipts, license evidence and all failed attempts.

Use Rust 1.96.0, cargo-mutants 27.1.0, T3 Node 24.21.0 and Scope Node 26.10.0. [The environment record](provenance/closeout-v1/before-historical-baselines-environment-v2.json) contains exact executable paths and hashes. Read root and research instructions before running commands.

Disk checks use `shutil.disk_usage`, with a 12% free-space stop ahead of the user's 10% boundary. The baseline run ended at about 28.5% free, 264 GiB. Do not rely on APFS `df` capacity percentages or delete unrelated files. Darwin source copies use private copy-on-write inodes, and actual backend evidence is retained.

## Continue in this order

1. Review the build and overlay plan independently. Implement only explicitly configured trusted preparation, with meaningful failure, correction and negative controls. Revalidate the seven cases and any other affected execution paths before measurements.
2. Run required checks and refresh the release CLI. Freeze new source, SDK, tools, runtimes, candidate/profile inputs, receipts and owned-store digests. Any changed execution profile needs a distinct registration and output directory; preserve the existing 78-candidate declarations and any superseded profiles.
3. Reserve an exclusive measurement window. Hold other builds, test runs, GUI activity, curation, scans and helper edits. Keep continuous disk and process-ownership checks active.
4. Run one whole declared historical profile with `--mutant-limit 24 --max-raw-units 16384`, 100 seeds and equal-size fractions 25%, 50%, 75%, 100%. Rerun clean fixed baselines; validation results are not reused as measurements. Preserve every failed replay and unknown column. Do not assemble each fault's best outcome from different profiles.
5. After the window ends, review and integrate the pending [historical sensitivity child PR26](https://github.com/alundgren/irudd-ts/pull/26), head `1779247e379da615978d9e7c89d6d06d6563fc1b`. It remains open and unmerged. Its offline controls passed, but required combined validation is still needed before integration. Its separately recorded posthoc plan is in `provenance/setup-final-v1/historical-sensitivity-plan-v2.json`.
6. Derive secondary outcomes without changing primary bytes or historical fault labels. Compare policies on paired eligible faults, reporting additional eligibility separately. Use related-fault groups for uncertainty, not seeds as independent faults.
7. Retain the new raw evidence and provenance, verify fresh offline reconstruction, review the resulting claims, and update the same Scope artifact `archguard-test-value-20261003`. Keep the final main PR open.

The existing prepared-store command uses `history.py --dependency-installations` with the frozen map and the prepared output directory. Do not run it unchanged before adding and validating the required build/overlay profile. Changed declarations must be re-audited and registered. The experiment README documents the argument contract; the retained helper and environment records supply the exact local paths.

The present evidence supports an interpretable view of observed contribution and redundancy. It does not validate a deletion policy, a scalar quality score, future fault prediction or ownership-cost ranking.
