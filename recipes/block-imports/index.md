# Ban an import by its written name

Keep Node builtins out of browser sources, even when they are external endpoints.

Category: Control dependencies
Capabilities: forbiddenImport

You need: Built Archguard CLI on your PATH.

Diagram: Browser source → node:fs · blocked

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts"
  ],
  "rules": [
    {
      "id": "browser-no-fs",
      "kind": "forbiddenImport",
      "files": [
        "src/client/**"
      ],
      "specifiers": [
        "node:fs",
        "node:fs/*"
      ]
    }
  ]
}
```

## src/client/main.ts · before

```typescript
import { readFileSync } from 'node:fs';
```

## src/client/main.ts · correction

```typescript
export const load = () => fetch('/settings.json');
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding browser-no-fs. After the correction: exit 0.

## Keep in mind

Specifiers match the written import string. Use forbiddenDependency for resolved file paths.

## Reference and source

- [docs/guides/rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md)

## Related recipes

- [block-server](https://alundgren.github.io/irudd-ts/recipes/block-server/index.md)
- [block-calls](https://alundgren.github.io/irudd-ts/recipes/block-calls/index.md)
