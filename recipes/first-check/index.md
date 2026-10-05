# Run your first architecture check

Build Archguard and stop a client module from importing server code.

Category: Start here
Capabilities: check, installation

You need: Rust 1.96+ to build. No Node installation needed for graph checks.

Diagram: Source files → Your rules → Findings + exit code

## In the Archguard checkout · Rust 1.96+

```shell
cargo build --release --locked
export PATH="$PWD/target/release:$PATH"
```

## In your project · archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts",
    "src/**/*.tsx"
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
      "transitive": true
    }
  ]
}
```

## src/server/db.ts

```typescript
export const db = {};
```

## src/client/main.ts

```typescript
import { db } from '../server/db';
console.log(db);
```

## src/client/main.ts · correction

```typescript
export {};
```

## In your project

```shell
archguard check --root . --config archguard.json
archguard check --root . --config archguard.json --json
```

## What to expect

The client import below produces exit 1. Remove that import for exit 0.

## Keep in mind

Exit 2 means analysis did not complete or configuration is invalid. Source resolution does not check compiler types.

## Reference and source

- [README.md](https://github.com/alundgren/irudd-ts/blob/main/README.md)
- [docs/guides/configuration.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/configuration.md)

## Related recipes

- [block-server](https://alundgren.github.io/irudd-ts/recipes/block-server/index.md)
- [read-results](https://alundgren.github.io/irudd-ts/recipes/read-results/index.md)
