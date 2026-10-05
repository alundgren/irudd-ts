# Keep server code out of the client

Report a configured restriction when client code reaches a server module through a shared module.

archguard.json:

```json
{
  "schemaVersion": 1,
  "include": ["src/**/*.ts"],
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

src/client/main.ts:

```typescript
import '../shared/data';
```

src/shared/data.ts:

```typescript
import '../server/db';
```

Archguard reports:

```text
src/client/main.ts:1:1: client-server: forbidden dependency on src/server/db.ts
  src/client/main.ts -> src/shared/data.ts -> src/server/db.ts
```

Fix · src/client/main.ts:

```typescript
export {};
```

Exit 1 with the finding above. After the fix, exit 0.

Reference: https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md
