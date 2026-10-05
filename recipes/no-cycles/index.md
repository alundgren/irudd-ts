# Find dependency cycles

Catch two modules that import each other before the cycle grows.

Category: Control dependencies
Capabilities: noCycles

You need: Built Archguard CLI on your PATH.

Diagram: a.ts → b.ts → b.ts → a.ts · cycle

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts"
  ],
  "rules": [
    {
      "id": "no-cycles",
      "kind": "noCycles",
      "files": [
        "src/**"
      ],
      "includeTypes": false
    }
  ]
}
```

## src/a.ts · before

```typescript
import './b';
export const a = 1;
```

## src/b.ts · before

```typescript
import './a';
export const b = 2;
```

## src/b.ts · correction

```typescript
export const b = 2;
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding no-cycles. After the correction: exit 0.

## Keep in mind

noCycles omits literal dynamic-import edges. includeTypes: false also omits type-only edges.

## Reference and source

- [docs/guides/rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md)

## Related recipes

- [block-server](https://alundgren.github.io/irudd-ts/recipes/block-server/index.md)
