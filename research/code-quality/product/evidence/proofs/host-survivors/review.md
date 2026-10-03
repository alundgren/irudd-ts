# Host classification survivor review

The original run planned and executed 238 mutations. Its ten selected shared tests passed, 85 mutations were killed and 153 survived. This review preserves those outcomes. Direct probes are diagnostic evidence, not new mutation-runner outcomes.

| Classification | Count | Evidence |
| --- | ---: | --- |
| Useful test selection gap | 92 | 88 differ against assertions from two existing omitted browser tests. Four more differ through adapted original resolver code using two other existing test cases. |
| Useful direct public-classifier test gap | 13 | Nine valid IPv6 cases and four malformed cases produce different exported `isPublicFaviconHost` results. Eight still differ after URL canonicalization. |
| Equivalent under the frozen source | 47 | 46 earlier claims independently verified from prefix lengths, array positions and masks. One further reserved-suffix mutation proved equivalent from the earlier private-host check. |
| Implementation detail in current callers | 1 | A malformed raw private-host string changes `isPrivateNetworkHost`, while public favicon eligibility and URL-based production callers are unchanged. |
| Unresolved | 0 | Each survivor has a recorded classification and supporting evidence. |

The copied upstream source is commit `31a9da179ed0763335f05681c577474aec5d2309`. `hostClassification.ts` has SHA-256 `07576a1ed02ccbd1b3b84d82bbb95d663959edaaf87f470324a5d361fd89e66a`. T3's MIT notice is retained in `LICENSE`.

The Node 24 pure-module probe checks all 153 original survivors over 4,431 recorded host strings and four exports. It also extracts and checks 60 existing selected shared assertions and 111 existing assertions from the two omitted browser classification blocks. All extracted original assertions pass. No selected shared assertion unexpectedly changes under a surviving mutation. There are 106 mutants with observed exported differences and 47 without sampled differences.

The 47 equivalence findings do not rely on sampled outputs. Forty-four mutations change prefix array entries the current matcher never reads. One changes a low bit removed by the partial prefix mask. One changes the zero-remainder shortcut; the only current remainders are 0, 7 and 12, and a zero remainder still gives a zero mask and a true final comparison. The remaining mutation changes `suffix.slice(1)` to `suffix.slice(0)`. Bare reserved names already return through `isPrivateNetworkHost`, and dotted names already match `endsWith`.

The two omitted classification blocks are existing T3 tests at `browserTargetResolver.test.ts:220` and `:276`. They must not be described as missing upstream tests. The other existing consumer tests begin at `:192` and `:203`. The consumer probe retains the original resolver function code and replaces imported classifiers and prepared connection reads with fixed test data. It does not execute the original Vitest module.

The 13 additional public classifier witnesses include eight cases that reach current URL-based callers, one valid single-zero compressed IPv6 spelling that URL canonicalization changes before classification, and four malformed addresses rejected by URL parsing. The latter five demonstrate differences in the exported host-string API. They do not demonstrate changed current URL-based product behavior. Their absence is established only for the inspected test cases, not every T3 test.

All survivor IDs, operators, source locations, edits, reasons and proof status are in `survivor-classification.json` and `survivor-classification.tsv`. `probe-results.json` preserves direct differences and existing assertion failures. `probe-cases.json` preserves the full corpus and assertion extraction. `remaining-probe-results.json` preserves every additional witness, URL canonicalization result, consumer result and the malformed-only private export difference. `provenance.json` records hashes, notices, runtime, adaptation and reproduction commands. No production source or original report was edited, and no mutation score target was used.
