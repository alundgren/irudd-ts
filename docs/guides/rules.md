# Built-in rules

Put rules in the top-level `rules` array. Every rule needs `id` and `kind`. `files` selects sources, or package manifests for `packageDependency`, and defaults to `**`. `exceptions` removes source files from a rule's scope, except that `requiredFile` always checks every configured `files` pattern. Selectors use root-relative globs.

| Kind | Configuration | Check |
| --- | --- | --- |
| `forbiddenDependency` | `files`, `targets`, optional `transitive` | Selected sources must not reach protected source files through resolved imports |
| `forbiddenImport` | `files`, `specifiers` | Selected sources must not import matching written module specifiers |
| `forbiddenCall` | `files`, `origins`, optional `transitive` | Selected sources must not call imported origins, or depend on modules with those calls |
| `noCycles` | `files` | Dependency cycles intersecting selected sources are violations |
| `requiredExport` | `files`, `names` | Selected sources must expose each name with one visible export origin |
| `requiredFile` | `files` | Every selector must match at least one analyzed source |
| `packageDependency` | Manifest `files`, `specifiers` | Selected workspace packages must not declare matching dependencies |
| `publicEntry` | Consumer `files`, protected `targets`, permitted `specifiers` | Imports into protected files must use an allowed public specifier |
| `serviceNamespace` | Consumer `files` | Value consumers of recognized Effect service exports must use a namespace import |
| `uniqueServiceId` | `files` | Recognized literal Effect service identifiers must be unique |
| `serviceLayer` | `files` | Recognized service files must expose exactly one visible value export named `layer` |

`includeTypes` defaults to `true`. Use `false` where erased type imports should not count. `transitive` defaults to `false`. Cycle checks omit literal dynamic-import edges; other dependency checks include them.

```json
{
  "schemaVersion": 1,
  "rules": [
    {
      "id": "client-server",
      "kind": "forbiddenDependency",
      "files": ["src/client/**"],
      "targets": ["src/server/**"],
      "transitive": true,
      "includeTypes": false
    },
    {
      "id": "client-runtime",
      "kind": "forbiddenCall",
      "files": ["src/client/**"],
      "origins": ["effect/Effect#runPromise"],
      "transitive": true
    }
  ]
}
```

Transitive call checks report a module dependency path to an imported call. They do not prove that a callback executes or an Effect program fails. `requiredExport` checks visible source origins and explicit external declarations; it does not enumerate external star exports. Computed service IDs and arbitrary service factories are outside the recognized syntax.

Package dependencies combine production, peer, and optional dependency names. Development dependencies are outside the current package facts. `requiredFile` checks the analyzed source inventory, not arbitrary files on disk.

The [graph-plugin example](../../examples/graph-plugin/README.md) implements the same transitive boundary in Rust and TypeScript. [Repository rules](repository-rules.md) add classification and companion policies. The [T3 profiles](../../examples/t3code/README.md) show adoption candidates with documented scope and evidence.
