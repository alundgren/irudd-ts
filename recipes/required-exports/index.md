# Require a named export

Make every adapter expose a create function.

Category: Keep file conventions
Capabilities: requiredExport

You need: Built Archguard CLI on your PATH.

Diagram: mail.ts → create export · required

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts"
  ],
  "rules": [
    {
      "id": "adapter-create",
      "kind": "requiredExport",
      "files": [
        "src/adapters/*.ts"
      ],
      "names": [
        "create"
      ]
    }
  ]
}
```

## src/adapters/mail.ts · before

```typescript
export const send = () => {};
```

## src/adapters/mail.ts · correction

```typescript
export const create = () => ({});
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding adapter-create. After the correction: exit 0.

## Keep in mind

Each name needs one visible export origin. This checks presence, not the function signature; external star exports are not enumerated.

## Reference and source

- [docs/guides/rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md)

## Related recipes

- [required-sources](https://alundgren.github.io/irudd-ts/recipes/required-sources/index.md)
- [service-layer](https://alundgren.github.io/irudd-ts/recipes/service-layer/index.md)
