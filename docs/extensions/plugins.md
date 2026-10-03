# Graph plugins and SDKs

Rust and TypeScript graph plugins receive the same ProjectFacts v1 payload: source files, imports with resolution status, exports, recognized calls and services, package inventory, analysis problems, and the configured resolution profile. Rules read an immutable project snapshot.

The Rust SDK is the `archguard` library. Implement `facts::ProjectRule` or use `config::RuleConfig` for built-in policy. The TypeScript SDK is [sdk/index.ts](../../sdk/index.ts); import `runPlugin`, `ProjectFacts`, `Diagnostic`, `dependencyPaths`, and `createDependencyQuery` directly. Node 24 runs these TypeScript examples without a separate compilation step.

```json
{
  "schemaVersion": 1,
  "plugins": [
    {"name": "team-rules", "command": ["node", "./team-rules.ts"], "timeoutMs": 5000}
  ]
}
```

Commands run from the graph configuration directory. Archguard executes only explicitly configured commands. Plugins are trusted code, and process management is not a sandbox. There is no automatic discovery of executable code in the scanned repository.

Each invocation receives one JSON project on stdin and returns one JSON object on stdout:

```json
{"schemaVersion": 1, "diagnostics": []}
```

Logs belong on stderr. Each diagnostic contains `rule`, `file`, `offset`, `message`, and optional `evidence`. Files must refer to analyzed sources or inventoried package manifests; offsets must be in range. Repository-wide findings use `.` with offset zero. Unknown protocol versions and malformed responses fail the check.

The Unix runner bounds input to 64 MiB, stdout to 8 MiB, and stderr to 64 KiB. It handles stdin concurrently and terminates the process group on deadline or exit. Rust plugins can be statically linked or run as executables; Archguard does not use a compiler-dependent dynamic Rust ABI.

For many TypeScript dependency roots, call `createDependencyQuery(project)` once and reuse it. `dependencyPaths` is convenient for a single root. Do not mutate facts while querying them.

See the [equivalent Rust and TypeScript plugins](../../examples/graph-plugin/README.md). The product tests compare selected violations, type-only dependencies, and clean controls across both languages.

Compiler providers return separately versioned [semantic facts](semantic-provider.md). `semantic::SemanticRule` in Rust and `SemanticRule`, `semanticFile`, `readSemanticFacts`, and `missingMemberDiagnostics` in TypeScript consume that contract. It does not replace the graph plugin payload or introduce another plugin execution protocol.
