# Make other code use a package's public entry

Stop code from reaching into a workspace package's private files.

archguard.json:

```json
{
  "schemaVersion": 1,
  "include": ["src/**/*.ts", "packages/**/*.ts"],
  "rules": [
    {
      "id": "api-public-entry",
      "kind": "publicEntry",
      "files": ["src/**"],
      "targets": ["packages/api/src/**"],
      "specifiers": ["@app/api"]
    }
  ]
}
```

src/main.ts:

```typescript
import '../packages/api/src/index';
```

Archguard reports:

```text
src/main.ts:1:1: api-public-entry: import packages/api/src/index.ts through its declared public package entry
```

Fix · src/main.ts:

```typescript
import '@app/api';
```

Exit 1 with the finding above. After the fix, exit 0.

Reference: https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md
