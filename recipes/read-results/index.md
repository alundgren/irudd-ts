# Read clean, failing, and incomplete results

Use completeness and the process exit code to decide the next action.

Category: Start here
Capabilities: exitCodes, json

You need: Built Archguard CLI on your PATH.

Diagram: Alternative outcomes: 0 is clean, 1 has policy violations, 2 is incomplete or invalid.

## Capture a check report

```shell
archguard check --root . --config archguard.json --json > archguard-report.json
```

## Inspect completeness and findings with Node 24

```javascript
import { readFileSync } from 'node:fs';
const report = JSON.parse(readFileSync('archguard-report.json', 'utf8'));
console.log(report.complete, report.diagnostics, report.problems);
```

## What to expect

0: all checks completed and passed. 1: all checks completed with violations. 2: fix config, parsing, resolution, or required facts.

## Keep in mind

An incomplete report can still contain useful findings. Dryer and mutation runs exit 0 for complete evidence, including matches or survivors.

## Reference and source

- [docs/guides/configuration.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/configuration.md)
- [docs/code-quality.md](https://github.com/alundgren/irudd-ts/blob/main/docs/code-quality.md)

## Related recipes

- [first-check](https://alundgren.github.io/irudd-ts/recipes/first-check/index.md)
- [plan-mutations](https://alundgren.github.io/irudd-ts/recipes/plan-mutations/index.md)
