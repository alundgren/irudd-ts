# Check an inferred public member

Opt into the pinned compiler provider and catch account.missing on an inferred object.

Category: Add your own checks
Capabilities: missingMember, semantic-config

You need: Archguard checkout, Node 24, optional provider install, and Unix. Restore main.ts after trying the change.

Diagram: account.missing → Compiler member facts → Missing member

## Archguard checkout · Node 24

```shell
npm ci --prefix providers/typescript7 --ignore-scripts --no-audit --no-fund
cargo build --locked --bins --examples
./target/debug/archguard check --root examples/semantic --config examples/semantic/archguard.json --semantic-config examples/semantic/semantic.json --json
```

## examples/semantic/main.ts · try a missing member

```typescript
const account = { name: 'Ada' };
account.missing;
```

## examples/semantic/semantic.json

```json
{
  "schemaVersion": 1,
  "provider": {
    "name": "typescript7",
    "command": [
      "node",
      "../../providers/typescript7/provider.mjs"
    ],
    "timeoutMs": 30000
  },
  "contexts": [
    {
      "id": "app",
      "tsconfig": "tsconfig.json",
      "files": [
        "main.ts"
      ]
    }
  ],
  "rules": [
    {
      "id": "missing-member",
      "kind": "missingMember",
      "files": [
        "**"
      ]
    }
  ]
}
```

## What to expect

account.name passes. account.missing yields a member finding and compiler error, exit 2. With @ts-ignore suppressing that error, the rule can still yield exit 1.

## Keep in mind

The provider pins TypeScript 7.0.2 and its experimental API. Unknown, any, error, and unavailable states do not establish a clean result.

## Reference and source

- [examples/semantic/README.md](https://github.com/alundgren/irudd-ts/blob/main/examples/semantic/README.md)
- [docs/extensions/semantic-provider.md](https://github.com/alundgren/irudd-ts/blob/main/docs/extensions/semantic-provider.md)

## Related recipes

- [compiler-facts](https://alundgren.github.io/irudd-ts/recipes/compiler-facts/index.md)
- [read-results](https://alundgren.github.io/irudd-ts/recipes/read-results/index.md)
