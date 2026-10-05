# Compare duplicate normalization settings

Keep literal values for a stricter comparison, or erase local names when investigating shifted declarations.

Category: Review code and tests
Capabilities: normalization, dryer cache

You need: Built Archguard CLI on your PATH.

Diagram: Function source → Explicit normalization → Compare both reports

## dryer-strict.json

```json
{
  "schemaVersion": 1,
  "selection": {
    "include": [
      "src/**/*.ts"
    ]
  },
  "normalization": {
    "normalizationVersion": 1,
    "localIdentifiers": "erase",
    "properties": "preserve",
    "literals": "value"
  }
}
```

## Your project

```shell
archguard dryer --root . --config dryer.json --json > default-report.json
archguard dryer --root . --config dryer-strict.json --cache /tmp/dryer-extraction.json --json > strict-report.json
```

## What to expect

This profile preserves properties and literal values while erasing local identifier distinctions.

## Keep in mind

Erasing locals loses the difference between a + a and a + b. Repeated boilerplate can match. Inspect both reports and opaque-node counts.

## Reference and source

- [docs/code-quality.md](https://github.com/alundgren/irudd-ts/blob/main/docs/code-quality.md)
- [src/dryer/config.rs](https://github.com/alundgren/irudd-ts/blob/main/src/dryer/config.rs)

## Related recipes

- [find-duplicates](https://alundgren.github.io/irudd-ts/recipes/find-duplicates/index.md)
