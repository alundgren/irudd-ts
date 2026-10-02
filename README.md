# Archguard

A Rust CLI for repository architecture contracts. Rust and TypeScript plugins receive the same resolved module graph, package inventory and syntax facts. Rules can follow dependencies across files without rebuilding that information.

Requires Rust 1.96 or newer. TypeScript plugin examples and local validation require Node 24. This release runs subprocess plugins on Unix only.

```bash
cargo build --release --locked
./target/release/archguard check --root . --config archguard.json
./target/release/archguard facts --root . --config archguard.json > project-facts.json
scripts/check.sh
```

`check --json` emits a deterministic diagnostic order plus scan counts, completeness and elapsed time. Exit codes are 0 for a complete clean check, 1 for policy violations, and 2 for incomplete analysis or invalid configuration. A parser error, unsupported dynamic target, excluded source dependency or unresolved internal import cannot produce a clean result. Diagnostics may still identify violations in an incomplete project, but the result requires attention.

Configuration is explicit JSON with `schemaVersion: 1`. Unknown keys and duplicate rule IDs fail. `--root` is relative to the invoking directory. Plugin commands run in the configuration file's directory, with that directory as their working directory. Glob selectors use `/` and match paths relative to the analysis root.

```json
{
  "schemaVersion": 1,
  "rules": [
    {"id":"client-server", "kind":"forbiddenDependency", "files":["client/**"], "targets":["server/**"], "transitive":true}
  ],
  "plugins": [
    {"name":"team-rules", "command":["node", "./team-rules.ts"], "timeoutMs":5000}
  ]
}
```

Available built-in rule kinds are `forbiddenDependency`, `forbiddenImport`, `forbiddenCall`, `noCycles`, `serviceNamespace`, `uniqueServiceId`, `serviceLayer`, `requiredExport`, `requiredFile`, `packageDependency` and `publicEntry`. Policy IDs and selectors turn these into repository-specific contracts. `includeTypes` defaults to true. Cycle checks exclude literal dynamic-import edges; other dependency checks include them. `exceptions` excludes source files for a particular rule. A `publicEntry` rule uses `targets` for protected files and `specifiers` for permitted public imports.

The source graph uses Oxc Resolver with nearest-tsconfig path mappings and configured export conditions, which default to `types`, `import`, `default`. It records `.js` to `.ts`/`.tsx`, `.mjs` to `.mts` and `.cjs` to `.cts` aliases. It is not a TypeScript type checker or a claim of compiler/runtime resolution parity. Uninstalled external packages remain external edges. Non-source assets are leaves. Workspace package manifests default to the root and immediate children of `apps`, `packages` and `infra`; set `packageManifests` explicitly for another layout. Packages must declare source exports to participate in workspace resolution. Scan selectors are not dependency closure: excluded or unselected source targets make analysis incomplete.

TypeScript syntax facts use lexical bindings. An imported alias such as `E.runPromise()` can be attributed to Effect, while a locally shadowed `E` is not. Reassignment, dynamic property names, arbitrary object aliases, macros and inferred types are outside this release's analysis. Rust facts support direct `crate::module` use paths for local dogfooding; they do not expand macros, nested/inline modules or resolve Rust types.

For TypeScript plugins, import `runPlugin`, `ProjectFacts`, `Diagnostic` and `dependencyPaths` from [sdk/index.ts](sdk/index.ts). [examples/graph-plugin.ts](examples/graph-plugin.ts) follows client dependencies transitively. Rust rules implement `ProjectRule`; [examples/graph_plugin.rs](examples/graph_plugin.rs) uses the identical facts and policy. Rust extensions are statically linked or separate executables, avoiding a compiler-dependent dynamic ABI.

Subprocess plugins are trusted local code. Archguard executes only commands you explicitly configure, passes one versioned JSON project on stdin, and requires one versioned JSON response on stdout. Logs belong on stderr. It bounds stdout and stderr, handles stdin concurrently and terminates the Unix process group on timeout or exit. Diagnostics must name known analyzed source files or package manifests and valid byte offsets. Repository diagnostics use `.` with offset zero. This is process management, not a sandbox.

Licenses are checked against locked dependency manifests by `scripts/licenses.py`. The repository is MIT licensed. Preserve dependency license texts when distributing compiled binaries.
