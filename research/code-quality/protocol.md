# Reviewer evidence experiments

These experiments ask whether structural similarity and surviving mutations help an implementer or reviewer identify a concrete mistake. Findings do not require a refactor or a new test. No experiment adds CI or a required score.

## Frozen inputs

| Input | Revision |
| --- | --- |
| Archguard starting point | `657a5aabb564d9f4f6784a70fa4fe3faef87a613` |
| T3 Code | `31a9da179ed0763335f05681c577474aec5d2309` |
| Uncle Bob's dryer | `6892667b3441b88379bc8d0439fc2152b0fdb341` |
| Uncle Bob's mutator | `bb9262a7b1d884a446bc14dc56568151dab02d71` |

The installations, selected files, hashes, and commands belong to each experiment. Both workstreams use the same T3 revision. An authored fixture, a copied-source reduction, and a full upstream run establish different claims. Retain the distinction in reports and reviewer feedback.

The reference Python projects are inspected and executed from external checkouts. Neither pinned checkout provides a license file or a license declaration in its project manifest. The user also supplied [author context](author-context.md) inviting agents to customize these tools or write alternatives. Their implementation source was not copied into this repository during these trials; reuse recommendations depend on measured behavior and adaptation cost. Copied T3 source retains its upstream MIT notice and records any adaptation.

## Structural similarity

The authored cases cover names and literals only, one extra guard or call, the same business concept expressed differently, unrelated behavior with similar control flow, intentional boilerplate, and a 30-80 line agent-style copy with changed names and branches.

Inspect exact pair membership and locations as well as scores. Compare set and multiset fingerprints, preserving property names, preserving call names, larger-subtree weights, and threshold changes. A token detector and a normalized-tree detector may discover different candidate units. Do not compare their aggregate counts as if those units were identical.

A reviewer should see both implementations and why they matched. Record whether the candidate suggests a reusable concept, is intentional parallel code, is unrelated boilerplate, or remains unclassified. Structural similarity does not establish equivalent behavior.

## Mutation feedback

The authored cases probe a boundary comparison, a boolean conjunction, arithmetic, a boolean constant, and zero-versus-one behavior. Run a clean baseline first, then intentionally weak tests and behavior-focused corrections. Retain negative controls that show the correction tests the contract.

Keep survived, killed by test failure, uncovered, timeout, invalid mutation, and execution error distinct. A failed baseline halts analysis. A harness failure cannot establish that tests killed a mutant. Verify source bytes and rerun the baseline after exceptional outcomes.

Report mutation location, operator, original text, replacement, execution status, and reproduction command. Classify every authored survivor and identify the extent of any real-source sample. Do not infer a useful-survivor percentage for a repository from the authored controls.

Compare operator inventories and generated sites before comparing runtime. Changed-file or function selection can bound cost, but the resulting report covers only that selection. Differential reuse must be tested against test-only edits, imported helper changes, configuration, and dependencies. Source hashes alone cannot validate a test result.

## Reproduction and cost

Use isolated locked installations. New output starts in ignored `research/local/` and enters retained evidence only with provenance. Retain exact tool versions, source/configuration hashes, commands, raw outcomes, exclusions, diagnostics, and elapsed times. Keep unfavorable comparisons.

Build `cargo build --release --locked --bins --examples` before measurement. Coordinate timing windows so the two workstreams do not measure concurrently. Separate builds, installs, validation, and recorded measurement runs. Report observed costs for the actual selected targets, without extrapolating a small fixture to the full application.

Run each prototype's meaningful failure, correction, and negative controls, then the repository's `scripts/check.sh` before PR creation. Research dependencies remain outside product installs. Independent review precedes child PR merges, and the final PR stays open for human review.
