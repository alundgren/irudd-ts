# Block dependencies between file roles

Keep contracts independent of host implementation code.

Category: Keep file conventions
Capabilities: repository.forbiddenDependency

You need: Built Archguard CLI on your PATH.

Diagram: Contract role → Host role · blocked

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts"
  ],
  "repository": {
    "roles": [
      {
        "id": "contract",
        "files": [
          "src/contracts/**"
        ]
      },
      {
        "id": "host",
        "files": [
          "src/host/**"
        ]
      }
    ],
    "rules": [
      {
        "id": "contracts-no-host",
        "kind": "forbiddenDependency",
        "from": "contract",
        "to": "host",
        "transitive": true,
        "includeTypes": true
      }
    ]
  }
}
```

## src/contracts/message.ts · before

```typescript
import '../host/main';
```

## src/host/main.ts · before

```typescript
export const host = 1;
```

## src/contracts/message.ts · correction

```typescript
export type Message = { id: string };
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding contracts-no-host. After the correction: exit 0.

## Keep in mind

Role exclusions affect source and target membership. Transitive paths can pass through modules with no role.

## Reference and source

- [docs/guides/repository-rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/repository-rules.md)

## Related recipes

- [block-server](https://alundgren.github.io/irudd-ts/recipes/block-server/index.md)
- [classify-files](https://alundgren.github.io/irudd-ts/recipes/classify-files/index.md)
