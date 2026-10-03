# Validation and independent review

The required release build completed before recorded measurements. `scripts/check.sh` passed on the starting checkout, in each implementation worktree, and on the combined integration checkout. It checks formatting, Clippy, generic and provider tests, historical controls, existing dependency inventories, maintained documentation links, preserved archives, and Archguard's own configuration. The combined configured scan reported 50 selected files, 316 imports, zero violations, and zero analysis problems.

The combined checkout also passed seven Node detector controls, two Python runner controls, seven Python mutation-observer/excerpt controls, the retained mutation-outcome verifier, and all 69 retained mutation checksum entries. The checksum correction was independently verified in a clean committed export. Neither existing archived research artifacts nor product dependencies changed.

Independent agent review preceded both child merges:

| Child PR | Reviewed head | Result |
| --- | --- | --- |
| [Dryer #13](https://github.com/alundgren/irudd-ts/pull/13) | `bc33f1fb278fd4ad6510e555b306443fdfd891c1` | Approved and merged into the integration branch |
| [Mutator #14](https://github.com/alundgren/irudd-ts/pull/14) | `1ecede0cecbea1f5fc97bf85e2a486cdc0411034` | Approved and merged into the integration branch |

Dryer review corrected unary-operator loss, timeout-output retention, exceptional checkout verification, candidate-count wording, and a quoted CLI message. Mutator review corrected one-based UTF-16 source excerpts, mixed assertion/execution-failure classification, unhandled-error observation, and a manifest that indexed an ignored Python cache. Original trials and measured implementations remain preserved. The mutation reviewer independently reran six actual Vitest controls and verified the recorded observer corrections.

The standalone HTML report was checked in cached Chrome 153 on desktop and mobile viewports. Default, changed-threshold, and weighted candidate memberships matched the retained values; the page had no runtime exceptions or horizontal page overflow. The report uses embedded data and styles.

These checks validate the research prototypes and their handoff. The experiments do not establish whole-T3 coverage, calibrated clone thresholds, population survivor usefulness, or comparative performance. The combined PR targets `main` and is left open for human review.
