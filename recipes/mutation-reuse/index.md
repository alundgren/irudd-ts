# Reuse complete mutation results

Opt into conservative result reuse when every relevant test input is declared and deterministic.

Category: Review code and tests
Capabilities: declaredInputs, state, externalInputs

You need: A complete mutation configuration. Replace externalInputs with actual required files, or use an empty list.

Diagram: Hash declared inputs → Always fresh baseline → Reuse complete results

## mutator.json · add execution.state

```json
{
  "directory": "/tmp/project-mutation-state",
  "reuse": "declaredInputs",
  "externalInputs": [
    "/absolute/path/to/test-data.json"
  ]
}
```

## Run twice with the same complete configuration

```shell
archguard mutator run --root . --config mutator.json --json
archguard mutator run --root . --config mutator.json --json
```

## What to expect

Unchanged declared inputs can reuse complete killed and survived results. Changing tests, helpers, dependencies, or other declared inputs invalidates reuse.

## Keep in mind

Keep state outside source and dependency inputs. Leave reuse off for network, clock, or undeclared host dependencies. Cleanup uncertainty prevents reuse.

## Reference and source

- [docs/code-quality.md](https://github.com/alundgren/irudd-ts/blob/main/docs/code-quality.md)
- [src/mutator/state.rs](https://github.com/alundgren/irudd-ts/blob/main/src/mutator/state.rs)

## Related recipes

- [run-mutations](https://alundgren.github.io/irudd-ts/recipes/run-mutations/index.md)
- [mutation-limits](https://alundgren.github.io/irudd-ts/recipes/mutation-limits/index.md)
