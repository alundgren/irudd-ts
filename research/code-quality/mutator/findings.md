# Findings from the mutation research prototype

Mutation testing supplied useful test-review prompts in this bounded trial. All five authored weaknesses survived the upstream run, and behavior assertions killed them after correction. The same process raised a precision-contract question in T3's selected calendar test. It is a missing test or specification if minute precision is intended. It also left an equivalent arithmetic mutant alive, which is a reason to classify survivors instead of chasing a perfect score.

The external upstream implementation needs adaptation before its raw statuses can supply dependable review input. It reports a timeout, broken worker command, and deliberately invalid replacement as killed. Its differential cache can also preserve a killed result after the tests or imported behavior changes, and can skip the baseline entirely. The prototype keeps those upstream claims beside command outcomes rather than correcting history silently.

## Targets and results

Targets and reasons were frozen in `targets.json` before trials. Source revisions, per-file hashes, commands, test outputs, operator inventories and durations are retained under `evidence/`. `review-input.json` classifies every authored survivor from both phases and every survivor in the selected real file. This is the full survivor sample for one small T3 module, not a sample of all T3 tests.

| Source and tests | Upstream outcomes | Stryker outcomes |
| --- | --- | --- |
| Authored weak behavior tests | 1 killed, 6 survived, 1 uncovered | 11 killed, 5 survived, 2 no coverage |
| Authored corrected behavior tests | 6 killed, 1 survived, 1 uncovered | 15 killed, 1 survived, 2 no coverage |
| T3 source and existing adapted tests | 1 killed, 1 survived | 5 killed |
| T3 source with milliseconds correction | 2 killed | 5 killed |

The authored survivors at lines 4, 8, 12, 16 and 20 were missing test assertions. They respectively changed the shipping minimum boundary, payment-and-verification conjunction, credit addition, initial eligibility and zero fee. Corrections test below/at/above the shipping minimum, both mixed boolean combinations, positive and zero credit addition, the exact initial boolean, and the exact fee. Those are behavior cases a reviewer can understand without asserting how the functions are implemented.

The survivor at line 24 changes `balance + 0` to `balance - 0`. It is equivalent under the specified finite integer credit contract, which treats signed zero as the same business value. JavaScript can distinguish signed zero through `Object.is`; that distinction is outside this business contract. Adding assertions solely to kill this mutant would not improve order behavior. The uncalled archived-plan function remains uncovered and outside the selected workflow. Neither outcome justifies inventing a global target score.

The T3 reproduction copied `apps/mobile/src/features/threads/customSnoozeDate.ts` at `31a9da179ed0763335f05681c577474aec5d2309` without changing source bytes. Its existing test import changed from `vite-plus/test` to `vitest`; original and adapted test hashes are recorded. The surviving replacement changes the last `0` in `next.setHours(selected.getHours(), selected.getMinutes(), 0, 0)` to `1`. The existing test checks seconds, but omits milliseconds. A demonstration test checks that both reset to zero and that the original date retains its seconds and milliseconds. The survivor then fails an assertion. The source reset alone does not establish that milliseconds are an observable product requirement. A reviewer must confirm the precision contract before requesting this assertion; the demonstration is not a mandatory recommendation or a proven production defect. This is a reduced standalone reproduction with nine baseline tests, not execution of the mobile application or proof of full repository coverage.

## Operator coverage and execution reliability

Upstream generated eight authored sites across comparison, logical, arithmetic, boolean and zero/one constants. Stryker generated eighteen authored mutants across empty blocks, boolean literals, conditional expressions, equality/comparison, logical operators and arithmetic. Four requested weaknesses had matching replacements in both tools. The zero-to-one fee mutation existed only upstream. Stryker's real-file inventory comprised three block removals and two method replacements; upstream's comprised two zero-to-one constants. Stryker found no survivor in the existing T3 tests because it did not generate the missing milliseconds mutation. These different inventories prevent an aggregate score comparison.

The reliability controls preserve the upstream status and independently observed outcome. Independent review found that the initial prototype observer wrongly accepted mixed assertion and execution failures. The original evaluator and trial remain under `evidence/initial/`. Actual passing, assertion-only, assertion-plus-import, runtime, unhandled-rejection and nested-teardown controls record the failure and correction in `classifier-controls.json` and `classifier-correction.json`. The full comparison was rerun with the corrected observer and explicit reporter:

| Control | Upstream report | Recorded command observation |
| --- | --- | --- |
| Corrected assertion rejects true-to-false default | killed | Vitest assertion failure |
| Worker command exits 12 while root baseline passes | killed | execution error |
| Worker command sleeps beyond two-second limit | killed | timeout, exit 124 |
| Artificial replacement inserts an unmatched opening parenthesis | killed | invalid mutant, Vite transform error |
| Initially failing test or config baseline with forced execution | baseline failed | no mutant workers run |
| Runner raises an exception in a worker | exception | worker directories removed, original source restored |

The invalid control supplies an artificial replacement to the upstream executor, using the existing boolean site's ID. It is not a mutation that upstream generated. `site.mutant` records the actual invalid text. The raw failure is classified as an execution error by the generic command observer and as an invalid mutant by the deliberate control metadata.

