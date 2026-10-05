# Find structurally similar functions

Compare renamed invoice and order logic before deciding what to extract.

Category: Review code and tests
Capabilities: dryer, similarityThreshold

You need: Built Archguard CLI on your PATH.

Diagram: Selected functions → Normalized comparison → Pairs to inspect

## Archguard checkout

```shell
./target/debug/archguard dryer --root . --config examples/code-quality/dryer.json --json
```

## Your project · dryer.json

```json
{
  "schemaVersion": 1,
  "selection": {
    "include": [
      "src/**/*.ts",
      "src/**/*.tsx"
    ]
  },
  "minimumLines": 4,
  "minimumNodes": 20,
  "similarityThreshold": 0.82
}
```

## Your project

```shell
archguard dryer --root . --config dryer.json --json > duplicate-report.json
```

## What to expect

The authored example reports similar function pairs with locations and similarity evidence. A complete report exits 0 even with matches.

## Keep in mind

Dryer supports TypeScript and TSX. Similarity is review evidence, not proof that two functions should be combined.

## Reference and source

- [docs/code-quality.md](https://github.com/alundgren/irudd-ts/blob/main/docs/code-quality.md)
- [examples/code-quality/clones.ts](https://github.com/alundgren/irudd-ts/blob/main/examples/code-quality/clones.ts)

## Related recipes

- [tune-duplicates](https://alundgren.github.io/irudd-ts/recipes/tune-duplicates/index.md)
- [duplicate-groups](https://alundgren.github.io/irudd-ts/recipes/duplicate-groups/index.md)
