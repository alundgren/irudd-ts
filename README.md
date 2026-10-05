# Archguard

Archguard checks architecture rules across a source repository. It follows dependencies, enforces package and module boundaries, detects cycles, and checks file conventions such as companion files and registry imports. Rust and TypeScript plugins can add checks over the same project graph. An optional TypeScript compiler provider checks inferred public members.

The [Archguard site](https://alundgren.github.io/irudd-ts/) shows what it adds to Oxlint, with short examples. Agents can start with [llms.txt](https://alundgren.github.io/irudd-ts/llms.txt).

## Get started

Install an exact [released binary](docs/guides/installation.md) for Linux or macOS on x86_64 or arm64. Binary releases become available after the first manual release. To build from this checkout instead, use Rust 1.96 or newer:

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

For optional structural duplicate and mutation feedback, see the [code-quality guide](docs/code-quality.md) and [authored examples](examples/code-quality/README.md). Findings are review evidence, with no CI gate or score target.

To have an agent evaluate architecture checks, duplication improvements, and tests within a time and disk budget, install the [evaluation skill](docs/guides/evaluate.md). It publishes an authorized draft PR before mutation testing and updates it as results arrive.
