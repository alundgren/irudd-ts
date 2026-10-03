# Compiler member facts

Archguard can opt into a separate TypeScript compiler capability. Oxc continues to provide syntax and lexical bindings for ProjectFacts v1. The compiler result is SemanticFacts v1, with explicit compiler contexts and property-access facts. Existing graph plugins receive exactly their existing ProjectFacts v1 payload. Rust and TypeScript readers expose the separate semantic contract.

The adapter pins the released native TypeScript compiler 7.0.2 and its **experimental** `typescript/unstable/sync` JavaScript API. The user selected this API despite its current instability. This is not a promise of stable Microsoft APIs or compatibility with 7.1. No older compiler fallback runs. Updating the pin requires adapter, historical, completion and SDK checks together. The published source revision is [2bd066d87f5bafd315be9f40889d0a60b9e58e0b](https://github.com/microsoft/typescript-go/commit/2bd066d87f5bafd315be9f40889d0a60b9e58e0b). [Microsoft's 7.0 release notes](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/) describe its current API status.

Install the optional provider and historical test dependencies without lifecycle scripts:

```sh
npm ci --prefix providers/typescript7 --ignore-scripts --no-audit --no-fund
```

The compiler's native optional package must be installed for the current platform. Missing API exports or binaries become incomplete analysis. The subprocess runner currently requires Unix process-group cleanup. Provider input is bounded to 64 MiB, output to 8 MiB, stderr to 64 KiB, and the configured deadline includes IO and response validation. The default native API child inherits the provider's process group. Compiler configuration does not authorize repository plugins or external content mappers. The adapter passes only the root directory to API construction and rejects exposed external-code options.

Create an explicit semantic configuration next to your normal archguard.json. Provider commands are trusted code chosen by the person configuring Archguard. This example assumes the configuration lives at the repository root:

```json
{
  "schemaVersion": 1,
  "provider": {"name": "typescript7", "command": ["node", "providers/typescript7/provider.mjs"], "timeoutMs": 30000},
  "contexts": [{"id": "app", "tsconfig": "tsconfig.json", "files": ["src/**/*.ts"]}],
  "rules": [{"id": "missing-member", "kind": "missingMember", "files": ["**"]}]
}
```

Context tsconfig paths resolve from the scanned root. Provider commands run from the semantic configuration directory. File selectors filter already analyzed sources; they do not expand discovery. Each selected source receives one result in every configured context that selects it. Sources outside the explicit compiler program are unavailable. Multiple contexts remain separate. Compiler project loading follows that pinned compiler's configuration and resolution behavior, independently of Oxc Resolver's profile. Referenced projects require the compiler inputs they normally use; this command does not build missing declaration outputs.

```sh
archguard check --config archguard.json --semantic-config semantic.json --json
archguard semantic-facts --config archguard.json --semantic-config semantic.json
```

The facts command emits only the separate semantic payload. Exit 0 requires complete graph and compiler checks. Exit 1 represents complete policy violations. Compiler errors, unavailable member facts, malformed/omitted provider records, failed processes, and graph problems cause exit 2. A partial result can contain both useful rule findings and compiler errors.

Rust independently inventories public dot property accesses with Oxc and hashes each selected source. The provider returns exactly one fact per inventoried site and compiler context. Rust rejects omitted files/sites/contexts, extra or duplicate sites, mismatched names/byte offsets/hashes, unsupported backend identity and contradictory completion flags. Computed properties such as `x[key]` and private properties such as `x.#field` are outside this first contract. Public optional dot accesses and JSX member tags such as `<View.Child />` are included. Offsets are UTF-8 bytes, converted from TypeScript UTF-16 positions.

Each property has an inferred receiver display type and state, member status and optional resolved symbol/declaration locations. Display types are for reporting, not text that rules should parse. Known member absence differs from `any`, `unknown`, an error type and unavailable queries. Those unavailable states never prove a clean member check. Accesses supported only by an index signature, such as `Record<string, number>.member`, are unavailable when the compiler exposes no named symbol; the rule does not report a missing member in that case. Optional nullable accesses use the compiler-resolved member symbol rather than raw receiver-union lookup. Generated mapped members can exist with no source declarations. Compiler diagnostics retain phase, category, code, message and locations separately from rule findings. Dependency diagnostics can reference files outside source discovery.

Rust implements `semantic::SemanticRule`, `semantic::SemanticFacts::file`, and a missing-member rule. TypeScript exports `SemanticRule`, `semanticFile`, `readSemanticFacts` and `missingMemberDiagnostics` from sdk/semantic.ts and sdk/index.ts. The TypeScript reader consumes host-validated facts; host ingestion performs the provider trust checks. A Rust/TypeScript test runs equivalent member rules over the same payload. No second plugin execution protocol is introduced. A negative control suppresses a compiler diagnostic with `@ts-ignore`; the independent member rule still reports the known missing member from facts.

## Historical evidence

[PR #11304](https://github.com/pingdotgg/t3code/pull/11304) corrected a real merged integration regression. Before `e1c94f703f05b43f2935d09ed1ef882d0ee13ff8`, the device hub ticket callback accessed `client.auth.webSocketTicket` although its helper supplied a group client. Fixed `fb52d125b78b2ed1638a719a7546fad9c695c620` adds `group: "auth"` and accesses `client.webSocketTicket`.

The MIT fixtures preserve the exact request objects, generic parameter signature and group-client implementation. They use real Effect 4.0.0-rc.112 declarations. The API contains only the auth ticket endpoint; unrelated environment/service values and request/response schemas use explicit substitutes. The Effect.fn implementation wrapper and request execution are omitted. This is a reduced compiler reproduction, not a full repository or runtime replay. Ordinary TypeScript checking also detects it. Archguard's addition is reusable inferred member/type/symbol facts beyond its lexical import/call data.

The before case produces TS2741 and TS2339, a known missing `auth` member and an error receiver on the following access. The fixed case has no diagnostics and a present generated `webSocketTicket` member. Tests include an incomplete correction, unrelated same-name members, any/unknown/error controls and an independent site inventory. Verify copied snippets against a local historical clone with `python3 scripts/verify_semantic_history.py /path/to/t3code`.

TypeScript 6.0.3 was used only during initial research. Its inference, member and symbol facilities already exist; this change does not claim those are new to TypeScript 7. The selected native API adds a different sync/async connection and snapshot arrangement, batch lookups, and an explicit public error-type predicate. Incremental API performance and TS6/TS7 semantic parity were not established by the preliminary research.

Measurements are recorded separately after a serialized local benchmark window. Compare equivalent selected sources, compiler contexts and rules. Include provider-disabled and enabled runs, raw samples, source/corpus/config hashes and the native/provider executable hashes. Do not compare this reduced reproduction to a full lexical repository scan as equivalent work.

Run the cost comparison after building all release binaries and examples:

```sh
cargo build --release --locked --bins --examples
python3 scripts/benchmark_semantic.py --binary target/release/archguard --output benchmarks/local/semantic.json
```

The third measurement runs the provider directly on a request emitted by the Rust SDK example, excluding host graph discovery and site inventory from that measurement. Its process exit 0 means a valid protocol response; the response's complete flag still distinguishes the historical failure. CLI exit codes retain their usual meaning. Native executable hashing in this measurement script currently targets Linux x64. SemanticFacts.complete describes compiler capability completion; source graph problems are also printed and prevent a clean CLI exit.
