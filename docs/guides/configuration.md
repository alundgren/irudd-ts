# Configuration and commands

Configuration is explicit JSON with `schemaVersion: 1`. Unknown keys, invalid selectors, and duplicate rule IDs fail. `--root` and `--config` resolve from the invoking directory. Glob selectors use `/` and match paths relative to the analysis root.

```sh
archguard check --root /path/to/project --config /path/to/project/archguard.json
archguard check --root /path/to/project --config /path/to/project/archguard.json --json
archguard facts --root /path/to/project --config /path/to/project/archguard.json > project-facts.json
```

The JSON check report includes sorted diagnostics, analysis problems, scan counts, completeness, and elapsed time. Exit 0 means a complete clean check, exit 1 means complete policy violations, and exit 2 means incomplete analysis or invalid configuration. Findings can remain useful in an incomplete report.

## Source selection

`include` selects supported source files. Defaults select TypeScript and JavaScript variants recursively. `exclude` removes files from discovery. Source dependencies excluded by either selector remain incomplete internal edges. File-role selectors and compiler-context selectors filter the analyzed files; neither expands discovery.

`packageManifests` selects package manifests for workspace inventory. Defaults are `package.json`, `apps/*/package.json`, `packages/*/package.json`, and `infra/*/package.json`. Configure it explicitly for another layout. Workspace packages need source exports to participate in source resolution.

## Source resolution

Oxc supplies TypeScript syntax and lexical bindings. Oxc Resolver supplies source-module lookup with nearest-tsconfig path mappings and inheritance. Neither supplies compiler types. Source resolution does not promise compiler or runtime parity.

| Option | Default and purpose |
| --- | --- |
| `conditions` | `types`, `import`, `default`, as enabled export conditions. Conditional object keys are considered in package declaration order |
| `extensions` | `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, `.cjs`, `.json` for extensionless lookup |
| `extensionAliases` | Pairs of requested suffix and ordered candidates. Defaults map `.js` to `.ts`/`.tsx`/`.js`, `.mjs` to `.mts`/`.mjs`, and `.cjs` to `.cts`/`.cjs` |
| `requireExternalResolution` | `false`; set `true` after installation to require successful external package lookups |

Uninstalled external packages remain external edges in the default `source` mode. Required package lookup uses the `installed-source` facts profile. Missing configured export targets and unsupported host module schemes remain problems with exit 2. Node builtins are external endpoints. Installed dependency source is outside graph traversal; non-source assets are leaves.

Rust analysis supports direct `crate::module` imports for repository policy checks. It resolves the first module component to `src/module.rs` or `src/module/mod.rs`. It does not expand macros, resolve nested paths fully, analyze inline module declarations, or provide Rust compiler types. Relative `self::` and `super::` imports are unsupported and remain visible problems.

TypeScript imported aliases can identify calls such as `E.runPromise()` while local shadowing prevents that attribution. Reassignment, arbitrary object aliases, dynamic property names, and inferred types require other analysis.

## Policies and extensions

`rules` contains [built-in rules](rules.md). `repository` contains [file roles and repository rules](repository-rules.md). `plugins` contains [explicit trusted graph-plugin commands](../extensions/plugins.md). Each rule uses its own stable ID in diagnostics.

[Compiler member checks](../extensions/semantic-provider.md) use a separate `--semantic-config` file. [Caching](cache.md) uses `--cache /path/to/cache.json`; policies and configured plugins still run every time.
