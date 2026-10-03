# Archguard

Archguard checks architecture rules across a source repository. It follows dependencies, enforces package and module boundaries, detects cycles, and checks file conventions such as companion files and registry imports. Rust and TypeScript plugins can add checks over the same project graph. An optional TypeScript compiler provider checks inferred public members.

## Get started

Build from this checkout with Rust 1.96 or newer:

```sh
cargo build --release --locked
```

Create `archguard.json` in the project you want to check:

```json
{
  "schemaVersion": 1,
  "include": ["src/**/*.ts", "src/**/*.tsx"],
  "rules": [
    {
      "id": "client-server",
      "kind": "forbiddenDependency",
      "files": ["src/client/**"],
      "targets": ["src/server/**"],
      "transitive": true
    }
  ]
}
```

Run the binary with your project's root and configuration path:

```sh
./target/release/archguard check --root /path/to/project --config /path/to/project/archguard.json
```

Add `--json` for machine-readable findings. `archguard facts` exports the resolved project graph for custom tooling.

| Exit | Meaning |
| --- | --- |
| `0` | Analysis completed and every check passed |
| `1` | Analysis completed with policy violations |
| `2` | Analysis could not complete, or configuration is invalid |

Parser errors and unresolved internal dependencies remain visible. Select all source files needed by your checks; selectors do not automatically include dependency closures. Source resolution follows explicit configuration and is separate from compiler checking and runtime behavior.

## Choose your checks

- [Built-in rules and configuration](docs/guides/configuration.md) cover dependencies, imports, calls, cycles, exports, and packages.
- [Repository rules](docs/guides/repository-rules.md) classify file roles and check companions, registry import prerequisites, and dependencies between roles.
- [Rust and TypeScript plugins](docs/extensions/plugins.md) use the same versioned project facts. The TypeScript SDK and examples require Node 24.
- [Compiler member checks](docs/extensions/semantic-provider.md) use an optional, separately installed TypeScript 7.0.2 provider.
- [Graph caching](docs/guides/cache.md) is opt-in. Recorded measurements have not shown a speed improvement.

Plugins and semantic providers are trusted commands you explicitly configure. Subprocess execution currently requires Unix.

Start with the [runnable examples](examples/README.md), including a [T3 Code adoption case study](examples/t3code/README.md). The [documentation index](docs/README.md) has reference and extension guides.

Archguard is MIT licensed. See [dependency licenses](docs/licenses/README.md) when distributing binaries or upstream assets.
