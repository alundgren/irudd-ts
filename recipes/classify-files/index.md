# Give every source file exactly one role

Catch sources that fall outside your conventions or match overlapping roles.

Category: Keep file conventions
Capabilities: repository.classified

You need: Built Archguard CLI on your PATH.

Diagram: Service and contract paths receive their roles. An unclassified file produces a finding.

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
        "id": "service",
        "files": [
          "src/services/**"
        ]
      },
      {
        "id": "contract",
        "files": [
          "src/contracts/**"
        ]
      },
      {
        "id": "test",
        "files": [
          "src/*.test.ts"
        ]
      }
    ],
    "rules": [
      {
        "id": "classify-source",
        "kind": "classified",
        "files": [
          "src/**"
        ]
      }
    ]
  }
}
```

## src/services/orders.ts · before

```typescript
export {};
```

## src/misc.ts · before

```typescript
export {};
```

## src/contracts/message.ts · correction

```typescript
export {};
```

## Correction

```shell
rm src/misc.ts
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding classify-source. After the correction: exit 0.

## Keep in mind

Roles filter selected sources. A classification rule chooses where exactly-one membership is required.

## Reference and source

- [docs/guides/repository-rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/repository-rules.md)

## Related recipes

- [companions](https://alundgren.github.io/irudd-ts/recipes/companions/index.md)
- [role-dependencies](https://alundgren.github.io/irudd-ts/recipes/role-dependencies/index.md)
