# Write a small test-command adapter

Use the Node-builtin SDK to report an assertion you actually executed.

Category: Review code and tests
Capabilities: readMutationRequest, writeMutationResult, custom test command

You need: Node 24, copied SDK, Linux or macOS, and an absolute path to your installed Node executable.

Diagram: Mutation request → Executed assertion → Validated result

## Copy the complete SDK into your project

```shell
cp -R /path/to/irudd-ts/sdk /path/to/project/sdk
```

## src/approval.ts

```typescript
export const approved = (amount: number) => amount > 100;
```

## run-tests.ts

```typescript
import { AssertionError, strictEqual } from 'node:assert';
import { approved } from './src/approval.ts';
import { readMutationRequest, writeMutationResult } from './sdk/mutator.ts';
import type { TestFailure } from './sdk/mutator.ts';

const request = readMutationRequest();
const failures: TestFailure[] = [];
try {
  strictEqual(approved(100), false);
} catch (error) {
  failures.push({
    kind: error instanceof AssertionError ? 'assertion' : 'runtime',
    testId: 'approval/threshold', file: 'src/approval.ts',
    message: String(error)
  });
}
const exitCode = failures.length ? 1 : 0;
writeMutationResult(request, {
  schemaVersion: 1, requestId: request.requestId,
  runId: request.runId, inputDigest: request.inputDigest,
  complete: true, exitCode, reason: 'finished',
  tests: { passed: failures.length ? 0 : 1, failed: failures.length, skipped: 0 },
  failures
});
process.exitCode = exitCode;
```

## mutator.json

```json
{
  "schemaVersion": 1,
  "plan": {
    "schemaVersion": 1,
    "selection": {
      "include": [
        "src/**/*.ts"
      ]
    },
    "operators": [
      "comparison"
    ]
  },
  "execution": {
    "command": [
      "/absolute/path/to/node",
      "run-tests.ts"
    ],
    "workspace": {
      "include": [
        "src/**",
        "run-tests.ts",
        "sdk/**"
      ]
    },
    "limits": {
      "workers": 1,
      "maxMutants": 10
    }
  }
}
```

## In your project · replace the absolute Node path above

```shell
archguard mutator run --root . --config mutator.json --json
```

## What to expect

The fresh baseline passes. The amount >= 100 mutation fails the exact boundary assertion and receives a killed result.

## Keep in mind

Only AssertionError becomes assertion evidence. Other errors remain runtime failures. A nonzero process status by itself cannot kill a mutation.

## Reference and source

- [docs/mutator-test-protocol.md](https://github.com/alundgren/irudd-ts/blob/main/docs/mutator-test-protocol.md)
- [sdk/mutator.ts](https://github.com/alundgren/irudd-ts/blob/main/sdk/mutator.ts)

## Related recipes

- [run-mutations](https://alundgren.github.io/irudd-ts/recipes/run-mutations/index.md)
- [vitest-reporter](https://alundgren.github.io/irudd-ts/recipes/vitest-reporter/index.md)
