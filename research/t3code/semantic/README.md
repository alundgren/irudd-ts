# Semantic history and cost

This directory owns the reduced T3 compiler reproduction and its Effect dependency. The [provider](../../../providers/typescript7/README.md) installs the compiler separately.

```sh
npm ci --prefix providers/typescript7 --ignore-scripts --no-audit --no-fund
npm ci --prefix research/t3code/semantic --ignore-scripts --no-audit --no-fund
cargo test --locked --features semantic-tests,research-tests --test t3_semantic
python3 research/t3code/scripts/verify_semantic_history.py /path/to/t3code
```

Fixture hashes and original excerpts are recorded in [evidence.json](evidence.json). Copied snippets retain [the upstream MIT notice](fixtures/LICENSE).

## Historical evidence


[PR #11304](https://github.com/pingdotgg/t3code/pull/11304) corrected a real merged integration regression. Before `e1c94f703f05b43f2935d09ed1ef882d0ee13ff8`, the device hub ticket callback accessed `client.auth.webSocketTicket` although its helper supplied a group client. Fixed `fb52d125b78b2ed1638a719a7546fad9c695c620` adds `group: "auth"` and accesses `client.webSocketTicket`.

The MIT fixtures preserve the exact request objects, generic parameter signature and group-client implementation. They use real Effect 4.0.0-rc.112 declarations. The API contains only the auth ticket endpoint; unrelated environment/service values and request/response schemas use explicit substitutes. The Effect.fn implementation wrapper and request execution are omitted. This is a reduced compiler reproduction, not a full repository or runtime replay. Ordinary TypeScript checking also detects it. Archguard's addition is reusable inferred member/type/symbol facts beyond its lexical import/call data.

The before case produces TS2741 and TS2339, a known missing `auth` member and an error receiver on the following access. The fixed case has no diagnostics and a present generated `webSocketTicket` member. Tests include an incomplete correction, unrelated same-name members, any/unknown/error controls and an independent site inventory. Verify copied snippets against a local historical clone with `python3 research/t3code/scripts/verify_semantic_history.py /path/to/t3code`.

TypeScript 6.0.3 was used only during initial research. Its inference, member and symbol facilities already exist; this change does not claim those are new to TypeScript 7. The selected native API adds a different sync/async connection and snapshot arrangement, batch lookups, and an explicit public error-type predicate. Incremental API performance and TS6/TS7 semantic parity were not established by the preliminary research.

Measurements are recorded separately after a serialized local benchmark window. Compare equivalent selected sources, compiler contexts and rules. Include provider-disabled and enabled runs, raw samples, source/corpus/config hashes and the native/provider executable hashes. Do not compare this reduced reproduction to a full lexical repository scan as equivalent work.

Run the cost comparison after building all release binaries and examples:

```sh
cargo build --release --locked --bins --examples
python3 research/t3code/scripts/benchmark_semantic.py --binary target/release/archguard --output research/local/semantic.json
```

The third measurement runs the provider directly on a request emitted by the Rust SDK example, excluding host graph discovery and site inventory from that measurement. Its process exit 0 means a valid protocol response; the response's complete flag still distinguishes the historical failure. CLI exit codes retain their usual meaning. Native executable hashing in this measurement script currently targets Linux x64. SemanticFacts.complete describes compiler capability completion; source graph problems are also printed and prevent a clean CLI exit.

The recorded [seven-sample cost result](../results/compiler-semantic-provider.json) uses clean source revision `044caef1e431fbab485e81b76c5be5ff4bb906d6`, Linux x64 and Node 24.21.0. Each cell shows wall-time median and full range in milliseconds:

| Selected source | Graph only | Compiler enabled | Direct provider |
| --- | ---: | ---: | ---: |
| PR11304 before | 6.34 (5.03–8.93) | 654.65 (616.35–787.93) | 635.57 (565.41–792.04) |
| PR11304 fixed | 4.81 (3.82–5.78) | 646.32 (569.81–797.33) | 637.38 (610.22–728.96) |
| Unrelated clean control | 4.00 (3.00–5.95) | 289.10 (270.19–369.73) | 288.96 (249.82–315.70) |

The enabled check costs 72–134 times the graph-only median on these single-file sources. It adds compiler diagnostics and member/type/symbol queries, including transitive Effect declarations for the historical fixture; the graph-only check does not perform those checks. This comparison measures the cost of enabling the capability on the same selected source, not interchangeable checking implementations. Before remains incomplete with a known missing `auth` member, an error receiver and two compiler errors; fixed and control remain complete. The raw artifact retains every sample, requests, provider facts, normalized output hashes and executable/configuration/source hashes.

Each sample starts fresh host/provider/native processes. Filesystem caches remain available; modes run in a fixed order without an explicit warmup. Other task owners paused heavy work during the reserved window, but background host activity can affect the ranges. Direct-provider and enabled ranges overlap, so subtracting their medians does not establish host overhead. No full-repository, incremental API or TypeScript 6 versus 7 performance conclusion follows from this reduced measurement.
