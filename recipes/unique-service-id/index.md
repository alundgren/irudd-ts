# Catch duplicate Effect service identifiers

Give each recognized service its own literal identifier.

Category: Keep file conventions
Capabilities: uniqueServiceId

You need: Built Archguard CLI on your PATH.

Diagram: Orders and Payments use the same literal service identifier, producing duplicate-ID findings.

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts"
  ],
  "rules": [
    {
      "id": "unique-service-id",
      "kind": "uniqueServiceId",
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

## src/payments.ts · before

```typescript
import * as C from 'effect/Context';
export class Payments extends C.Service<Payments, {}>()('app/Orders') {}
```

## src/payments.ts · correction

```typescript
import * as C from 'effect/Context';
export class Payments extends C.Service<Payments, {}>()('app/Payments') {}
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding unique-service-id. After the correction: exit 0.

## Keep in mind

Computed identifiers and arbitrary service factories are outside the recognized syntax.

## Reference and source

- [docs/guides/rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md)

## Related recipes

- [service-namespace](https://alundgren.github.io/irudd-ts/recipes/service-namespace/index.md)
