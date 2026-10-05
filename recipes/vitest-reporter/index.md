# Connect an existing Vitest test command

Copy the SDK reporter into your project and declare the test command and all worker inputs.

Category: Review code and tests
Capabilities: Vitest reporter, mutation protocol, workspace dependencies

You need: An installed compatible Vitest project, Node 24, Linux or macOS, and all test inputs declared in workspace. Adjust paths to your project.

Diagram: Declared workspace → Vitest + reporter → Structured test evidence

## Copy from the Archguard checkout into your project

```shell
cp -R /path/to/irudd-ts/sdk /path/to/project/sdk
```

## Your project · mutator.json

```json
{
  "schemaVersion": 1,
  "plan": {
    "schemaVersion": 1,
    "selection": {
      "include": [
        "src/**/*.ts"
      ]
    }
  },
  "execution": {
    "command": [
      "/absolute/path/to/node",
      "node_modules/vitest/vitest.mjs",
      "run",
      "--reporter=./sdk/mutator-vitest-reporter.ts"
    ],
    "workspace": {
      "include": [
        "src/**",
        "tests/**",
        "sdk/**",
        "package.json",
        "package-lock.json",
        "vitest.config.ts"
      ],
      "exclude": [
        "**/node_modules/**"
      ],
      "dependencies": [
        {
          "source": "./node_modules",
          "destination": "node_modules"
        }
      ]
    },
    "limits": {
      "workers": 1,
      "maxMutants": 20
    }
  }
}
```

## Your project · replace the absolute Node path above

```shell
archguard mutator run --root . --config mutator.json --json
```

## What to expect

The reporter records executed tests and separates assertion failures from import, hook, and teardown errors. Only valid assertion evidence can kill an edit.

## Keep in mind

Use a compatible already installed one-shot runner. The adapter was checked with Vitest 5.0.1. It does not install or discover tests, and watch mode is unsupported.

## Reference and source

- [docs/mutator-test-protocol.md](https://github.com/alundgren/irudd-ts/blob/main/docs/mutator-test-protocol.md)
- [sdk/mutator-vitest-reporter.ts](https://github.com/alundgren/irudd-ts/blob/main/sdk/mutator-vitest-reporter.ts)
- [sdk/mutator.ts](https://github.com/alundgren/irudd-ts/blob/main/sdk/mutator.ts)

## Related recipes

- [run-mutations](https://alundgren.github.io/irudd-ts/recipes/run-mutations/index.md)
- [mutation-limits](https://alundgren.github.io/irudd-ts/recipes/mutation-limits/index.md)
