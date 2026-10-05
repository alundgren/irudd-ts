# Enforce a team import convention

Require namespace imports for Effect service modules.

archguard.json:

```json
{
  "schemaVersion": 1,
  "include": ["src/**/*.ts"],
  "rules": [
    {
      "id": "service-namespace",
      "kind": "serviceNamespace",
      "files": ["src/**"]
    }
  ]
}
```

src/main.ts:

```typescript
import { Orders, layer } from './orders';
```

Archguard reports:

```text
src/main.ts:1:1: service-namespace: import service module src/orders.ts as a namespace instead of Orders
  src/orders.ts
src/main.ts:1:1: service-namespace: import service module src/orders.ts as a namespace instead of layer
  src/orders.ts
```

Fix · src/main.ts:

```typescript
import * as Orders from './orders';
```

Exit 1 with the finding above. After the fix, exit 0.

Reference: https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md
