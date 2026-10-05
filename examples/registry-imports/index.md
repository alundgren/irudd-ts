# Never forget to register a migration

The registry must import every migration file.

archguard.json:

```json
{
  "schemaVersion": 1,
  "include": ["src/**/*.ts"],
  "repository": {
    "roles": [
      {
        "id": "migration",
        "files": ["src/migrations/*.ts"],
        "exclude": ["**/*.test.ts"]
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

src/migrations.ts:

```typescript
export const migrations = [];
```

Archguard reports:

```text
src/migrations/001.ts:1:1: migration-import: role migration requires a direct static value import in src/migrations.ts
  src/migrations.ts -> src/migrations/001.ts
```

Fix · src/migrations.ts:

```typescript
import first from './migrations/001';
export const migrations = [first];
```

Exit 1 with the finding above. After the fix, exit 0.

Reference: https://github.com/alundgren/irudd-ts/blob/main/docs/guides/repository-rules.md
