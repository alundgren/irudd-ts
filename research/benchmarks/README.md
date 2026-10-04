# Historical reductions and local benchmarks

[Persistent graph caching](../t3code/cache.md) documents the opt-in cache, verified PR13151 and PR14389 edit replays, and fresh-versus-reused measurements. Cache reuse counts do not imply a latency improvement. The independent pre-implementation reduction measurements are retained in [the uncached baseline](https://github.com/alundgren/irudd-ts/blob/194bf91ce887498bf798cff2163d51bd897aa942/research/t3code/results/cache/uncached-baseline.json).

## Repository-role iteration

[Repository structure](../t3code/structure.md) records the new structural experiment: ten deliberately invalid fixtures and their corrections, plus a complete scan of the pinned T3 server dependency closure. Those checks establish the selected structural policies, separate from the historical reductions below.

The existing equivalent-work benchmark was rerun on source revision `6e78dca`, retaining [all raw samples](https://github.com/alundgren/irudd-ts/blob/194bf91ce887498bf798cff2163d51bd897aa942/research/benchmarks/results/process-latency-repository-roles.json). At 4,000 synthetic files, graph-check medians were 119.33 ms native, 141.95 ms with a Rust subprocess, 331.06 ms with a TypeScript subprocess and 1,414.35 ms with the independent Oxlint JavaScript graph. Native graph samples ranged from 88.57 to 187.30 ms. Direct-import medians were 147.82 ms native, 253.13 ms Oxlint built-in and 706.63 ms Oxlint JavaScript. Cycle medians were 152.74 ms native and 650.07 ms Oxlint built-in. Every workload passed expected violation and negative-control equivalence checks before timing ratios were recorded.

These are the existing graph/direct/cycle workloads, not an Oxlint equivalent of the new companion or registry-import rules. Earlier measurements below remain valid evidence, including the unfavorable native cycle comparison and its large variance. No traversal algorithm changed in this iteration; differences between runs do not establish an implementation speedup. Machine load and complete process costs remain relevant. The [initial overlapping run](https://github.com/alundgren/irudd-ts/blob/194bf91ce887498bf798cff2163d51bd897aa942/research/benchmarks/results/process-latency-repository-roles-validation-overlap.json) is retained with an explicit note because local validation ran concurrently during its initial work. Use the subsequent run for this iteration's comparison.

The fixtures test two architecture policies against four upstream PRs. Three PRs adopted the same Effect service namespace convention. They are not three independently discovered runtime bugs. The fourth removed a type-only import cycle. These are reduced syntax and dependency reproductions, not full builds or replays of T3 Code.

[historical-evidence.json](../t3code/historical-evidence.json) records exact before and fixed commits, upstream paths, checked source text, line numbers and SHA-256 hashes of the original files. Copied snippets retain [T3 Code's MIT notice](../t3code/history/LICENSE.t3code). Verify the source evidence against a local upstream clone with:

```sh
python3 research/t3code/scripts/verify_history.py /path/to/t3code
cargo test --locked --features research-tests --test t3_history
```

| PR | Preserved failure | Before | Fixed |
| --- | --- | --- | --- |
| [14385](https://github.com/pingdotgg/t3code/pull/14385) | `linkCreatedPullRequest.ts` imports the `OrchestratorV2` service as a named value | 1 namespace diagnostic | clean |
| [14387](https://github.com/pingdotgg/t3code/pull/14387) | `AcpRegistryOrchestratorV2.live.test.ts` imports that service as a named value | 1 namespace diagnostic | clean |
| [14389](https://github.com/pingdotgg/t3code/pull/14389) | The same named consumer import resolves through a flat service export before the barrel changes to a namespace export | 1 namespace diagnostic | clean |
| [13151](https://github.com/pingdotgg/t3code/pull/13151) | `FilePreviewModal.tsx` imports `FilePreview`, which imports the modal's type | 1 cycle diagnostic | clean |

Every fixture check must report `complete: true`. Before exits with status 1 and fixed exits with status 0. The integration test checks both the status and JSON report.

The reductions retain exact consumer import statements and source paths. Service implementations retain the `Context.Service` declaration, class name and original tag string but replace the service interface with an empty interface. Other imports and execution logic are omitted. `effect/Context` remains an external dependency. PR14389 uses a minimal workspace package manifest to resolve the original package specifier, not a copied full manifest. Its consumer import is unchanged across revisions. Only the barrel export changes, so a heuristic that rejects all named imports from a service filename would fail this fixed case.

The mobile reduction keeps generic and iOS implementations. The default config selects generic `.tsx`; separate Android and iOS configs use the exact upstream extension candidate orders and omit the opposite platform. Tests assert that the modal resolves to the expected implementation, that its cycle contains the matching platform file and that both fixed variants are clean. Both upstream return edges are preserved and verified. These are source-resolution policies, not a claim of general Metro runtime parity. Type-only imports count toward this cycle policy. A separate test disables type-only edges and checks a dynamic return edge, which the cycle policy excludes.

Negative controls cover namespace imports, type-only service imports, a named helper export, a regular class and prose that resembles an import. The fixed PR14389 fixture also checks a named namespace imported through a barrel.

The inspected set contains seven candidate PRs plus one policy PR. PR14183's run stuck in starting, PR14665's duplicate event storage and PR14641's nested wake ownership need runtime state or execution semantics. A syntax graph cannot establish their failures or fixes. They remain unsupported cases in the evidence file. There is no percentage claim about T3's overall bug history. PR14018 supplies quarantine rules and permitted legacy importers/readers; no before/fixed graph failure has been demonstrated for that PR, so it is policy evidence only.

## Run the benchmarks

Run these locally. There is no CI job and no build time in the measurements.

```sh
npm ci --ignore-scripts --prefix research/benchmarks/toolchain
cargo build --locked --release --bins --examples
python3 research/benchmarks/scripts/benchmark.py
```

If Cargo uses another target directory, pass the actual executables:

```sh
python3 research/benchmarks/scripts/benchmark.py \
  --binary /path/to/target/release/archguard \
  --rust-plugin /path/to/target/release/examples/graph_plugin \
  --sizes 100 1000 --repetitions 7 \
  --output research/local/results.json
```

The toolchain package lock pins Oxlint 1.86.0, TypeScript 7.0.2 and a TypeScript 5.9.3 parser alias used by the independent graph implementation. TypeScript 7 has a different compiler API, so the graph rule uses the pinned stable parser separately. The alias can override `.bin/tsc`; compiler version probes use `typescript/bin/tsc` directly. Neither compiler executable is timed. Node must support native TypeScript stripping for SDK plugins. The result records the actual versions, executable hashes, repository revision, platform, full commands and corpus hashes. Later runs also record load averages at the start, end and after every sample, plus native host-reported elapsed time. [toolchain/licenses.json](toolchain/licenses.json) records licenses for all locked packages, including optional platform packages. Oxlint is MIT; TypeScript is Apache-2.0. Dependencies are installed without lifecycle scripts and are not committed.

Each sample measures the wall time of a new complete process through captured JSON output. Trials run serially after one warmup per implementation, with a seeded randomized order and warm filesystem caches. Oxlint uses one thread. Results include every sample, median, minimum and maximum. These are process latency measurements on generated projects, not isolated parser throughput or cold-disk results.

## Equivalent work

For each size, every implementation receives exactly the same generated `.ts` files. The runner checks the file count, completion status, exit status, diagnostic count and expected violation set before it records timings, then repeats these checks on every sample. If the checks disagree, the workload is recorded as incomparable and receives no timings or ratios.

| Workload | Implementations | Expected work |
| --- | --- | --- |
| Transitive dependency | Native built-in, TypeScript subprocess SDK, Rust executable plugin, Oxlint JavaScript plugin | Each client imports a shared module; every fourth shared module imports its paired server module. Report the same forbidden client/server pairs. The rest are clean controls. |
| Direct import | Native `forbiddenImport`, Oxlint `no-restricted-imports`, Oxlint JavaScript plugin | Reject the literal `../server/db.ts` in every scanned file. Every fourth client violates the rule and the others import a shared module. |
| Static cycle | Native `noCycles`, Oxlint `import/no-cycle` | Find the same disjoint two-module type-only cycles. Remaining pairs have a one-way edge or a dynamic return edge. |

The native and executable Rust dependency rules use the same rule implementation. The TypeScript example traverses the host's graph through the SDK. The Oxlint graph plugin independently reads and parses the generated files with TypeScript 5.9.3, resolves their explicit relative `.ts` imports and traverses that graph. Its timing includes this extra parse and graph construction, plus Oxlint's own parsing. Oxlint can rebuild a graph without a project graph API. This implementation demonstrates that option. It supports the benchmark's explicit relative static imports only, and does not claim the host's workspace-package, alias or re-export provenance support.

The direct corpus contains value imports only; it does not establish type-only import equivalence. The direct JavaScript rule tests a local literal-import convention. It does not claim Effect service definition provenance. The graph benchmark includes both value and type-only dependencies on server modules with `includeTypes: true`. It contains no dynamic edges, so the dependency SDK's dynamic behavior is not compared there.

For cycles, Archguard emits one diagnostic per strongly connected component while Oxlint emits one per importing participant. The runner normalizes the actual reported import spans to unordered two-file cycle pairs and checks raw counts as well. `includeTypes: true` matches Oxlint `ignoreTypes: false`; `allowUnsafeDynamicCyclicDependency: true` permits the generated dynamic-return controls. These settings agree on this corpus. They are not a proof of equivalent handling of arbitrary mixed static/dynamic graphs, self-loops or larger components. Diagnostic rendering work differs, so cycle ratios describe equivalent detected cycle pairs with that reporting difference included.

The result has a separate process-overhead section. It compares a host with no rules to the same host plus an empty TypeScript SDK subprocess, and to the Rust graph executable on files with no selected clients. The added median includes JSON serialization, pipe IO, runtime startup, protocol decoding and shutdown. It is not just `node` startup time and is not subtracted from the workload measurements.

## Final measured results

[process-latency.json](https://github.com/alundgren/irudd-ts/blob/194bf91ce887498bf798cff2163d51bd897aa942/research/benchmarks/results/process-latency.json) records the accepted SDK query helper against source revision `3e177a3ebf3efbfa0897e69ed7e72f7e4e71a4bc`. Every workload remained complete and matched the same files, corpus hashes and expected violation sets as the initial run. Native CLI and Rust plugin executable hashes are identical across runs. Values below are process medians in milliseconds, followed by the minimum and maximum of seven samples.

| Graph files | Native | Rust plugin | TypeScript SDK | Oxlint independent JS graph |
| --- | --- | --- | --- | --- |
| 100 | 8.17 [6.05, 10.91] | 13.82 [12.56, 17.50] | 150.26 [132.94, 153.32] | 556.28 [515.04, 673.69] |
| 1000 | 26.13 [23.38, 46.57] | 40.85 [35.30, 55.66] | 170.51 [159.44, 183.58] | 735.00 [621.28, 1097.60] |
| 4000 | 97.79 [83.31, 126.28] | 112.78 [103.10, 147.83] | 277.50 [253.02, 289.66] | 1088.19 [1050.03, 1216.43] |

| Workload | Files | Native | Oxlint built-in | Oxlint local JS rule |
| --- | --- | --- | --- | --- |
| direct | 100 | 8.09 [6.62, 13.01] | 99.54 [90.63, 123.17] | 235.44 [198.69, 338.50] |
| cycles | 100 | 23.90 [6.62, 33.34] | 119.96 [113.58, 172.38] | not measured |
| direct | 1000 | 29.08 [24.54, 43.63] | 111.90 [103.24, 146.72] | 307.20 [278.87, 336.73] |
| cycles | 1000 | 52.61 [30.47, 186.79] | 263.44 [133.16, 370.00] | not measured |
| direct | 4000 | 134.23 [98.02, 153.80] | 208.26 [190.02, 233.89] | 597.73 [522.19, 641.99] |
| cycles | 4000 | 902.51 [144.27, 987.69] | 651.82 [606.93, 1095.20] | not measured |

The TypeScript graph plugin now builds its file lookup once and reuses it across client roots. Its 4000-file median fell from 786.42 to 277.50 ms on this corpus. Startup and protocol overhead remain substantial. The final empty SDK subprocess added 141.21, 152.36 and 162.89 ms to the no-rule host median at 100, 1000 and 4000 files; the Rust executable added 5.07, 17.83 and 50.90 ms. These figures include decoding and serialization as described above.

The final 4000-file cycle median was slower than Oxlint, and varied widely even with an unchanged native executable. The raw samples include native host elapsed time and load averages, which were about 1.3 during those samples. These shared-host runs establish observed latency and equivalent detected cycle pairs; they do not establish the cause of the spread or a universal speed advantage. Synthetic file counts also do not predict T3 latency because the real files and dependency graph contain much more code and different connections.

The final tool versions were Archguard 0.1.0, Node 24.21.0, Oxlint 1.86.0 and Rust 1.98.1 on Linux x86_64 with four reported CPUs. The TypeScript 7.0.2 compiler is installed for reference and is not timed. Independent graph parsing uses TypeScript 5.9.3.

## Recorded runs

The first release run is retained in [process-latency.before-optimization.json](https://github.com/alundgren/irudd-ts/blob/194bf91ce887498bf798cff2163d51bd897aa942/research/benchmarks/results/process-latency.before-optimization.json). It records seven samples at 100, 1000 and 4000 files against source revision `c91b24e67ffbc8be180f9632d5a9ac26b9d30963`. All nine workload/size combinations passed completion, file count and violation-set checks. These measurements precede the SDK query optimization and apply to that source revision only.

At 4000 files, transitive dependency process medians were 92.19 ms native, 117.18 ms Rust plugin, 786.42 ms TypeScript SDK and 1201.28 ms Oxlint's independent JavaScript graph implementation. The graph has 1667 imports and 334 expected forbidden pairs. Direct imports had 3998 imports and 1000 expected violations, with medians of 115.81 ms native, 210.99 ms Oxlint built-in and 557.37 ms Oxlint JavaScript. This direct corpus uses value imports only.

The 4000-file cycle corpus had 3000 imports and 500 expected cycle pairs. Native reported 500 component diagnostics and Oxlint reported 1000 import diagnostics. Native's median was 558.10 ms, with a 137.00 to 650.58 ms sample range; Oxlint's median was 905.03 ms, with an 834.80 to 992.61 ms range. The native sample spread is too large to infer steady-state algorithmic complexity from this run on the shared host. Cycle detection needs profiling before attributing its cost to a particular function.

The empty SDK process added 157.77, 154.08 and 176.70 ms to the no-rule host median at 100, 1000 and 4000 files. The Rust executable with no selected clients added 5.38, 12.27 and 34.42 ms. Full host medians, ranges and samples are in the result file. This is measured protocol and process overhead on this machine, not a fixed runtime tax.

A separate [T3 census summary](https://github.com/alundgren/irudd-ts/blob/194bf91ce887498bf798cff2163d51bd897aa942/research/t3code/results/t3-census-summary.json) records a single release scan of the uninstalled upstream checkout at `e0db2a5e58bcbe7d738bca7667d2440ddb83e30f`. The scoped scan included app, package, infra and script TypeScript files, omitted fixture directories and selected no policies. It found 3908 files and 29225 imports, with 4623 analysis problems and `complete: false`, exited 2 and took 3818.79 ms wall time. Missing generated desktop files and inherited Astro/Expo configuration are examples of these failures. It does not establish a clean architecture result or predict a configured, installed repository's latency. No target dependencies were installed and `.repos` references were outside the scan selection. The full local report is `/tmp/irudd-ts-research/t3-release-census.json`.

The October 3 compiler-provider experiment is separate from the earlier graph comparison: [semantic provider cost](../t3code/semantic/README.md) records seven fresh-process samples per reduced historical/control source, with graph-only, compiler-enabled and direct-provider modes. The native TypeScript 7.0.2 compiler is timed in this experiment. Neither this result nor the [cache experiment](../t3code/cache.md) establishes full-repository compiler throughput or a speed improvement.
