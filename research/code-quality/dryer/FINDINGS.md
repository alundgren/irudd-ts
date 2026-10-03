# Findings from the pinned TypeScript comparison

Dryer can provide useful structural-copy evidence for a coding-agent reviewer, especially for the 45-line copied function. Its default score also missed an authored short copy with a guard and call, and reported a perfect match for unrelated behavior. Keep source review and parser completeness alongside every score. None of these results requires an abstraction or refactor.

The final corrected trial is [evidence/run3](evidence/run3/provenance.json), recorded on 2026-10-03 in a coordinated measurement window after the coordinator's release build and initial full repository validation passed. The pinned T3 revision is `31a9da179ed0763335f05681c577474aec5d2309`; the dryer revision is `6892667b3441b88379bc8d0439fc2152b0fdb341`. Runtimes were Python 3.12.14 and Node v24.21.0. Python packages were tree-sitter 0.26.0 and language-pack 1.20.0. Comparison packages were jscpd 4.3.0 and TypeScript 5.9.3.

## Authored controls

Scores below are rounded to six decimals. Raw JSON retains the full numeric values and exact locations. A pair is reported at 0.82 when its score is at least 0.82.

| Control and lines | Upstream set | Upstream multiset | Upstream weighted | Independent preserved set | Independent preserved multiset | Interpretation at upstream 0.82 |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| A, 1-7 and 9-15 | 1.000000 | 1.000000 | 1.000000 | 1.000000 | 1.000000 | Useful evidence of renames and numeric changes |
| B, 1-7 and 9-17 | 0.700000 | 0.707965 | 0.414865 | 0.681818 | 0.721311 | Useful authored copy missed after guard and call |
| C, 1-7 and 9-16 | 0.405063 | 0.492188 | 0.137791 | 0.296296 | 0.410959 | Negative control excluded |
| D, 1-5 and 7-11 | 1.000000 | 1.000000 | 1.000000 | 0.272727 | 0.407407 | Incidental syntax across unrelated behavior |
| E, 1-5 and 7-11 | 1.000000 | 1.000000 | 1.000000 | 1.000000 | 1.000000 | Incidental deliberate request boilerplate |
| F, 1-45 and 47-91 | 0.907143 | 0.954436 | 0.374600 | 0.737931 | 0.843373 | Useful evidence of a larger copied implementation |

[Upstream pairs](evidence/run3/authored-dryer-probe.stdout) and [independent pairs](evidence/run3/authored-independent.stdout) retain all 66 comparisons. Upstream reports nine pairs at 0.82. Four are the designated A, D, E, F pairs. Five others repeat the common baseline function across A, B, and C. Those five are incidental fixture reuse and should not count as five separate real findings. [interpretation.json](interpretation.json) classifies every upstream reported authored pair and the bounded real-source inspection.

The two [jscpd fragments](evidence/run3/authored-jscpd/jscpd-report.json) are exact baseline reuse across A/B/C. jscpd reports none of the designated A-F function pairs at four lines and fifty tokens in `mild` mode. Its fragments can extend across function boundaries, so function-pair membership differs from its token-fragment reports.

A uses changed local names and numeric values. It tests structural similarity, not identical output. B adds a negative-discount exception and an `audit` call, which matter to behavior even though most calculation code is copied. C uses reduction and a guard instead of the loop and `Math.max`. D reads debit/fee/payment versus capacity/reserve/consumed. E intentionally keeps two resource APIs separate. F changes locals, shorthand return keys, a condition, and numbers; the preserved-member score sees the output-key differences.

## Normalization and score consequences

Called names survive upstream normalization. For example, changing `map` to `filter` is visible. Ordinary property names disappear, which explains D's upstream score of 1. The independent preservation control lowers D to 0.272727 while A remains 1. Preserving names adds useful discrimination here, but does not identify types or prove that same-spelled methods do the same thing.

The multiset retains repetition counts. For F, upstream set Jaccard is 0.907143 and multiset Jaccard is 0.954436. In the independent representation, preserved-member set Jaccard misses F at 0.737931 while its multiset score reports F at 0.843373. This single fixture supports examining occurrence counts; it does not establish an optimal formula.

The experimental weight is the node count of each normalized subtree times its occurrence count. It lowers F to 0.374600 because the changed branch alters large ancestor subtrees. B also drops to 0.414865. Giving larger subtrees more weight can remove useful near-copy candidates after small edits. This weighting is a diagnostic experiment, not a recommended default.

| Upstream threshold | All authored reported pairs | Real T3 reported pairs |
| ---: | ---: | ---: |
| 0.70 | 13 | 4 |
| 0.75 | 9 | 2 |
| 0.80 | 9 | 0 |
| 0.82 | 9 | 0 |
| 0.85 | 9 | 0 |
| 0.90 | 9 | 0 |
| 0.95 | 8 | 0 |

B enters at 0.70, and F disappears at 0.95. D and E remain at every tested threshold. A higher threshold cannot remove those exact normalized matches. Full threshold counts for every formula are in the raw reports. There is no calibrated quality gate.

## Real T3 source

