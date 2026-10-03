# Independent T3 classification preparation

Read-only review of frozen T3 `31a9da179ed0763335f05681c577474aec5d2309`. No builds, runner replays or timing measurements. Plans are root `research/local/code-quality-product/t3-{delimitedPreview,hostClassification}-plan-r8.json`; both source hashes match the frozen files.

- delimitedPreview: 52 sites, SHA256 `3871bc65931237038e63a94df7c72341b9d81f95a79108ac45c771533352c0fe`.
- hostClassification: 238 sites, SHA256 `07576a1ed02ccbd1b3b84d82bbb95d663959edaaf87f470324a5d361fd89e66a`.

## CSV: all seven R10 survivors are useful contract controls

R10 dispatched all 52 sites: 33 killed, seven survived, ten runtime errors reporting Vitest's 5000ms test timeout, two command timeouts. The report remains incomplete; no omitted/not-run results or cleanup problems. IDs below use unique 12-character prefixes; the saved report contains full identities.

| ID / source line | Difference and useful control |
| --- | --- |
| a0696b7e65a9 / 22 | 31 short cells retain only 30 but falsely report no truncation. Compare 30 versus 31 cells independently of row/cell limits. |
| 8b3509509647 / 29 | An escaped quote at cell length 2000 appends a 2001st character. Use a quoted 2000-character cell followed by an escaped quote and closing quote; the original keeps 2000 characters and reports truncation. The 1999-character case is an exact-limit negative control. |
| ed54d1ece1ae / 30 | Same escaped-quote overflow loses the truncation flag while keeping the cell limit. |
| a9c3df139ab2 / 44 | Plain CR separators skip the next character: `a\rb` loses the second row's b. Consecutive LF separators also lose an empty record. CRLF is an unchanged negative control. |
| 7017f47cd6a5 / 46 | Exactly 100 rows ending in LF or CRLF falsely report truncation. Check complete 100-row text with and without a final newline, then 101 rows as the actual-overflow control. |
| 021f5b180cca / 46 | Same false truncation at exactly 100 newline-terminated rows, because the comparison now subtracts zero. |
| 0657a6743bce / 49 | A single ordinary 2001-character cell keeps 2000 characters but loses the truncation flag. Compare 2000 and 2001 characters independently of column/row overflow. |

No survivor requires an equivalence or specification-question label. Web `DelimitedTablePreview.tsx` uses truncated to show the partial-content notice. Both web attachment/workspace previews and mobile attachment loading call the shared parser. The original combined 101-row × 31-column × 2001-character test allows one truncation cause to hide another.

### CSV timeout distinctions

The two command-timeout edits have genuine nontermination paths: outer loop increment changed to decrement (line 25), and escaped-quote increment changed to decrement (line 31, cancelling the outer increment).

None of the ten runtime-timeout edits is unbounded in source. Lines 19, 25 `<`→`<=`, all four line-28 lookahead/predicate edits, line 44 equality reversal, and both line-46 flag edits keep the outer index advancing. Line 44 increment→decrement repeats one CR, but adds a row each time and returns at 100 rows. The original large combined-limit input is about 6MiB. Preserve runtime outcomes; they do not prove equivalence, assertion detection or a parser hang. A separately recorded Vitest timeout override is an operational control, not a changed assertion or score improvement.

## Hosts: scope and source-proven equivalence

The selected shared test file calls only isPublicFaviconHost. Browser navigation also calls exported isPrivateNetworkHost and isLocalLoopbackHost to decide private reachability and localhost substitution. `favicon.ts` parses a URL and checks its hostname before disclosing the host to Google.

Crucially, existing `apps/web/src/browser/browserTargetResolver.test.ts:220–348` already covers direct private-host boundaries and broad favicon special-purpose/NAT64/IPv6 exception cases. Existing `packages/shared/src/favicon.test.ts` covers URL-normalized privacy controls. They are outside this narrow profile. Do not call a surviving shared-profile mutation a missing T3 test until checking those tests.

There are 45 provably equivalent prefix-array zero→one sites, with the matcher unchanged: line 128 indices 6–7; line 134 indices 1–7; line 139 indices 2–7; line 140 indices 3–7; lines 141–142 indices 2–7; line 145 indices 2–7; line 146 indices 1–7. Suffix entries are never compared. Line 134 index 1 changes only a bit removed by the /23 mask. No extra tests are useful for these edits.

The line-51 remainingBits literal 0→1 is also equivalent for current private callers. All prefix lengths have remainder 0, 7 or 12. Remainder zero computes a zero mask if it misses the early return, preserving the true comparison; remainder one never occurs.

### Practical host controls and cautions

- Test the exported local/private APIs directly. Split-limit/index changes at line 100 can break fc00/fe80 private reachability while favicon classification still rejects those hosts through its later IPv6 rule. Local-loopback return changes can likewise be hidden by private/special-purpose checks.
- Reuse existing web-test address tables before inventing policy. They cover 198.18/19 boundaries, special IPv4 blocks and adjacent public controls, mapped forms, NAT64 embedded public/private IPv4, 2001 exceptions, documentation prefixes and repeated trailing dots.
- Add compressed IPv6 with exactly one omitted hextet and its uncompressed equivalent if a parser-boundary survivor needs a valid-host counterexample. Mapped low/high hextet edits need differing hextets; `::ffff:808:808` cannot detect their interchange.
- Treat malformed raw-host-only differences separately from valid URL-host behavior. The shared tests intentionally expect malformed IPv4 strings `10.0.0.999` and `10.0.0` to remain public; do not replace that policy based on the misleading test name. Some malformed IPv6 guard changes may be specification questions for raw-string exports even when URL-based callers reject those forms before classification.
- Do not assume IPv4-mapped loopback must make isLocalLoopbackHost true; its current contract explicitly recognizes localhost, ::1 and plain 127/8, while the broader private API handles mapped addresses.

Host full-run survivor/error classification is pending. These notes establish source/caller expectations and a set of exact equivalent candidates; they make no runtime or whole-repository coverage claim.

## Optional existing-boundary selection profile

Owned fixture: `/tmp/archguard-quality-product-host-boundary-selection-review`. `provenance.json` records original source/test/license hashes and assertion-block lines. `controls/host-boundaries.test.ts` preserves both existing assertion bodies, adapting only imports; `hostClassification-existing-boundaries.json` adds them to the original shared profile, limits workers to one and mutations to 238, and disables state reuse. No runtime was executed. `equivalent-mutations.json` records all 46 source-proven equivalent sites with full stable IDs.
