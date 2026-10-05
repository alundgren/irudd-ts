# Inspect mutation edits before running tests

List exact runtime edits and source hashes without executing a repository command.

Category: Review code and tests
Capabilities: mutator plan, mutation operators

You need: Built Archguard CLI on your PATH.

Diagram: Runtime expression → One exact edit → Stored plan

## Archguard checkout

```shell
./target/debug/archguard mutator plan --root . --config examples/code-quality/plan.json --json > /tmp/domain-plan.json
```

## Example edit · TypeScript

```typescript
// Original
const approved = amount > 100;
// One mutation
const approved = amount >= 100;
```

## Your project · plan.json

```json
{
  "schemaVersion": 1,
  "selection": {
    "include": [
      "src/domain/**/*.ts"
    ]
  }
}
```

## What to expect

The plan lists edits such as > becoming >=, their UTF-8 byte locations, original text, and source hashes.

## Keep in mind

Operators cover comparisons, equality, arithmetic, logic, updates, booleans, and zero/one literals. Type-only literals are excluded.

## Reference and source

- [docs/code-quality.md](https://github.com/alundgren/irudd-ts/blob/main/docs/code-quality.md)
- [examples/code-quality/README.md](https://github.com/alundgren/irudd-ts/blob/main/examples/code-quality/README.md)

## Related recipes

- [run-mutations](https://alundgren.github.io/irudd-ts/recipes/run-mutations/index.md)
- [mutation-limits](https://alundgren.github.io/irudd-ts/recipes/mutation-limits/index.md)
