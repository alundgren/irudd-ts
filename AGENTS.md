# Working on Archguard

Archguard checks repository architecture. Oxc supplies TypeScript syntax and lexical bindings; Oxc Resolver supplies configured source module resolution. Neither provides compiler types. The explicitly configured TypeScript 7 provider supplies a separate compiler-backed semantic contract. Keep source resolution, compiler semantics and runtime behavior distinct in code, diagnostics and reports.

Modules:
- `facts`: the versioned project/plugin data contract and diagnostics.
- `config`: JSON configuration, validation and file selectors.
- `typescript` and `rust`: syntax providers; they produce facts without deciding policy.
- `project`: file discovery, package inventory and module resolution.
- `cache`: optional persistence of syntax facts and resolved source imports, with input validation.
- `semantic`: separately versioned compiler requests, facts, completion validation and member policy.
- `subprocess`: bounded trusted-process transport shared by graph plugins and semantic providers.
- `rules`: repository-wide policy over the immutable facts.
- `roles`: explicit file-role classification and relationships over analyzed source files.
- `plugin`: explicit graph-plugin invocation and protocol validation.
- `main`: command parsing, rendering and exit codes.

Run `scripts/check.sh` locally before a PR. Do not add CI. Tests must exercise a meaningful failure, a correction and relevant negative controls. Run our own CLI on `archguard.json` when changing the SDK or examples; add applicable contracts as the tool grows.

Repository roles are configured policy, not inferred compiler or runtime facts. Role selectors do not expand source discovery. Companion checks require analyzed source files; registry-import checks require static value import declarations and do not prove actual registry membership or execution. Keep the structural T3 simulation separate from the default adoption profile and historical bug evidence.

Never silently discard parser errors or unresolved internal graph edges. Exit 0 means a complete clean check; exit 1 means policy violations; exit 2 means the check could not complete. Every import remains visible with its resolution status. Source resolution is configured explicitly and must not be described as TypeScript compiler or runtime parity.

Plugins are trusted code selected by explicit configuration. Never auto-discover or execute code from a scanned repository. The JSON protocol is versioned; Rust and TypeScript plugins receive the same project graph. Keep a Rust/TypeScript equivalent-rule test.

Historical experiments record exact upstream commits and distinguish full replay, reduced reproduction and synthetic policy fixtures. Timing comparisons must use equivalent files and checks. A fixed revision that still triggers the claimed regression has not passed.

Check licenses before adding dependencies or copying upstream code. Retain upstream notices for copied fixtures. Keep the dependency-license report current. Do not import proprietary rules or tools with incompatible redistribution terms.

Use direct language. Do not introduce the words forbidden by the session's global AGENTS instructions in prose or new code names. Use `gh` for GitHub operations. Independent review is required before merging. The task integration branch is `t3code/rust-architecture-enforcement-cli`; child PRs target it. The final reviewed PR targets `main` and may merge under the user's authorization.

Keep source-resolution profiles in sync with the SDK facts. Test platform lookup ordering and inherited aliases whenever resolver options change. T3 profiles are separate adoption candidates; distinguish current policy, historical migration policy and experimental rules in the catalog. A preset-entry count is not a count of demonstrated historical bugs. Module reachability must not be described as execution reachability.

Reuse a TypeScript dependency query when checking many roots. Benchmark changes to SDK traversal against both selected violations and clean controls; keep baseline source revisions and executable hashes explicit when comparing before/after results.

Benchmark locally with `cargo build --release --locked --bins --examples`, then `python3 scripts/benchmark.py` after the documented pinned toolchain installation. Building only examples does not refresh the CLI. Keep raw measurements tied to source revisions, corpus hashes and executable hashes; preserve range and unfavorable comparisons. Verify copied history with `python3 benchmarks/verify_history.py /path/to/t3code`. No CI is required by this project yet.

Installed-source checks require successful external package lookup when configured. Preserve failures and unsupported schemes; do not invent exclusions to make the full inventory clean. Selected complete dependency closures do not establish full repository coverage.

The semantic provider pins TypeScript 7.0.2 and its unstable public APIs. Archguard versions its own SemanticFacts separately from unchanged ProjectFacts. Never silently fall back to an older compiler. Compiler contexts and provider commands are explicit trusted configuration; reject incomplete site inventories, mismatched source hashes and unavailable required facts. Keep compiler diagnostics separate from policy findings. Computed/private properties are outside the initial contract; unresolved index-signature members remain unavailable. Update the adapter, native historical replay, completion controls and SDK parity together when changing the compiler pin.

Graph caching is opt-in and publishes only complete source graphs. Revalidate source bytes, discovery and observed resolver inputs, including missing paths, inherited configs, package metadata and symbolic links. Rerun all rules and configured plugins over the assembled facts even when imports are reused. Reuse counts do not prove faster checks; the measured initial cache is slower than fresh analysis.

Reserve benchmark sampling across agents. Run builds, independent probes and timings in separate windows. Continuous cache-edit measurements retain the last complete entry through incomplete intermediate states; distinguish those trials from reset-primer measurements. Keep negative measurements and raw ranges.
