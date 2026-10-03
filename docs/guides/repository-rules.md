# Repository structure as policy

Archguard can use a repository's file conventions to check relationships between files. Use the optional `repository` configuration section to assign roles and check conventions.

Roles are policy derived from analyzed file paths. Rust exposes `RepositoryPolicy`, `FileRole` and `RepositoryRule` in [roles.rs](../../src/roles.rs). `RepositoryPolicy` implements `ProjectRule`; `assignments` returns every analyzed source with zero, one or several matching role IDs. Roles do not become compiler facts or inferred runtime behavior. Rust and TypeScript plugins still receive the same version 1 facts; the TypeScript SDK does not yet offer a role-policy helper.

## Encoding a convention

This complete minimal policy requires a matching layer file for each service file:

```json
{
  "schemaVersion": 1,
  "repository": {
    "roles": [
      {"id":"service", "files":["src/Services/*.ts"], "exclude":["**/*.test.ts"]}
    ],
    "rules": [
      {"id":"service-layer", "kind":"companion", "role":"service", "replace":["/Services/", "/Layers/"]}
    ]
  }
}
```

Adding `src/Services/Payments.ts` without `src/Layers/Payments.ts` now produces a violation. The companion check requires an analyzed source at the expected path. It does not prove that the layer implements the service. Existing export and dependency rules can supply more prerequisites; proving the actual construction needs additional syntax or type facts.

| Repository rule | What it checks | Evidence limits |
|---|---|---|
| `classified` | Every analyzed source selected by `files`, minus `exclude`, has exactly one role. | Roles may overlap outside this explicit scope. Files not selected for analysis are outside the inventory. |
| `companion` | Each member of `role` has an analyzed source at its transformed path. | `replace` is a pair of nonempty distinct strings. Its first string must occur exactly once; invalid mappings fail with exit 2. There is no wildcard capture or filesystem probing. |
| `registryImport` | The named analyzed `registry` has an internal, non-type `import` or `importEquals` declaration targeting each member of `role`. | Dynamic imports, ordinary `require` calls and re-exports do not qualify. An imported migration can still be omitted from a registry tuple. This checks an import prerequisite only. |
| `forbiddenDependency` | A file in role `from` has a direct or transitive resolved module dependency on role `to`. | `includeTypes` defaults to true; `transitive` defaults to false. Other dependency checks include literal dynamic imports. This is module reachability. |

Role selectors use the same root-relative glob matching as existing rules. Role `exclude` applies to both source and target membership. Repository rule IDs share the existing rule-ID namespace. Unknown keys, unknown role references, duplicate IDs and invalid glob patterns fail configuration validation.

Discovery still comes from the top-level `include` and `exclude`, which are not expanded by a role selector or a companion. For a complete dependency check, select the needed dependency closure explicitly. Missing internal edges, parser problems and excluded sources keep the normal incomplete result and exit 2. A missing analyzed companion is a policy violation, even if a file exists outside the selected inventory. A missing analyzed registry is also a violation. Restrict file-presence policies to source files supported by the configured providers.


The [repository-rule example](../../examples/repository-rules/README.md) is runnable without an external checkout. The [T3 case study](../../examples/t3code/README.md) links its separate adoption profile and evidence.
