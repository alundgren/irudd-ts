# Keep runtime calls at the edge

Flag domain code that imports a module calling Effect.runPromise, directly or through a helper.

archguard.json:

```json
{
  "schemaVersion": 1,
  "include": ["src/**/*.ts"],
  "rules": [
    {
      "id": "domain-no-run",
      "kind": "forbiddenCall",
      "files": ["src/domain/**"],
      "origins": ["effect/Effect#runPromise"],
      "transitive": true
    }
  ]
}
```

src/domain/orders.ts:

```typescript
import '../runner';
```

src/runner.ts:

```typescript
import * as E from 'effect/Effect';
E.runPromise(task);
```

Archguard reports:

```text
src/domain/orders.ts:1:1: domain-no-run: reachable dependency src/runner.ts calls effect/Effect#runPromise
  src/domain/orders.ts -> src/runner.ts -> src/runner.ts:36
```

Fix · src/domain/orders.ts:

```typescript
export const order = 1;
```

Exit 1 with the finding above. After the fix, exit 0.

Reference: https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md
