# Product dogfooding reproductions

Run these separately from builds and validation when retaining runtime or resource evidence. Keep output under ignored `research/local/` until its source, configuration, executable and environment provenance is recorded.

For the TypeScript protocol SDK, supply the current repository and an explicit self-contained Vite Plus installation:

```sh
node research/code-quality/product/prepare-self.mjs /absolute/archguard /absolute/node_modules /tmp/archguard-self-profiles
archguard dryer --root /absolute/archguard --config /tmp/archguard-self-profiles/dryer.json --json
archguard mutator run --root /absolute/archguard --config /tmp/archguard-self-profiles/mutator.json --json
```

This is a labeled test-runner adaptation. The existing Node SDK tests register with Vitest instead of `node:test`; their assertions and SDK imports remain unchanged. A fixed reporter/protocol copy handles instrumentation independently of mutations in the SDK under test. All copies are explicit dependency inputs. The generator installs nothing and changes no repository source. This tests TypeScript SDK behavior, not the Rust implementation.

Inspect every surviving mutation and every execution error. A mutated API can throw an ordinary error inside a test. The conservative protocol records that as a runtime error rather than automatically treating it as an assertion kill. Missing result metadata and failed setup remain incomplete evidence. Retain those outcomes alongside useful survivors.

The authored domain and unchanged focused T3 test profiles live in [examples/code-quality](../../../examples/code-quality/README.md). T3 scope is limited to the configured files and test commands. It does not establish whole-repository coverage.
