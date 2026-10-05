# Try the T3 Code adoption profiles

Start with a concrete architecture profile from a real application.

Category: Start here
Capabilities: adoption profiles

You need: Built Archguard CLI on your PATH.

Diagram: Existing conventions → Candidate profile → Findings to review

## Archguard checkout · supply an existing T3 Code checkout

```shell
./target/debug/archguard check --root /absolute/path/to/t3code --config examples/t3code/profiles/t3code.json --json
```

## What to expect

The profile checks selected T3 Code architecture boundaries. Review its paths and documented scope before adapting it to your project.

## Keep in mind

The external checkout must match the profile. A clean selected profile does not establish whole-repository coverage or historical benchmark performance.

## Reference and source

- [examples/t3code/README.md](https://github.com/alundgren/irudd-ts/blob/main/examples/t3code/README.md)
- [examples/t3code/rules.md](https://github.com/alundgren/irudd-ts/blob/main/examples/t3code/rules.md)
- [examples/t3code/profiles/t3code.json](https://github.com/alundgren/irudd-ts/blob/main/examples/t3code/profiles/t3code.json)

## Related recipes

- [first-check](https://alundgren.github.io/irudd-ts/recipes/first-check/index.md)
- [agent-evaluation](https://alundgren.github.io/irudd-ts/recipes/agent-evaluation/index.md)
