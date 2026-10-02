# Working on Archguard

Archguard checks repository architecture. Oxc supplies TypeScript syntax and lexical bindings; Oxc Resolver supplies configured source module resolution. Neither gives this tool a TypeScript type checker. Keep that distinction in code, diagnostics and reports.

Modules:
- `facts`: the versioned project/plugin data contract and diagnostics.
- `config`: JSON configuration, validation and file selectors.
- `typescript` and `rust`: syntax providers; they produce facts without deciding policy.
- `project`: file discovery, package inventory and module resolution.
- `rules`: repository-wide policy over the immutable facts.
- `plugin`: explicit trusted subprocess execution with bounded IO and deadlines.
- `main`: command parsing, rendering and exit codes.

Run `scripts/check.sh` locally before a PR. Do not add CI. Tests must exercise a meaningful failure, a correction and relevant negative controls. Run our own CLI on `archguard.json` when changing the SDK or examples; add applicable contracts as the tool grows.

Never silently discard parser errors or unresolved internal graph edges. Exit 0 means a complete clean check; exit 1 means policy violations; exit 2 means the check could not complete. Every import remains visible with its resolution status. Source resolution is configured explicitly and must not be described as TypeScript compiler or runtime parity.

Plugins are trusted code selected by explicit configuration. Never auto-discover or execute code from a scanned repository. The JSON protocol is versioned; Rust and TypeScript plugins receive the same project graph. Keep a Rust/TypeScript equivalent-rule test.

Historical experiments record exact upstream commits and distinguish full replay, reduced reproduction and synthetic policy fixtures. Timing comparisons must use equivalent files and checks. A fixed revision that still triggers the claimed regression has not passed.

Check licenses before adding dependencies or copying upstream code. Retain upstream notices for copied fixtures. Keep the dependency-license report current. Do not import proprietary rules or tools with incompatible redistribution terms.

Use direct language. Do not introduce the words forbidden by the session's global AGENTS instructions in prose or new code names. Use `gh` for GitHub operations. Independent review is required before merging. The task integration branch is `t3code/rust-architecture-enforcement-cli`; child PRs target it. The final reviewed PR targets `main` and may merge under the user's authorization.

Keep source-resolution profiles in sync with the SDK facts. Test platform lookup ordering and inherited aliases whenever resolver options change. T3 profiles are separate adoption candidates; distinguish current policy, historical migration policy and experimental rules in the catalog. A preset-entry count is not a count of demonstrated historical bugs. Module reachability must not be described as execution reachability.

Reuse a TypeScript dependency query when checking many roots. Benchmark changes to SDK traversal against both selected violations and clean controls; keep baseline source revisions and executable hashes explicit when comparing before/after results.
