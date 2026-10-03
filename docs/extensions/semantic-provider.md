# Compiler member facts

Archguard can opt into a separate TypeScript compiler capability. Oxc continues to provide syntax and lexical bindings for ProjectFacts v1. The compiler result is SemanticFacts v1, with explicit compiler contexts and property-access facts. Existing graph plugins receive exactly their existing ProjectFacts v1 payload. Rust and TypeScript readers expose the separate semantic contract.

The adapter pins the released native TypeScript compiler 7.0.2 and its **experimental** `typescript/unstable/sync` JavaScript API. This is not a promise of stable Microsoft APIs or compatibility with 7.1. No older compiler fallback runs. Updating the pin requires adapter, historical, completion and SDK checks together. The published source revision is [2bd066d87f5bafd315be9f40889d0a60b9e58e0b](https://github.com/microsoft/typescript-go/commit/2bd066d87f5bafd315be9f40889d0a60b9e58e0b). [Microsoft's 7.0 release notes](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/) describe its current API status.

Install the optional provider without lifecycle scripts:

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

Rust implements `semantic::SemanticRule`, `semantic::SemanticFacts::file`, and a missing-member rule. TypeScript exports `SemanticRule`, `semanticFile`, `readSemanticFacts` and `missingMemberDiagnostics` from [sdk/semantic.ts](../../sdk/semantic.ts) and [sdk/index.ts](../../sdk/index.ts). The TypeScript reader consumes host-validated facts; host ingestion performs the provider trust checks. A Rust/TypeScript test runs equivalent member rules over the same payload. No second plugin execution protocol is introduced. A negative control suppresses a compiler diagnostic with `@ts-ignore`; the independent member rule still reports the known missing member from facts.


See the [runnable semantic example](../../examples/semantic/README.md) and [historical reproduction and measurements](../../research/t3code/semantic/README.md).
