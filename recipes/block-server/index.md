# Keep server code out of the client

Catch direct imports and indirect paths through shared modules.

Category: Control dependencies
Capabilities: forbiddenDependency

You need: Built Archguard CLI on your PATH.

Diagram: client/main.ts → shared/data.ts → server/db.ts · blocked

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts"
  ],
  "rules": [
    {
      "id": "client-server",
      "kind": "forbiddenDependency",
      "files": [
        "src/client/**"
      ],
      "targets": [
        "src/server/**"
      ],
      "transitive": true,
      "includeTypes": false
    }
  ]
}
```

## src/client/main.ts · before

```typescript
import '../shared/data';
```

## src/shared/data.ts · before

```typescript
import '../server/db';
```

## src/server/db.ts · before

```typescript
export const db = {};
```

## src/client/main.ts · correction

```typescript
export {};
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding client-server. After the correction: exit 0.

## Keep in mind

This checks resolved module reachability. includeTypes: false ignores erased type imports. It does not prove code executes.

## Reference and source

- [docs/guides/rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md)

## Related recipes

- [block-imports](https://alundgren.github.io/irudd-ts/recipes/block-imports/index.md)
- [role-dependencies](https://alundgren.github.io/irudd-ts/recipes/role-dependencies/index.md)
