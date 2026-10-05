# Keep runtime calls at the application edge

Report modules that call Effect.runPromise, and modules that depend on them.

Category: Control dependencies
Capabilities: forbiddenCall

You need: Built Archguard CLI on your PATH.

Diagram: domain/orders.ts → runner.ts → runPromise · blocked

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts"
  ],
  "rules": [
    {
      "id": "domain-no-run",
      "kind": "forbiddenCall",
      "files": [
        "src/domain/**"
      ],
      "origins": [
        "effect/Effect#runPromise"
      ],
      "transitive": true
    }
  ]
}
```

## src/domain/orders.ts · before

```typescript
import '../runner';
```

## src/runner.ts · before

```typescript
import * as E from 'effect/Effect';
E.runPromise(task);
```

## src/domain/orders.ts · correction

```typescript
export const order = 1;
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding domain-no-run. After the correction: exit 0.

## Keep in mind

Imported aliases are recognized; locally shadowed names are not attributed to the import. A dependency path does not prove a callback runs.

## Reference and source

- [docs/guides/rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md)

## Related recipes

- [block-server](https://alundgren.github.io/irudd-ts/recipes/block-server/index.md)
