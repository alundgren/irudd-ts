# Bound a mutation run

Set a small mutation budget, worker count, deadline, and workspace ceiling.

Category: Review code and tests
Capabilities: execution limits, cancellation

You need: Start with a complete mutation configuration from the run or Vitest recipe. This is a limits fragment.

Diagram: Declared inputs → Bounded workers → Report or explicit limit

## mutator.json · merge into execution.limits

```json
{
  "workers": 1,
  "maxMutants": 20,
  "commandTimeoutMs": 10000,
  "runTimeoutMs": 120000,
  "maxWorkspaceFiles": 10000,
  "maxWorkspaceFileBytes": 10485760,
  "maxWorkspaceBytes": 268435456,
  "maxTotalWorkspaceBytes": 536870912,
  "maxCpuSeconds": 10,
  "maxGeneratedFileBytes": 10485760
}
```

## Run after applying limits

```shell
archguard mutator run --root . --config mutator.json --json
```

## What to expect

Limits bound trusted command execution. Reaching an analysis or execution limit remains visible in completeness and problems.

## Keep in mind

Worker copies and limits are not an arbitrary-code sandbox. Address-space limits are Linux-only. SIGINT and SIGTERM request bounded cleanup.

## Reference and source

- [docs/code-quality.md](https://github.com/alundgren/irudd-ts/blob/main/docs/code-quality.md)
- [src/mutator/config.rs](https://github.com/alundgren/irudd-ts/blob/main/src/mutator/config.rs)

## Related recipes

- [vitest-reporter](https://alundgren.github.io/irudd-ts/recipes/vitest-reporter/index.md)
- [mutation-reuse](https://alundgren.github.io/irudd-ts/recipes/mutation-reuse/index.md)
