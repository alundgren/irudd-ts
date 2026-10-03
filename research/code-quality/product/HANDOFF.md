# Fresh-VM handoff

The user asked to stop today's work and retain everything needed to resume in one PR against `main`. [PR #15](https://github.com/alundgren/irudd-ts/pull/15) uses `t3code/dryer-mutator-research`. Leave it unmerged. Earlier child PRs are already merged into that branch; there is no additional branch or local worktree required for tomorrow.

## Delivery and review status

Structural duplicate detection and mutation execution are implemented for TypeScript/TSX through the CLI, public Rust APIs and versioned JSON. The product uses existing dependencies. The SDK uses Node builtins. No CI, score target, automatic runner discovery or additional product library was added.

Start with [the user guide](../../../docs/code-quality.md), [protocol](../../../docs/mutator-test-protocol.md), [examples](../../../examples/code-quality/README.md), [findings](FINDINGS.md), [independent review](FINAL-REVIEW.md) and [evidence archive](evidence/README.md). The initial reference-tool/jscpd/Stryker research is retained [separately](../findings.md).

Source review approved the coordinator implementation and cleanup correction at `e2f3d3740253578d5d66e9bb847c4f8d8f19975a`. This is source approval, not unconditional product-ready approval. The PR remains draft while final acceptance is pending.

The earlier required full check passed at `3f9f4e6`. That version also passed extracted-package build and public command execution, strict TypeScript checks, all 24 authored duplicate comparisons, domain weak/strong runs, and macOS all-feature binary/example cross-compilation. Final reporter tests contain twelve cases; together with eighteen SDK and one example test, all 31 Node tests passed. The isolated Linux pressure control returned descriptors 4→4 and threads 2→2, with no surviving children or owned workspace. Exact revisions, logs, resource observations and limitations are retained; they do not prove absence of every possible leak.

Two real Git parser replays reproduced uncertain process cleanup when a mutation stopped advancing the parser loop. The runner correctly preserved its workspace and reported incomplete analysis. The correction separates five-second process-group cleanup from 250 ms pipe draining. Deterministic regression controls preserve the unreaped leader identity, demonstrate failure with the old grace and success with the new grace, and fail closed for persistent live members or unavailable observations. The corrected real Git replay finished all 26 sites: 22 assertion kills, three survivors and one genuine nonadvancing-loop timeout, with no execution or cleanup errors. The baseline passed 22 tests, source bytes were preserved and the active journal was removed. The report remains incomplete because timeouts do not count as kills. The final required check passed at the corrected source revision. Its completed log is archived at `evidence/logs/archguard-quality-product-cleanup-final-required-check.log`.

Native macOS execution has not been tested. Cross-compilation is separate evidence. Full T3 browser/server suites and live user data were not exercised.

## Start on a fresh VM

No installation, executable, cache, checkpoint, process, private workspace or `/tmp` path from today is required. Historical absolute paths in archived reports identify their original inputs; regenerate profiles for new paths. Do not attempt to restore an old active journal or signal any recorded PID. The old machine's retained workspaces are diagnostic history, not resumable state on another host.

Use Rust 1.96 or newer, Node 24, Python 3 and `gh`. Check out PR #15, then install only the documented optional validation environments:

```sh
gh repo clone alundgren/irudd-ts
cd irudd-ts
gh pr checkout 15
npm ci --prefix providers/typescript7 --ignore-scripts --no-audit --no-fund
npm ci --prefix research/t3code/semantic --ignore-scripts --no-audit --no-fund
python3 research/code-quality/product/evidence/verify.py
scripts/check.sh
cargo build --release --locked --bins --examples
```

Read [architecture](../../../docs/development/architecture.md), [development setup](../../../docs/development/README.md), root `AGENTS.md` and `research/AGENTS.md` before making changes. The existing compiler pin is TypeScript 7.0.2. No runtime compiler-type claim follows from Oxc parsing.

The small authored domain examples require no npm dependency installation:

```sh
node examples/code-quality/prepare-domain.ts /tmp/archguard-domain-tomorrow
./target/release/archguard dryer --root . --config examples/code-quality/dryer.json --json
./target/release/archguard mutator plan --root . --config examples/code-quality/plan.json --json
./target/release/archguard mutator run --root . --config /tmp/archguard-domain-tomorrow/weak.json --json
./target/release/archguard mutator run --root . --config /tmp/archguard-domain-tomorrow/strong.json --json
node research/code-quality/product/replay-dryer.mjs "$PWD/target/release/archguard" "$PWD" /tmp/archguard-dryer-tomorrow
```

## Recreate the focused T3 and SDK experiments

Use a separate T3 checkout at the exact research pin. T3's own lockfile and optional installation are independent of Archguard's product dependencies. Today's package export used pnpm 11.10.0 and retained nine explicit package roots with their physical dependency closure. The archived installation inventory and licenses identify the original packages. An installation on another OS can have different native package bytes, which must invalidate reuse.