Every recorded ordinary run checked source bytes before and after. Timeout, worker failure, invalid replacement and an injected runner exception also left source bytes unchanged. The passing Vitest baseline ran again after those exceptional controls. The corrected observer requires recognized `AssertionError:` prefixes and a separate execution-reporter record with no suite, hook or unhandled errors before accepting a kill. Mixed assertion-and-import, runtime, unhandled and teardown failures remain execution errors. Vitest's JSON reporter alone omits unhandled errors, so it cannot supply that assurance. It does not infer compile validity from passing tests or claim that upstream exposes these distinctions itself.

## Differential and changed-scope reliability

Upstream's snapshots retain killed outcomes using only the owning function's digest. `engine._select` and `_carry_forward` do not include test files, config, imported helpers or dependencies. `_apply_selected` returns before checking a baseline when no sites are selected. The controls exercise this behavior with a single previously killed site:

| Change | Differential result | Forced result |
| --- | --- | --- |
| Replace exact eligibility assertion with typeof assertion | reused killed, no commands | survived |
| Break Vitest config | reused killed, no commands | baseline fails |
| Add a failing baseline assertion | reused killed, no commands | baseline fails |
| Change imported rounding helper to return 15 | reused killed, no commands | addition mutant survives |
| Change explicit local dependency implementation to return 15 | reused killed, no commands | addition mutant survives |
| Rewrite the owning eligibility function and its assertion | function selected again | baseline and mutant run |

The helper and dependency controls leave the test bytes unchanged between seed and edited runs. Their changed behavior makes addition/subtraction observationally equal for that selected assertion. The upstream cache does not detect this change. Dependency source bytes sit outside the `src` hashes reported by the upstream wrapper; `retention.json` supplies their separately recorded hashes and explains that the seed hash derives from the exact setup literal.

Git changed-file selection also needs explicit scope. A test-only edit selects no production files. An imported-helper edit selects `src/helper.ts`, but does not expand to the importing account functions. That is file selection, not dependency-aware impact analysis. A digest of a changed function does correctly cause that function's mutations to run again in the recorded control.

Stryker 10 with its Vitest runner performed the dry run for the incremental controls. Replacing the eligibility test reran both scoped mutants, and the old killed boolean became survived. Changing only the imported helper reused both killed outcomes; `--force` reran them and changed the arithmetic mutant to survived. This matches Stryker's [documented incremental limitations](https://stryker-mutator.io/docs/stryker-js/incremental/), which exclude files outside mutated/test files, config, dependencies and environment changes. It is a documented scope limit rather than an unexpected test-file invalidation failure. Use forced reruns or a conservative external digest covering source, tests, imports, configuration, lockfiles and environment before trusting an incremental review result. This prototype demonstrates the issue; it does not implement a production cache.

## Runtime and review use

Each measured comparison is one observational wall-clock sample, with one mutation worker, after installations, parser preparation, the required release build and a successful repository check. The coordinator reserved a measurement window across agents. Other host processes were not controlled. A brief local Python syntax check and text scan also occurred during the final window, so these observations should not be treated as controlled benchmarks.

| Run | Upstream seconds | Stryker seconds |
| --- | --- | --- |
| Authored weak tests | 12.582 | 11.977 |
| Authored corrected tests | 16.023 | 14.896 |
| T3 existing adapted tests | 4.974 | 7.412 |
| T3 with correction | 4.588 | 7.631 |

These times are not a speed ranking. The tools execute different mutants. Upstream reruns the full Vitest command for each selected site; Stryker uses mutation coverage and selected tests. Baseline, coverage generation and cache probes have separate raw durations. Reused outcomes do not by themselves show trustworthy or faster analysis. Stryker's helper-control seed, differential and forced observations were 7.129, 6.308 and 6.585 seconds for two scoped mutants; one sample does not establish a general gain.

For a review agent, prefer the retained per-site JSON with the original text, replacement, file, location, operator, status, classification and reproduction command. Ask the agent to name a changed behavior and a missing input case, and rerun that case after proposing an assertion. Equivalent survivors should stay classified as equivalent. The selected T3 milliseconds case is a useful specification question. If minute precision is intended, it identifies a missing assertion. If subsecond precision has no observable contract, a reviewer may instead classify it as an irrelevant implementation detail. This one-file reproduction cannot estimate the cost or signal rate of mutation testing across T3. No survivor here was classified as noisy/misleading or useful-but-expensive; those categories remain available for larger evaluations. The artificial execution-error controls are reliability failures, not real-source test suggestions.

The npm and Python installations are isolated research dependencies. `license-inventory.json` records their locked licenses. T3's MIT notice is retained. Mutator and its crapper dependency supplied no formal license file or field at the pinned revisions. The user supplied the author's invitation to customize these tools or write alternatives. External execution preserves the exact evaluated code; missing formal metadata is not a blocker for this research. The final npm audit records two moderate transitive advisories, with no high or critical advisory. No UI server was started.

For the next TypeScript research iteration, I would use Stryker's Vitest integration with forced scoped reruns and explicit operator inventories, then evaluate adding the useful zero/one constant replacements separately. This recommendation follows its working test-file invalidation and richer runner integration, not an aggregate score or speed claim. Adapting upstream mutator remains reasonable under the author's invitation, but its runner needs distinct timeout/invalid/execution-error outcomes, unconditional baseline validation and conservative cache inputs before those results can be trusted. Those changes are more work than this small orchestration prototype, so they remain proposed adaptations rather than claimed fixes.
