# Persistent graph cache

Caching is opt-in. Supply a cache file outside selected source and package-manifest paths:

```sh
archguard check --root /path/to/project --config archguard.json --cache /tmp/project.archguard-cache.json --json
archguard facts --root /path/to/project --config archguard.json --cache /tmp/project.archguard-cache.json
```

The cache stores syntax facts and resolved imports. Every invocation discovers files and packages again, reads selected source bytes, checks resolver filesystem inputs and reruns policy rules and configured plugins. An unchanged graph can reuse resolved imports; a changed source file is parsed and resolved again. Changes to a target's exports still affect policy because rules inspect the newly assembled project.

Cache identity includes the canonical project root, effective configuration, facts and cache versions, executing binary digest and embedded Cargo lock digest. Changing parser, resolver, host code or resolver options requires a new entry. Source validation uses SHA-256 of the bytes rather than modification time or file size.

The resolver records content reads, file-kind checks, missing paths, symbolic links and canonical paths through its public filesystem interface. This includes ancestor, inherited and referenced tsconfigs and installed package metadata, even when they are outside the project root. Workspace package inputs and the set of successfully parsed source files are also checked. A changed resolver input invalidates all cached resolved imports. This conservative invalidation keeps shared resolver lookups accounted for. Matching syntax entries can still be reused.

Only complete source graphs are written. A parser failure or unresolved internal dependency remains an analysis problem. Full installed mobile profiles currently contain unsupported or generated inputs, so they do not produce reusable complete graph entries. Caching does not turn those profiles into clean checks. This restriction applies even if their unaffected files could be useful partial cache candidates.

The cache envelope has an integrity checksum and is replaced atomically. A corrupt cache is discarded; the invocation analyzes fresh source and reports a cache problem with exit 2. When fresh analysis completes and persistence works, it replaces the bad entry so a later invocation can recover. A changed cache version is an ordinary miss. Cache read or write failures also produce exit 2. Cache data is local trusted data; the checksum detects accidental corruption and is not an authentication mechanism.

The `facts` contract is unchanged. JSON `check` reports include optional `cache` statistics when caching is requested: parsed and reused file counts, resolved and reused import counts, observed resolver input count and whether a new entry was published. Existing no-cache reports omit that field. Reuse statistics establish avoided parsing and resolution work; they do not establish a latency improvement.

## Reproducing measurements

Build the CLI and examples with the repository's pinned Rust toolchain, then run trials while other builds and analyses are idle:

```sh
cargo build --release --locked --bins --examples
python3 benchmarks/verify_history.py /path/to/t3code
python3 scripts/benchmark_cache.py --binary target/release/archguard --output benchmarks/local/cache.json
```

The runner checks cached and fresh facts and diagnostics after every edit step. PR13151 replays the existing verified type-cycle reduction under generic, Android and iOS orders, including a missing-target intermediate and its correction. PR14389 changes the barrel export while its consumer import stays unchanged. These are reduced source-edit replays of merged PRs. Their chosen edit order is a measurement sequence, not a reconstruction of the original development session.

Trials compare fresh analysis, an empty persistent cache, unchanged reuse and edited reuse. Empty cache describes persisted data; the filesystem may already be warm. The runner retains every sample, ranges, corpus hashes, upstream evidence, source revision and executable hash. Incomplete intermediate states remain exit 2 and are not accepted as complete entries. Historical clean controls run alongside violations.

An optional complete installed-project control can be added with `--control-root`, `--control-config` and `--control-revision`. It measures the selected source inventory recorded in its profile, separately from historical PR evidence. Do not infer whole-repository coverage from a selected dependency closure.

The pre-implementation reduction baseline is retained separately from the same-binary comparisons. These cases contain only two to four selected files, so content validation and persistence can cost more than fresh parsing. Report slower cache runs and their ranges. There is no general speed claim.
