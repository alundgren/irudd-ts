# Test command protocol

A mutation result needs evidence from the test runner. Exit status alone cannot distinguish an assertion failure from an import error or a failed teardown. An explicitly configured trusted command reads one request and writes one structured result. The TypeScript helpers in [sdk/mutator.ts](../sdk/mutator.ts) use Node builtins and require Node 24. They add no package installation.

The runner assigns absolute paths through `ARCHGUARD_MUTATION_REQUEST` and `ARCHGUARD_MUTATION_RESULT`. `readMutationRequest()` checks the request version, fields, task identity, SHA256 values, UTF-8 encoding, regular file, byte budget and assigned result path. It rejects symbolic links and repeated JSON fields. The request contains `requestId`, `runId`, `inputDigest`, `phase`, `mutationId`, `resultPath` and `maxResultBytes`.

Use `writeMutationResult(request, result)` after the complete test command finishes. It validates and encodes bounded plain data, writes a private temporary file, syncs it and renames it over the assigned result. It rejects unsafe counts, mismatched task identity, unknown fields, accessors and custom serialization methods. Encoding also reserves space for JSON escaping. A result that exceeds its budget cannot replace a previous valid result.

```ts
import { readMutationRequest, writeMutationResult } from "./sdk/mutator.ts";

const request = readMutationRequest();
// Run the explicitly configured tests and collect structured runner events.
writeMutationResult(request, {
  schemaVersion: 1,
  requestId: request.requestId,
  runId: request.runId,
  inputDigest: request.inputDigest,
  complete: true,
  exitCode: 0,
  reason: "finished",
  tests: { passed: 1, failed: 0, skipped: 0 },
  failures: [],
});
```

This example illustrates the result contract. A real adapter must derive counts and failures from executed tests. It must never turn arbitrary nonzero status into an assertion failure. Each assertion failure requires an executed test ID. Distinct failed test IDs must agree with `tests.failed`. Failure kinds are `assertion`, `runtime`, `import`, `hook`, `suite` and `unhandled`. A command with no executed tests provides no evidence that a mutant was tested.

## Vitest

[sdk/mutator-vitest-reporter.ts](../sdk/mutator-vitest-reporter.ts) is a one-shot reporter for an already installed Vitest environment. Add its path to the explicit test command's reporter configuration. It does not import a Vitest package or install one. The actual framework checks use T3 Code's pinned Vite Plus 1.0.0 and Vitest 5.0.1.

The reporter records test IDs, counts, assertion metadata, import and runtime errors, failed hooks and unhandled errors. It waits for the full test-run promise and runner close before reporting completion. Coverage and metadata writes can fail after `onTestRunEnd`. Vitest can also log teardown errors without retaining them in its unhandled-error state. The reporter observes these lifecycle outcomes and retains separate runtime evidence.

It requires the public `waitForTestRunEnd`, `close`, runner error state and logger methods. Hook status uses the runner task exposed by Vitest's reported entities. Missing or replaced methods, unavailable hook metadata, interruption, pending work, retried tests with retained errors and exceeded event budgets produce incomplete evidence. Watch mode, repeated runs and merged report replay are unsupported. Use a fresh process for each test command.

The adapter is fail closed. An assertion failure accompanied by a coverage, teardown, import or hook error must produce an execution error in the mutation runner. Such a failure is not evidence that tests killed the mutant. A timeout also remains a timeout.

## Development checks

Run the protocol and lifecycle controls from the repository root:

```sh
node --test tests/mutator_sdk.test.ts tests/mutator_reporter.test.ts
```

To check actual framework behavior using an explicitly supplied T3 checkout with its existing dependencies installed:

```sh
node scripts/check_mutator_vitest.mjs /absolute/path/to/t3code
```

The script creates owned temporary fixtures and removes them afterward. It covers passing and assertion-only runs, assertion plus teardown, hook and runtime failures, import errors, late coverage and metadata errors, forced exit, replaced or missing lifecycle methods, and a corrected teardown. It installs nothing and changes no source in the supplied checkout. These controls are separate from measurement trials.

The lifecycle APIs are documented in [Vitest's reporter guide](https://vitest.dev/api/advanced/reporters) and [Vitest's advanced API](https://vitest.dev/api/advanced/vitest#waitfortestrunend). Compatibility depends on the actual installed runner behavior; method names alone do not establish support for another release.
