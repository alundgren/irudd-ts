# Findings

Both concepts look useful for bounded coding-agent review experiments. Structural matching caught an authored 45-line copy that the pinned token detector missed. Mutation testing exposed five deliberately missing assertions and one unconstrained behavior in selected T3 code. The evidence also showed why automatic refactoring and mutation-score targets would be poor defaults.

These are research prototypes, with isolated dependencies and no production integration. [The protocol](protocol.md) records exact upstream revisions. [The author context](author-context.md) supplied during the work invites agents to customize the reference tools or write alternatives. Technical behavior and adaptation cost determine the recommendations below.

## Structural duplicate detection

| Authored case | Reference set score | What a reviewer learns |
| --- | ---: | --- |
| Names and numeric values changed | 1.000 | Useful evidence of a copied operation |
| Copy with a guard and an extra call | 0.700 | The default 0.82 misses this short copy |
| Same calculation expressed differently | 0.405 | Shared purpose does not imply structural similarity |
| Unrelated account/battery behavior | 1.000 | Property erasure can hide the domain distinction |
| Deliberate request boilerplate | 1.000 | An exact match can remain intentional |
| A 45-line agent-style copy | 0.907 | Useful candidate that needs source review |

The reference's set Jaccard over subtrees is a workable candidate measure. Counting occurrences raised the large-copy score to 0.954. Weighting subtrees by size and occurrence lowered it to 0.375 because a small edit changes large ancestor subtrees. Those controls support experimenting with repetition; they do not establish a better formula. Raising the threshold cannot remove the two incidental exact matches.

In the independent TypeScript detector, preserving property names lowered the unrelated pair to 0.273. It also lowered the large copy's set score to 0.738, with the counted score recovering to 0.843. Keeping call names helps distinguish operations, but same-spelled calls do not prove equal behavior. A reviewer benefits from seeing both normalization choices.

The ten-file T3 sample had no pair at 0.82. At 0.70, four reference candidates included a potentially useful comparison between two PATH merge implementations and three incidental pairs. jscpd 4.3.0 found no real-source fragment and missed all six designated authored pairs; its two authored reports came from baseline fragments repeated across fixtures. Whole-function tree matching and token fragments answer different questions.

The reference CLI also exited 0 with no candidates on malformed TypeScript. The independent prototype retains parser diagnostics and reports incomplete analysis. Keep this completion check if adapting the reference. The small TypeScript implementation is promising for further experiments, while jscpd remains useful for unchanged text fragments. The selected sample cannot establish repository-wide usefulness or frequency.

## Mutation testing

The authored weak suite passed its baseline. The reference produced eight sites: six survived, one was killed, and one was uncovered. Five survivors exposed the intended weaknesses, and one was equivalent for the fixture's integer-balance contract. Corrected behavior tests killed those five useful survivors while leaving the equivalent mutation alone. This constructed 5-of-6 result is a control outcome, not a useful-survivor estimate for real repositories.

Stryker 10.0.0 with Vitest 4.1.11 generated eighteen mutants for the same authored source. Four useful survivors and one equivalent survivor remained before correction; only the equivalent survivor remained afterward. Its configured operators did not generate the reference's `0` to `1` fee mutation. Retain operator/site inventories instead of comparing aggregate scores.

The selected T3 reduction exposed a final `0` to `1` millisecond mutation in `applySnoozePickerTime` that the existing tests did not distinguish. A demonstration assertion kills it. A reviewer should first confirm whether minute precision is part of the intended contract; the survivor alone does not establish a production defect. Stryker killed its five generated mutations on that reduction and did not generate this numeric mutation.

The reference runner reports test-command errors and timeouts as killed. Its differential trials reused a previously killed site after test-only, configuration, imported-helper, dependency-behavior, and failing-baseline changes, with no test-command call or baseline in those differential runs. Forced runs revealed surviving mutations or the broken baseline. Direct adoption needs corrections to execution status and invalidation.

Stryker correctly re-evaluated the test-only change in our control. It reused the killed arithmetic mutation after an imported-helper edit until `--force` exposed a survivor. This agrees with its documented limitation: incremental reuse tracks mutated/test files and misses other files and environment changes. Use conservative invalidation or forced bounded reruns when relevant inputs change. [Stryker incremental documentation](https://stryker-mutator.io/docs/stryker-js/incremental/).

Stryker is the stronger TypeScript execution backend for further experiments because it integrates Vitest and per-test mutation coverage. The reference's small operator set still revealed a numeric case Stryker did not generate. Boundary and boolean mutations gave concrete feedback here; arithmetic produced both a missing test and an equivalent mutation. This trial does not establish an operator ranking for other code.

## Cost and next use

The recorded duplicate-tool invocations each took about 0.12-1.68 seconds. Selected mutation runs took about 4.6-16.0 seconds, depending on source, tests, tool, and operator inventory. These are individual observed wall times on a shared host; a brief local syntax check and text scan also occurred during the final mutation window. They do not establish a speed ranking or the cost of mutating the full T3 application. Raw commands, times, hashes, and unfavorable outcomes are retained in each experiment.

Use changed-file or function selection to bound review cost, then show candidate pairs and survivors directly to the reviewer. Preserve incomplete/error/timeout states. The mutation prototype exports [classified survivor records](mutator/evidence/review-input.json), and the clone prototype retains pair locations, formula, membership, exclusions, and [manual interpretations](dryer/interpretation.json). Both can be consumed without making a global score the objective.

The next useful experiment is an agent review of the same change with and without this evidence. Record concrete missed behavior, accidental copies, misleading suggestions, and the cost of resolving them. Expand the real-code sample before adopting either tool broadly. Detailed outcomes, reproduction commands, independent-review corrections, and limitations are in the [dryer findings](dryer/FINDINGS.md) and [mutator findings](mutator/findings.md).
