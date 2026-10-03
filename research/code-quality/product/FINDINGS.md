# Product dogfooding findings

Archguard now produces structural duplicate candidates and surviving mutation evidence through its Rust APIs and CLI. The implementation uses the existing pinned Oxc parser and binding analysis. The TypeScript SDK uses Node builtins; callers supply their own trusted test runner. No additional product library, automatic installation, CI integration or score gate was introduced.

The [initial research](../findings.md) compares the pinned reference tools with jscpd and Stryker. Those archived experiments have different implementations and operator inventories. The results below come from the product, using the frozen T3 Code revision `31a9da179ed0763335f05681c577474aec5d2309` and explicit source/test selections. They do not establish whole-repository coverage.

## What structural comparison tells a reviewer

The default comparison preserves property names, called operations and local binding relationships. It normalizes literal values while retaining their kinds. The filter uses subtree-set Jaccard; reports also expose counted and size-weighted comparisons. These measures answer different questions and are deliberately visible together.

| Authored case | Default set score | Interpretation |
| --- | ---: | --- |
| A, changed names and numeric values | 1.000 | A useful copied-operation candidate |
| B, added guard and call | 0.723 | The default 0.82 threshold misses the modified short copy |
| C, same calculation expressed differently | 0.290 | Shared purpose alone produces little similarity |
| D, unrelated account and battery behavior | 0.292 | Preserved properties distinguish the operations |
| E, intentional request boilerplate | 0.879 | Similarity does not justify an abstraction |
| F, a 45-line agent-style copy | 0.774 | Preserved properties miss this changed-domain copy at 0.82 |

Erasing properties raises F to 0.917 and D to 0.742. A reviewer gets a more useful copy candidate together with more incidental similarity. Keeping literal values lowers the name/literal-only A pair to 0.674. The [replay helper](README.md) retains all comparisons at threshold zero so these misses remain visible.

The set/multiset difference matters. F's counted score is 0.829 while its weighted score is 0.291. Large subtree weights amplify differences in ancestors after a small edit. Counting repetition can recover a candidate, but these six examples do not establish a universally better filter. The value 0.82 is a starting point, not a calibrated boundary between good and bad code.

The authored adoption profile also contains two 35-line invoice/order implementations. The final product identifies an exact normalized match after a correction that treats equivalent shorthand and expanded properties together. A real static `__proto__` property remains distinct because it changes JavaScript behavior. The earlier miss and the correction controls are retained.

The focused four-module T3 sample has no duplicate pair at the default threshold. Three native TSX editor variants produce an exact `basename` pair. Explicitly erasing local binding relationships also finds two platform `ComposerEditor` implementations at set 0.827, counted 0.885 and weighted 0.611. That duplication is intentional. A new local declaration can change declaration ordinals and reduce the default score; selecting erased locals demonstrates this tradeoff instead of silently changing the default.

The SDK/example self profile finds the two authored pairs and no SDK pair. This is useful as a bounded inspection result, not evidence that the repository contains no duplication.

## Mutation feedback and test corrections

The authored domain has eleven runtime mutation sites. Its six ordinary weak tests kill four mutants and leave seven survivors. Six expose missing behavior assertions: the shipping threshold, domestic eligibility, nonzero tax, an approval flag and two empty-list paths. Eighteen behavior tests kill those six, leaving one equivalent mutation. `Math.abs(amount) + 0` and subtraction of zero are equivalent for the fixture's finite input contract, including signed-zero controls.

An earlier version used raw `amount + 0` and incorrectly called subtraction of zero equivalent. Negative zero disproved that claim. Independent review caught it, and the corrected fixture now tests signed zero, negative input and nonfinite rejection. Retaining this mistake matters more than reporting a flattering aggregate score.

The unchanged T3 path module and its six tests produce 21 sites, 15 killed and six surviving mutations. Inspection of the exported functions and their callers separates them:

| Survivor behavior | Classification | Review consequence |
| --- | --- | --- |
| `../repo` and `.\\repo` lose explicit-relative recognition | Missing test | Callers use this decision to join the input to the current directory |
| `x/` keeps its final separator | Missing test | Existing normalization promises trailing-separator removal |
| One-slash roots fall through a different guard | Equivalent | Later normalization restores the same public result |
| Empty/one-character paths take a different branch | Equivalent | Remaining guards restore the same public result |
| Two edits change `C:/` to `C:\\` | Missing specification | Inspected callers/tests do not require one dispatch representation |

The [counterexample helper](README.md) checks exact source hashes, applies the observed edit to an owned copy and retains T3 licensing. Its own controls verify moved source lines, unchanged ordinary relative paths and rejection of unknown owners or unsafe mutation IDs before output creation.

