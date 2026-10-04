# Test value measurements

The current-suite experiment covers four selected CPU test slices across Archguard, T3 Code and irudd-scope. It measures which tests detect the declared mutations, what disappears when an individual test is removed, and which tests share the same observed requirements. It does not recommend deleting tests.

The user requested closeout after investigating the seven unavailable baselines. The requested 30–50 verified historical pairs remain unfinished. Read [the resumption steps](RESUME.md) before continuing.

[Open the interactive report](report.html). The same report was accepted and visually checked in Scope as `archguard-test-value-20261003`, revision 76. The [draft PR29](https://github.com/alundgren/irudd-ts/pull/29) remains open.

## Current-suite result

| Selected slice | Tests | Declared mutations executed | Usable columns | Killed | Unexplained survivors | Unknown | Compiler rejected | Fixed requirements | Exploratory core |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Archguard Rust mutation result | 3 | 65 | 29 | 10 | 19 | 35 | 1 | 2 | 2 |
| T3 Code execution state | 52 | 90 | 74 | 54 | 20 | 16 | 0 | 18 | 15 |
| T3 Code preview state | 46 | 32 | 29 | 19 | 10 | 3 | 0 | 9 | 7 |
| Scope event handling | 3 | 12 | 12 | 9 | 3 | 0 | 0 | 1 | 1 |
| Total | 104 | 199 | 144 | 92 | 52 | 54 | 1 | 30 | 25 |

Every mutation in each declared slice was executed. The Rust engine initially lists 218 mutation opportunities; the declared binary/unary production-line filter selects 65. The 218 inventory is not the measured denominator.

Forty-three tests that kill a fixed subsuming requirement have zero exclusive requirement loss individually. They share responsibility with other tests. Removing several such tests together can still lose a requirement. The 25-test greedy core retains the 30 observed requirements in these matrices; it was not validated by executing the reduced suite.

The assertion-only policy leaves 54 columns unknown. Those columns can conceal additional unique contribution. A separate posthoc analysis admits complete test-body failures while still rejecting runner, import, hook, identity and cleanup errors. It has 198 usable columns, 146 killed mutants and 33 requirements. The exclusive requirement counts for all 104 tests stay unchanged; normalized removal losses can change when the denominator changes. This sensitivity result does not replace the primary analysis.

Runtime is reported separately. Shared setup and startup are excluded, Rust per-test runtime is unavailable, and six clean baselines do not establish zero flakiness. Maintenance cost was not measured.

## Historical profiles

The original 50-candidate profile and its continuation are preserved separately from the repaired, lock-matched profile. Together the original attempts verified five adapted source-reversion faults; three have complete matrices usable for equal-size ranking simulations. They are too few to establish a useful ranking advantage.

The new 78-candidate profile is frozen, with exact source-archive hashes and installer-input signatures. All 46 owned historical dependency stores were prepared and independently rehashed. Baseline-only validation attempted the 75 runnable candidates: 68 passed and seven were unavailable. One unavailable candidate needs a missing Kiro transcript fixture, one needs a built Scope web server, and five need a built Scope CLI. Three declared preflight exclusions remain: two Bun-only candidates and one Electron test slice. All 75 executions confirmed process cleanup and their pinned runtime identities. No fault replays or mutations have run for this new profile.

The seven setup failures are recorded in [the baseline summary](provenance/closeout-v1/lock-matched78-baseline-summary.json). The full baseline ledger and raw execution events are retained in [the baseline archive inventory](lock-matched-baseline-validation/manifest.json). Build-based test slices need fresh compilation after each source mutation or parent-source reversion; building only the fixed template would test stale code.

These are retrospective reproductions using fixed-revision tests and parent-source reversions. Across the frozen 78 candidates, 98 selected test-file instances changed in the fix and six were added. None were unchanged. Their 104 file instances represent 82 distinct repository/path pairs. Tests with unchanged names can still contain changed assertions. This design cannot establish prediction of future faults using only tests that existed before the fix.

Mutation locations are also selected from the known fix. Equal-size comparisons simulate selections from the full-pool matrices and fault labels; they do not execute each subset. Related-fault groups, rather than random seeds, are the bootstrap sampling units.

## Retained evidence

Each profile has a deterministic `raw-evidence.tar.gz` and a `manifest.json` listing every retained member, byte count and SHA-256. Analytical records are also available under `data/`. Archive hashes, member hashes and exported loose copies were checked. Fresh restoration of the retained current, native, original historical and posthoc profiles successfully regenerated the offline report.

Mutable source copies, third-party dependency stores, private homes and caches are omitted. Their recorded identities and hashes remain in the receipts. The provisional Vite+ launcher attempt is retained for audit and excluded from the primary result because its actual runtime pin was not established. Earlier failed installations and incomplete executions remain visible rather than being replaced by successful retries.

Optional dependency installations retain their upstream license files privately. License evidence distinguishes the proprietary Cursor SDK from permissively licensed packages. Dependency source and binaries are not included in these retained archives.

[The source notice inventory](notices/manifest.json) retains the exact MIT license notices for all 78 selected fixes and their first parents. Those 156 revisions use two distinct notice documents. These notices accompany the retained mutation snippets and source-reversion diffs.

The research tools and reproduction commands are documented in [the experiment README](../../README.md). Exact manifests, source audits, validation logs and helper versions are under `provenance/`. Reproducing test execution requires the declared upstream sources, pinned tools and explicitly prepared historical installations. Restoring the analytical evidence does not require those installations.

From the repository root, restore into a new directory and regenerate the original-profile report:

```sh
python3 research/test-value/results/2026-10-03/provenance/closeout-v1/restore_retained_evidence_v2.py \
  research/test-value/results/2026-10-03 /tmp/archguard-evidence-restored
python3 research/test-value/report.py \
  --root /tmp/archguard-evidence-restored/primary \
  --secondary-root /tmp/archguard-evidence-restored/secondary \
  --output /tmp/archguard-evidence-restored/report.html
```

The helper verifies archive digests, exact member inventories and each member's size and hash before writing regular relative files. It maps current evidence to `primary/current`, native evidence to `primary/native`, the original historical attempts to `primary/history` and `primary/history-continuation`, posthoc evidence to `secondary`, and the new baseline-only profile to `baseline-validation`. It rejects an existing destination. [The relocation inventory](../../../relocations.json) maps original analytical file paths to their retained loose copies. Relocation changes displayed evidence paths; measured report payloads match the original data.
