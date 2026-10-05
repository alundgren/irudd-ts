# Require an Effect service layer export

Require one visible value export called layer on service files.

Category: Keep file conventions
Capabilities: serviceLayer

You need: Built Archguard CLI on your PATH.

Diagram: Recognized service → layer value export · required

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts"
  ],
  "rules": [
    {
      "id": "service-layer",
      "kind": "serviceLayer",
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
```

## src/orders.ts · correction

```typescript
import * as C from 'effect/Context';
export class Orders extends C.Service<Orders, {}>()('app/Orders') {}
export const layer = 1;
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding service-layer. After the correction: exit 0.

## Keep in mind

The placeholder value demonstrates export presence only. This rule does not prove that layer constructs the service.

## Reference and source

- [docs/guides/rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md)

## Related recipes

- [service-namespace](https://alundgren.github.io/irudd-ts/recipes/service-namespace/index.md)
- [companions](https://alundgren.github.io/irudd-ts/recipes/companions/index.md)
