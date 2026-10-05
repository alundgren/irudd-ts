# Require a registry to import every migration

Catch a new migration with no static value import in the registry.

Category: Keep file conventions
Capabilities: repository.registryImport

You need: Built Archguard CLI on your PATH.

Diagram: migrations.ts → Static value import → 001.ts · required

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts"
  ],
  "repository": {
    "roles": [
      {
        "id": "migration",
        "files": [
          "src/migrations/*.ts"
        ],
        "exclude": [
          "**/*.test.ts"
        ]
      }
    ],
    "rules": [
      {
        "id": "migration-import",
        "kind": "registryImport",
        "role": "migration",
        "registry": "src/migrations.ts"
      }
    ]
  }
}
```

## src/migrations/001.ts · before

```typescript
export default 1;
```

## src/migrations.ts · before

```typescript
export const migrations = [];
```

## src/migrations.ts · correction

```typescript
import first from './migrations/001';
export const migrations = [first];
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding migration-import. After the correction: exit 0.

## Keep in mind

Dynamic imports, require calls, type imports, and re-exports do not qualify. This checks imports, not registry membership or execution.

## Reference and source

- [docs/guides/repository-rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/repository-rules.md)

## Related recipes

- [companions](https://alundgren.github.io/irudd-ts/recipes/companions/index.md)
