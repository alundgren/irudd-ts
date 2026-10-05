# Require a matching companion file

Require src/layers/Orders.ts whenever src/services/Orders.ts is analyzed.

Category: Keep file conventions
Capabilities: repository.companion

You need: Built Archguard CLI on your PATH.

Diagram: services/Orders.ts → layers/Orders.ts · required

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
          "src/services/*.ts"
        ],
        "exclude": [
          "**/*.test.ts"
        ]
      }
    ],
    "rules": [
      {
        "id": "service-companion",
        "kind": "companion",
        "role": "service",
        "replace": [
          "/services/",
          "/layers/"
        ]
      }
    ]
  }
}
```

## src/services/Orders.ts · before

```typescript
export class Orders {}
```

## src/layers/Orders.ts · correction

```typescript
export const live = 1;
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding service-companion. After the correction: exit 0.

## Keep in mind

The replacement string must occur exactly once. The companion must be analyzed; presence does not prove it implements the service.

## Reference and source

- [docs/guides/repository-rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/repository-rules.md)

## Related recipes

- [classify-files](https://alundgren.github.io/irudd-ts/recipes/classify-files/index.md)
- [registry-imports](https://alundgren.github.io/irudd-ts/recipes/registry-imports/index.md)
