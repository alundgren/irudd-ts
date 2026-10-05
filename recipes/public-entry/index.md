# Require imports through a public package entry

Stop consumers from reaching into a workspace package's private source paths.

Category: Control dependencies
Capabilities: publicEntry

You need: Built Archguard CLI on your PATH.

Diagram: A consumer can import @app/api. An import directly into private source is blocked.

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts",
    "packages/**/*.ts"
  ],
  "rules": [
    {
      "id": "api-public-entry",
      "kind": "publicEntry",
      "files": [
        "src/**"
      ],
      "targets": [
        "packages/api/src/**"
      ],
      "specifiers": [
        "@app/api"
      ]
    }
  ]
}
```

## packages/api/package.json · before

```json
{"name":"@app/api","exports":{".":"./src/index.ts"}}
```

## packages/api/src/index.ts · before

```typescript
export const api = 1;
```

## src/main.ts · before

```typescript
import '../packages/api/src/index';
```

## src/main.ts · correction

```typescript
import '@app/api';
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding api-public-entry. After the correction: exit 0.

## Keep in mind

Include protected package sources in discovery. The allowed list checks the written specifier for imports that resolve into those sources.

## Reference and source

- [docs/guides/rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md)

## Related recipes

- [resolve-imports](https://alundgren.github.io/irudd-ts/recipes/resolve-imports/index.md)
- [required-exports](https://alundgren.github.io/irudd-ts/recipes/required-exports/index.md)