From outside the Archguard checkout:

```sh
gh repo clone pingdotgg/t3code /tmp/t3code-quality-tomorrow
cd /tmp/t3code-quality-tomorrow
git checkout 31a9da179ed0763335f05681c577474aec5d2309
corepack pnpm --filter @t3tools/shared --filter @t3tools/web install --frozen-lockfile --ignore-scripts
```

Return to Archguard and create a self-contained package copy. The research exporter installs nothing, accepts only a new destination outside T3, preserves the physical pnpm dependency closure and rejects links outside it. Its actual export reproduced today's 196 directories, 10,247 files and 286,134,017 bytes. Destination and symlink rejection controls are retained.

```sh
python3 research/code-quality/product/copy-dependencies.py /tmp/t3code-quality-tomorrow /tmp/archguard-quality-dependencies-tomorrow
node examples/code-quality/prepare-t3.ts /tmp/t3code-quality-tomorrow /tmp/archguard-quality-dependencies-tomorrow /tmp/archguard-t3-tomorrow
node research/code-quality/product/prepare-self.mjs "$PWD" /tmp/archguard-quality-dependencies-tomorrow /tmp/archguard-self-tomorrow
./target/release/archguard mutator run --root /tmp/t3code-quality-tomorrow --config /tmp/archguard-t3-tomorrow/gitPatchPath-strengthened.json --json
```

The supplied generators create current-source profiles, not byte-identical historical trials. Frozen source, original test adaptations, strengthened test copies, configuration and per-run hashes are in `evidence/`. Never relabel a current twelve-test reporter profile as the historical original-five trial. Generated absolute paths should be regenerated, not edited into archived historical bytes.

For later self-dogfooding, use generated `protocol.json` and `reporter.json` with the Archguard root. Larger profiles can take tens of minutes. Run one selected profile at a time while checking free space; each configured worker receives a full private copy. Commands are explicitly trusted and these copies are not a hostile-code sandbox. Reuse is off by default; an explicitly configured external state directory and `declaredInputs` reuse still require a fresh baseline and identical full declared inputs.

## Useful evidence already obtained

- The authored A–F duplicate cases expose threshold misses and intentional boilerplate. Erasing properties recovers the 45-line copy but increases incidental similarity. Set, counted and weighted comparisons remain visible; no universal threshold is established.
- Domain weak tests produce four kills and seven survivors; behavior corrections produce ten kills and one qualified equivalent. A mistaken earlier equivalence claim was disproved by negative zero and retained with its correction.
- T3 path supplemental tests catch two real public behavior gaps. Two equivalent cases and two drive-root specification questions remain.
- CSV supplemental tests eliminate eight survivors at the longer explicit test deadline. Fifty complete kills are reused after a fresh baseline; two genuine nonprogress timeouts rerun and remain incomplete.
- Broader existing T3 host tests remove 88 focused-selection survivors. The original 153 survivors were classified separately from the supplemental runner outcomes, including 47 source-proven equivalents. This does not mean T3 had 153 defects.
- Eleven protocol SDK public gaps are directly proved by old-test PASS/new-test FAIL controls. The full strengthened run detects five with assertions and six with ordinary runtime errors. Runtime errors do not become kills; nine remaining survivors have retained classifications.
- Reporter retry reuses 151 complete results and reruns the three original timeouts after a fresh baseline. It completes with 66 kills and 88 survivors, classified as 46 useful public controls, 40 implementation details and two qualified equivalents. The original timeout report and why-unresolved notes remain intact. Eight before/after proof pairs support the selected new public tests.

## Next acceptance work

The user explicitly stopped further checks and tests during delivery. The remaining items below are deferred work for a later session; they were not rerun to prepare this PR and handoff.

1. Read the archived end-of-day status and final review. The final required check passed at `e2f3d37`. Keep PR #15 draft until the remaining acceptance work is reviewed.
2. Verify the corrected Git profile attempts all 26 sites, kills the escaped-single-character gap, retains the two equivalent cases and empty-token specification question, and reports the nonadvancing loop as a timeout with confirmed cleanup rather than an assertion kill. Keep any new adverse outcome.
3. Run the exact final revision's release pressure control alone after building release binaries/examples:

```sh
cargo test --release --locked --test mutation_execution repeated_linux_execution_resources_remain_bounded -- --exact --ignored --nocapture --test-threads=1
```

4. Repackage the final revision, inspect the allowlist, and build/test the extracted archive. The retained successful package validation belongs to `3f9f4e6`, before the cleanup correction and final reporter test delta. Do not present it as exact-final validation.
5. Obtain independent review for any further source fixes and final evidence. When acceptance is complete, update PR #15 and mark it ready. Do not merge it into `main`, add CI, impose zero duplicates or target a perfect mutation score.

The evidence verifier checks integrity and recorded contracts, not authenticity or universal equivalence. Keep complete/incomplete outcomes, source selections, test-selection gaps and source/runtime identities visible in the review.