Two authored supplemental tests in private worker copies cover the relative-path and trailing-separator behavior, with ordinary relative paths and preserved roots as negative controls. The replay kills exactly those two mutations, producing 17 killed and four surviving sites with eight passing baseline tests. The original frozen T3 source and tests remain unchanged. The two equivalent mutations and two specification questions remain for review.

The delimited-preview profile executed all 52 sites with nine passing baseline tests. It produced 33 assertion kills, seven survivors, ten runtime errors and two command timeouts. All seven survivors expose useful behavior cases: independent column and cell truncation, escaped quotes at the cell limit, bare carriage returns, blank records and an exact 100-row result with a final line ending. The supplemental profile covers these cases with complete-content controls.

The ten runtime errors came from Vitest's five-second test timeout. Source inspection shows that these mutations still terminate; even a repeated carriage return returns after accumulating 100 rows. The original test constructs roughly six million characters while testing several limits together. A slow bounded mutation can lose its useful assertion failure behind the framework timeout. The authored profile now gives each test 15 seconds within a 30-second command deadline. The original five-second outcomes remain evidence. The two command-timeout edits can make the outer index move backwards or leave the escaped-quote loop without progress, so they have a different explanation. None counts as a kill.

Dogfooding also corrected the product itself. Earlier CSV and SDK runs stopped when cleanup could not be confirmed. An actual Linux process-descriptor control reproduced an `ESRCH` read after a process disappeared; the collector now distinguishes that disappearance from unknown process state. The historical failures did not retain enough diagnostics to prove that specific cause for each run. The corrected CSV replay exercised every site, preserved source bytes and removed its active journal without another cleanup failure.

The SDK run also exposed discarded command diagnostics and a capped problem list that could hide a terminal cleanup location. The product now retains bounded validated failure details and error-only process excerpts, reports omitted problem counts, and prioritizes terminal recovery information. It redacts configured arguments and environment values before clipping. This can redact ordinary words such as `test` and `run`; protecting arbitrary supplied values costs some diagnostic readability. Separate controls cover escaped values, malformed UTF-8, control characters, quiet runtime failures and noisy successful commands.

## Execution and reuse choices

The product runs a fresh baseline before mutation results or reuse. Each worker gets a fresh byte copy of the declared workspace and dependency inputs. It does not write through links to the original repository or installation. The optional state records reuse complete killed/survived results only when the full declared input identity still agrees. Changes to tests, helpers, configuration, executables, dependencies, metadata or source membership invalidate reuse. This deliberately reruns more work than function-only snapshots.

Only failures attributed to executed assertions count as kills. Runtime/import/hook failures, missing or invalid result metadata, timeouts and cancellation remain distinct outcomes. This prevents a broken test runner from improving the apparent test quality. It can also leave a behavior-changing mutation classified as an execution error when that mutation throws an ordinary error inside a test. Review those errors alongside survivors.

Changed-file selection bounds mutation planning. The current product reports function ownership but does not select changed functions independently, skip sites by test coverage or map mutations to individual tests. Fresh test processes and full private dependency copies make large Vitest profiles expensive. A smaller explicitly declared installation can help, but writable links or undeclared inputs would weaken isolation and reuse validity. Stryker remains an established alternative when per-test mutation coverage is worth its separate integration and dependencies.

## Practical limits

These commands produce reviewer evidence. Preserved call names can miss a copied operation after a rename; same-spelled methods can belong to unrelated APIs. Property erasure can hide domain differences. Equivalent mutants and underspecified behavior remain after good tests. None of those cases calls for automatic refactoring, extra implementation-detail assertions or a perfect score.

The parser uses bounded workers and conservative punctuation/depth/node limits. Some compiler-valid files with large strings or flat expressions can be rejected. The report remains incomplete rather than silently pretending those files were analyzed. Oxc syntax/binding facts do not supply compiler types.

The execution support targets Linux and macOS. Linux runtime tests and a macOS library compile are separate evidence; this environment does not establish native macOS process/resource behavior. Explicit commands are trusted and private workspaces are not a hostile-code sandbox. Cleanup has its own bounded traversal deadline, while kernel filesystem calls can still block. Uncertain cleanup preserves the owned workspace for inspection instead of claiming successful removal.

Start with a small selected change and hand its pair locations, surviving edits and incomplete outcomes to the reviewer alongside the specification. The most useful next comparison is whether that reviewer finds a concrete mistake they otherwise miss, and how much time it takes to dismiss incidental evidence.