The frozen ten-file corpus contains 73,953 bytes. Upstream compares 82 functions and excludes 29 for minimum size. The independent detector compares 81 and excludes 30 because its syntax node counts differ. `safeDecodeURIComponent`, markdownLinks.ts lines 188-194, meets upstream's twenty-node limit and falls below the independent limit. All selected files parse without errors in both parsers.

`snoozeDateToPickerDate`, customSnoozeDate.ts lines 2-4, is excluded by the four-line minimum. The other two snooze functions are compared. The mobile/web `filePath.ts` modules and all selected larger functions remain part of the input. No pair reaches 0.82 in dryer or the independent detector; jscpd reports no fragment. This selected sample provides no evidence of a default-threshold real clone candidate. It says nothing about the unselected repository.

Lowering the upstream threshold to 0.70 produces four pairs, manually inspected as a bounded sensitivity sample:

| Source locations at pinned revision | Set score | Classification and reason |
| --- | ---: | --- |
| filePreview.ts 117-120 versus 123-129 | 0.750000 | Incidental, shared extension guard with different media lookup behavior |
| filePreview.ts 117-120 versus 132-140 | 0.750000 | Incidental, shared guard and fallback syntax; host preview deliberately combines classifiers |
| shell.ts 203-225 versus 408-433 | 0.704225 | Useful structural-similarity evidence for comparing PATH merge implementations; one preserves case/quotes while the other normalizes them |
| proposedPlan.ts 98-104 versus 106-109 | 0.702703 | Incidental, title construction and filename construction deliberately return different outputs |

Paths are `packages/shared/src/filePreview.ts`, `packages/shared/src/shell.ts`, and `apps/web/src/proposedPlan.ts`. All remaining real pairs are unclassified. The useful shell pair is below the default threshold and its behavior differs on Windows. A reviewer can compare that difference without being told to combine the functions.

## Failures and completeness

[Malformed-input evidence](evidence/run3/diagnostic-dryer-probe.stdout) records tree-sitter's `ERROR` node and `has_error=true`. The upstream extraction API returns no warning, and its direct CLI exits 0 with "No duplicate candidates found." jscpd also exits 0; it is a token detector, not a parser validation check. The wrapper and independent detector both mark the input incomplete and exit 2. A clean-looking clone report must not erase parser failure.

[The first trial](evidence/run1/provenance.json) is retained as an execution error. The runner initially resolved the venv Python symlink to its base interpreter, which bypassed the installed parser packages. It stopped immediately with `PackageNotFoundError`, before analyzing controls. The correction preserves the venv entry path. The successful trial used that correction and does not treat the failed trial as a clean or duplicate result.

The independent seven-test Node suite and two-test Python suite cover a property-erasure failure and member-preserving correction, unchanged-member and local-rename controls, called-member differences, nested callback exclusion, malformed and missing input, corrected syntax, repetition-count differences, unary operator preservation, timeout partial-output retention, and final checkout checks after failure. Neither parser checks compiler diagnostics or runtime behavior.

The earlier successful run2 is preserved as a superseded comparison. Independent review found that TypeScript child traversal omits prefix/postfix unary operators: `+x` and `-x` initially compared exactly, as did `x++` and `x--`. The detector now records the operator explicitly. A regression demonstrates the failure against [the preserved run2 implementation](evidence/run2/measured-implementation/detector.mjs), its correction, and renamed-local negative controls. Independent extraction also stopped selecting object-property arrows, matching upstream's candidate units. Run2 compared 81 real functions and excluded 35, including five extra tiny object arrows; run3 compares 81 and excludes 30. None of those five was an accepted candidate.

Review also corrected timeout handling. Partial stdout/stderr and timeout status now survive, and post-run revision/cleanliness checks are recorded even after errors. The primary execution error remains distinct from a checkout verification error. These fixes were tested and the whole suite was rerun in a second reserved window. Run2's measured runner, wrapper, and detector are retained beside its original raw evidence. Run3's manifest records both external checkouts clean after the final run.

## Provenance and practical use

Each [retained file hash](evidence/run3/retained-files.json) preserves the raw run bytes. [The run manifest](evidence/run3/provenance.json) records commands, revisions, runtime/executable hashes, downloaded grammar hashes, upstream source hashes, and one elapsed observation per invocation. Its `inputs` also contains run-time snapshots of README, tests, and dependency notices. Those are ancillary snapshots; the comparison consumes the detector/wrapper/runner, package pins, constraints, corpus manifest, and source fixtures. The run3 measured executable/configuration/fixture bytes match the retained implementation. Later prose edits do not alter the measured input. This trial is a functional comparison on a shared host, not a speed comparison.

The successful observed invocation costs range from 0.117049 to 1.674143 seconds. The wrapper computes and serializes all pair scores; the upstream CLI emits only thresholded candidates; jscpd detects token fragments. Their workloads differ. Raw values are retained without claiming one is faster.

For reviewer handoff, include both source ranges, exact score/formula, threshold membership, parser status, exclusions, and manual classification. Suggested wording is "These functions have structural similarity under this normalization; compare their behavior and decide whether the repetition is intentional." The author's post supplied for this task invites customization or alternative implementations. The missing formal license declaration remains metadata, rather than the reason for this research recommendation. Parser-error handling, missed short copies, incidental exact matches, and the need for a broader representative evaluation determine the technical next steps.
