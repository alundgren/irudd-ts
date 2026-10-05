# Use namespace imports for Effect services

Keep a service and its layer under one module name at call sites.

Category: Keep file conventions
Capabilities: serviceNamespace

You need: Built Archguard CLI on your PATH.

Diagram: Consumer → import * as Orders → Service module

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts"
  ],
  "rules": [
    {
      "id": "service-namespace",
      "kind": "serviceNamespace",
      "files": [
        "src/**"
      ]
    }
  ]
}
```

## src/orders.ts · before

```typescript
import * as C from 'effect/Context';
export class Orders extends C.Service<Orders, {}>()('app/Orders') {}
export const layer = 1;
```

## src/main.ts · before

```typescript
import { Orders, layer } from './orders';
```

## src/main.ts · correction

```typescript
import * as Orders from './orders';
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding service-namespace. After the correction: exit 0.

## Keep in mind

Only recognized Effect service syntax and value consumers are checked. This is an import convention.

## Reference and source

- [docs/guides/rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md)

## Related recipes

- [unique-service-id](https://alundgren.github.io/irudd-ts/recipes/unique-service-id/index.md)
- [service-layer](https://alundgren.github.io/irudd-ts/recipes/service-layer/index.md)
