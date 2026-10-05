# Require an analyzed source file

Require a contracts entry point to exist in the selected source inventory.

Category: Keep file conventions
Capabilities: requiredFile

You need: Built Archguard CLI on your PATH.

Diagram: Selected sources → contracts/index.ts · required

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts"
  ],
  "rules": [
    {
      "id": "contracts-entry",
      "kind": "requiredFile",
      "files": [
        "src/contracts/index.ts"
      ]
    }
  ]
}
```

## src/main.ts · before

```typescript
export {};
```

## src/contracts/index.ts · correction

```typescript
export type Message = { id: string };
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding contracts-entry. After the correction: exit 0.

## Keep in mind

Every configured pattern must match an analyzed source. This cannot require a README or another unsupported non-source file.

## Reference and source

- [docs/guides/rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md)

## Related recipes

- [select-sources](https://alundgren.github.io/irudd-ts/recipes/select-sources/index.md)
- [companions](https://alundgren.github.io/irudd-ts/recipes/companions/index.md)
