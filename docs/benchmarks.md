# Historical reductions and local benchmarks

The fixtures test two architecture policies against four upstream PRs. Three PRs adopted the same Effect service namespace convention. They are not three independently discovered runtime bugs. The fourth removed a type-only import cycle. These are reduced syntax and dependency reproductions, not full builds or replays of T3 Code.

[historical-evidence.json](historical-evidence.json) records exact before and fixed commits, upstream paths, checked source text, line numbers and SHA-256 hashes of the original files. Copied snippets retain [T3 Code's MIT notice](../benchmarks/history/LICENSE.t3code). Verify the source evidence against a local upstream clone with:

```sh
python3 benchmarks/verify_history.py /path/to/t3code
cargo test --locked --test history
```

| PR | Preserved failure | Before | Fixed |
| --- | --- | --- | --- |
| [14385](https://github.com/pingdotgg/t3code/pull/14385) | `linkCreatedPullRequest.ts` imports the `OrchestratorV2` service as a named value | 1 namespace diagnostic | clean |
| [14387](https://github.com/pingdotgg/t3code/pull/14387) | `AcpRegistryOrchestratorV2.live.test.ts` imports that service as a named value | 1 namespace diagnostic | clean |
| [14389](https://github.com/pingdotgg/t3code/pull/14389) | The same named consumer import resolves through a flat service export before the barrel changes to a namespace export | 1 namespace diagnostic | clean |
| [13151](https://github.com/pingdotgg/t3code/pull/13151) | `FilePreviewModal.tsx` imports `FilePreview`, which imports the modal's type | 1 cycle diagnostic | clean |

Every fixture check must report `complete: true`. Before exits with status 1 and fixed exits with status 0. The integration test checks both the status and JSON report.

The reductions retain exact consumer import statements and source paths. Service implementations retain the `Context.Service` declaration, class name and original tag string but replace the service interface with an empty interface. Other imports and execution logic are omitted. `effect/Context` remains an external dependency. PR14389 uses a minimal workspace package manifest to resolve the original package specifier, not a copied full manifest. Its consumer import is unchanged across revisions. Only the barrel export changes, so a heuristic that rejects all named imports from a service filename would fail this fixed case.

The mobile reduction keeps both generic and iOS implementations, but its default resolution selects generic `.tsx`. That is a declared reduction, not a claim to match Metro resolution. Both upstream return edges are preserved and verified. Type-only imports count toward this cycle policy. A separate test disables type-only edges and checks a dynamic return edge, which the cycle policy excludes.

Negative controls cover namespace imports, type-only service imports, a named helper export, a regular class and prose that resembles an import. The fixed PR14389 fixture also checks a named namespace imported through a barrel.

The inspected set contains seven candidate PRs plus one policy PR. PR14183's run stuck in starting, PR14665's duplicate event storage and PR14641's nested wake ownership need runtime state or execution semantics. A syntax graph cannot establish their failures or fixes. They remain unsupported cases in the evidence file. There is no percentage claim about T3's overall bug history. PR14018 supplies quarantine rules and permitted legacy importers/readers; no before/fixed graph failure has been demonstrated for that PR, so it is policy evidence only.

## Run the benchmarks

Run these locally. There is no CI job and no build time in the measurements.

```sh
npm ci --ignore-scripts --prefix benchmarks/toolchain
cargo build --locked --release --examples
python3 scripts/benchmark.py
```

If Cargo uses another target directory, pass the actual executables:

```sh
python3 scripts/benchmark.py \
  --binary /path/to/target/release/archguard \
  --rust-plugin /path/to/target/release/examples/graph_plugin \
  --sizes 100 1000 --repetitions 7 \
  --output benchmarks/local/results.json
```

The toolchain package lock pins Oxlint 1.86.0, TypeScript 7.0.2 and a TypeScript 5.9.3 parser alias used by the independent graph implementation. TypeScript 7 has a different compiler API, so the graph rule uses the pinned stable parser separately. Node must support native TypeScript stripping for SDK plugins. The result records the actual versions, executable hashes, repository revision, platform, full commands and corpus hashes. [toolchain/licenses.json](../benchmarks/toolchain/licenses.json) records licenses for all locked packages, including optional platform packages. Oxlint is MIT; TypeScript is Apache-2.0. Dependencies are installed without lifecycle scripts and are not committed.

Each sample measures the wall time of a new complete process through captured JSON output. Trials run serially after one warmup per implementation, with a seeded randomized order and warm filesystem caches. Oxlint uses one thread. Results include every sample, median, minimum and maximum. These are process latency measurements on generated projects, not isolated parser throughput or cold-disk results.

## Equivalent work

For each size, every implementation receives exactly the same generated `.ts` files. The runner checks the file count, completion status, exit status, diagnostic count and expected violation set before it records timings, then repeats these checks on every sample. It aborts on disagreement and produces no ratio for that run.

| Workload | Implementations | Expected work |
| --- | --- | --- |
| Transitive dependency | Native built-in, TypeScript subprocess SDK, Rust executable plugin, Oxlint JavaScript plugin | Each client imports a shared module; one quarter of shared modules import their paired server module. Report the same forbidden client/server pairs. The rest are clean controls. |
| Direct import | Native `forbiddenImport`, Oxlint `no-restricted-imports`, Oxlint JavaScript plugin | Reject the literal `../server/db.ts` in every scanned file, including type imports. One quarter of clients violate the rule and the others import a shared module. |
| Static cycle | Native `noCycles`, Oxlint `import/no-cycle` | Find the same disjoint two-module type-only cycles. Remaining pairs have a one-way edge or a dynamic return edge. |

The native and executable Rust dependency rules use the same rule implementation. The TypeScript example traverses the host's graph through the SDK. The Oxlint graph plugin independently reads and parses the generated files with TypeScript 5.9.3, resolves their explicit relative `.ts` imports and traverses that graph. Its timing includes this extra parse and graph construction, plus Oxlint's own parsing. Oxlint can rebuild a graph without a project graph API. This implementation demonstrates that option. It supports the benchmark's explicit relative static imports only, and does not claim the host's workspace-package, alias or re-export provenance support.

The direct JavaScript rule tests a local literal-import convention. It does not claim Effect service definition provenance. The graph benchmark contains no dynamic edges; the dependency SDK's dynamic behavior is not compared there.

For cycles, Archguard emits one diagnostic per strongly connected component while Oxlint emits one per importing participant. The runner normalizes the actual reported import spans to unordered two-file cycle pairs and checks raw counts as well. `includeTypes: true` matches Oxlint `ignoreTypes: false`; `allowUnsafeDynamicCyclicDependency: true` permits the generated dynamic-return controls. These settings agree on this corpus. They are not a proof of equivalent handling of arbitrary mixed static/dynamic graphs, self-loops or larger components. Diagnostic rendering work differs, so cycle ratios describe equivalent detected cycle pairs with that reporting difference included.

The result has a separate process-overhead section. It compares a host with no rules to the same host plus an empty TypeScript SDK subprocess, and to the Rust graph executable on files with no selected clients. The added median includes JSON serialization, pipe IO, runtime startup, protocol decoding and shutdown. It is not just `node` startup time and is not subtracted from the workload measurements.
