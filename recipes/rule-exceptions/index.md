# Limit a rule and allow one explicit exception

Block filesystem imports in application source while keeping one legacy adapter out of scope.

Category: Control dependencies
Capabilities: forbiddenImport

You need: Built Archguard CLI on your PATH.

Diagram: Selected source → Apply rule scope → Explicit exception

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts",
    "tools/**/*.ts"
  ],
  "rules": [
    {
      "id": "no-fs",
      "kind": "forbiddenImport",
      "files": [
        "src/**"
      ],
      "exceptions": [
        "src/legacy.ts"
      ],
      "specifiers": [
        "node:fs"
      ]
    }
  ]
}
```

## src/main.ts · before

```typescript
import 'node:fs';
```

## src/legacy.ts · before

```typescript
import 'node:fs';
```

## src/main.ts · correction

```typescript
export {};
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding no-fs. After the correction: exit 0.

## Keep in mind

Rule exceptions filter policy scope; top-level exclude changes discovery. requiredFile always checks every configured files pattern.

## Reference and source

- [docs/guides/rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md)

## Related recipes

- [block-imports](https://alundgren.github.io/irudd-ts/recipes/block-imports/index.md)
- [select-sources](https://alundgren.github.io/irudd-ts/recipes/select-sources/index.md)
