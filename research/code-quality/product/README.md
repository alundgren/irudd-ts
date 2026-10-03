# Product dogfooding reproductions

Read the [product findings](FINDINGS.md) alongside these commands. The [initial research](../findings.md) retains the reference-tool and mature-tool comparisons.

Run these separately from builds and validation when retaining runtime or resource evidence. Keep output under ignored `research/local/` until its source, configuration, executable and environment provenance is recorded.

Replay the six archived duplicate fixtures against the current product:

```sh
node research/code-quality/product/replay-dryer.mjs /absolute/archguard-binary /absolute/archguard /tmp/archguard-dryer-product-cases
```

This retains every comparison at threshold zero and compares default normalization, erased properties, erased local identifiers and preserved literals. It records raw output, configuration, source and executable hashes, and verifies source preservation. The fixtures are syntax-only examples with undefined domain types and calls; they are not executed or compiler-checked. Inspect the scores alongside intended duplication and semantic differences.

For the TypeScript protocol SDK and reporter, supply the current repository and an explicit self-contained Vite Plus installation:

```sh
node research/code-quality/product/prepare-self.mjs /absolute/archguard /absolute/node_modules /tmp/archguard-self-profiles
archguard dryer --root /absolute/archguard --config /tmp/archguard-self-profiles/dryer.json --json
archguard mutator run --root /absolute/archguard --config /tmp/archguard-self-profiles/protocol.json --json
archguard mutator run --root /absolute/archguard --config /tmp/archguard-self-profiles/reporter.json --json
```

This is a labeled test-runner adaptation. The existing Node SDK and reporter tests register with Vitest instead of `node:test`; their assertions and SDK imports remain unchanged. A fixed reporter/protocol copy handles instrumentation independently of mutations in the SDK under test. An authored private ESM package manifest satisfies Vite Plus's workspace-root requirement. All copies are explicit dependency inputs. The generator installs nothing and changes no repository source. This tests TypeScript SDK behavior, not the Rust implementation.

Inspect every surviving mutation and every execution error. A mutated API can throw an ordinary error inside a test. The conservative protocol records that as a runtime error rather than automatically treating it as an assertion kill. Missing result metadata and failed setup remain incomplete evidence. Retain those outcomes alongside useful survivors.

The authored configuration allows 15 seconds per test. Protocol commands have a 30-second deadline and a one-hour run limit. Reporter commands have 60 seconds and a two-hour run limit because each lifecycle test executes several isolated Node controls. These are explicit experiment bounds. Adjust them for the selected tests and retain timeout outcomes alongside assertion failures.

The authored domain and unchanged focused T3 test profiles live in [examples/code-quality](../../../examples/code-quality/README.md). T3 scope is limited to the configured files and test commands. It does not establish whole-repository coverage.

For the focused path profile, retain its source-only plan beside the run report and inspect concrete survivor counterexamples:

```sh
node research/code-quality/product/inspect-path.mjs /absolute/trusted-t3code /absolute/path-plan.json /absolute/path-report.json /tmp/archguard-path-counterexamples
```

This explicitly loads copied T3 path modules with Node, applies only the observed logical/zero-one edits, checks the original source hash, and retains upstream licensing beside the copies. It writes outside T3. The examples distinguish missing relative-path cases and one-character trailing separators from equivalent early returns and unresolved drive-root separator representation. Observing no difference on a few inputs alone does not prove equivalence; inspect the exported behavior and its callers.

Counterexamples use the mutation owner's function name, so source comments can move lines without changing which exported behavior is tested. Unknown owners and invalid mutation IDs fail before creating output. A focused correction control is available with `node --test research/code-quality/product/inspect-path.test.mjs`.
