# T3 repository conventions

The [repository-rule guide](../../docs/guides/repository-rules.md) describes the general capability. This experiment uses the [T3 structural profile](../../examples/t3code/profiles/t3code-structure.json).

## T3 conventions and proposed rules


The practice checkout is pinned to `e0db2a5e58bcbe7d738bca7667d2440ddb83e30f`. The simulation reads it without executing repository code. All fixtures in the simulation script are independently authored examples. They reproduce structural policies, not historical merged-PR failures.

| T3 role | Observed convention | Simulation policy |
|---|---|---|
| Migration | 54 numbered modules, IDs 1 through 54, imported in [Migrations.ts](https://github.com/pingdotgg/t3code/blob/e0db2a5e58bcbe7d738bca7667d2440ddb83e30f/apps/server/src/persistence/Migrations.ts#L14). Entries use imported modules in a separate tuple list. | Numbered source names, default export and registry import prerequisite. Tuple IDs, ordering and membership are future work. |
| Persistence service | 11 `Services` modules have same-basename `Layers` modules, for example [ProjectionThreads](https://github.com/pingdotgg/t3code/blob/e0db2a5e58bcbe7d738bca7667d2440ddb83e30f/apps/server/src/persistence/Services/ProjectionThreads.ts). | Service-to-layer companion and proposed transitive ban on service-to-live-layer imports. The infrastructure `Layers/Sqlite.ts` does not need a matching service. |
| MCP tool contract and handler | `device`, `preview` and `pullRequests` each have `tools.ts`, `handlers.ts` and `handlers.test.ts`. [Preview tools](https://github.com/pingdotgg/t3code/blob/e0db2a5e58bcbe7d738bca7667d2440ddb83e30f/apps/server/src/mcp/toolkits/preview/tools.ts#L244) legitimately use combined and smaller toolkits. | Bidirectional tools/handlers companions, handler test companion and proposed handler-to-migration ban. No requirement for one toolkit per folder or a `tools.test.ts`. |
| Shared contract and API host | Contracts live in `packages/contracts`; server entry modules include `ws.ts` and `http.ts`. [RPC declarations](https://github.com/pingdotgg/t3code/blob/e0db2a5e58bcbe7d738bca7667d2440ddb83e30f/packages/contracts/src/rpc.ts#L466) associate methods with schemas and compose a group. | Proposed transitive contract-to-host ban. Method/group/schema correspondence is future work. |
| Tests | Adjacent tests appear in several roles. Migration tests cover selected upgrades: 18 test files for 54 migrations, including a combined test for migrations 27 and 28. | Production-to-test import ban. Require handler tests, where all three current toolkits have them. No blanket test-per-migration requirement. |

The profile has nine roles and eleven rule entries. Classification is intentionally limited to the named directories and host modules. New helper files within those directories may need a configured helper role. This is an adoption candidate, separate from the default T3 policy. A rule count is not a count of demonstrated bugs.

T3 documents queues in [DrainableWorker](https://github.com/pingdotgg/t3code/blob/e0db2a5e58bcbe7d738bca7667d2440ddb83e30f/packages/shared/src/DrainableWorker.ts). We found no queue-message directory convention in the investigated server paths. A different repository could encode a `queue-message` role and producer/consumer dependencies with the same role configuration, but T3 does not justify that policy yet.

## Reproduce the simulation

```bash
cargo build --release --locked --bins --examples
python3 research/t3code/scripts/simulate_structure.py --t3 /path/to/t3code
./target/release/archguard check --root /path/to/t3code --config examples/t3code/profiles/t3code-structure.json --json
./target/release/archguard facts --root /path/to/t3code --config examples/t3code/profiles/t3code-structure.json > /tmp/t3-facts.json
./target/release/examples/role_inventory examples/t3code/profiles/t3code-structure.json < /tmp/t3-facts.json
```

The runner verifies exact diagnostic IDs, complete analysis and exit 0 or 1 on eleven synthetic cases. Each mutation has a correction check against the clean fixture. The clean control permits selective migration testing, split toolkit exports, layer infrastructure and host-to-contract imports. The optional real-tree scan requires the clean pinned checkout, records all analysis problems and labels diagnostics provisional when incomplete. It records executable, configuration, source-input and corpus hashes, every timing sample, ranges and medians. Timing is end-to-end process latency including parsing, source resolution and the selected rules. There is no Oxlint comparison or historical detection claim in this experiment.

Raw results are saved to [structure-simulation.json](https://github.com/alundgren/irudd-ts/blob/194bf91ce887498bf798cff2163d51bd897aa942/research/t3code/results/structure-simulation.json). The broad research inventory is [fact research](../notes/facts.md).

## Recorded results

The complete pinned T3 scan selected 1,151 files and 10,451 imports. It returned exit 0, zero analysis problems and zero policy diagnostics. It assigned 54 migrations, 11 persistence services, 12 persistence layers including infrastructure, three tool-contract modules, three handlers, 48 shared-contract modules, two API hosts, one migration registry and 474 tests. The 543 unassigned sources are outside the required classification scope. There were no overlapping assignments.

That single end-to-end real-tree scan took 865.38 ms. It is an observation on this machine, not a general latency guarantee. The eleven synthetic cases used 11 to 13 files and seven measured runs after a warmup. Their medians ranged from 10.09 to 18.67 ms; the clean control's median was 18.67 ms. All ten deliberate violations produced exactly the expected rule, and all corrections passed. See the raw artifact for every sample and range.

The [before-closure artifact](https://github.com/alundgren/irudd-ts/blob/194bf91ce887498bf798cff2163d51bd897aa942/research/t3code/results/structure-simulation-before-closure.json) retains the first experiment: 1,096 files, 84 excluded-source problems, exit 2 and no provisional diagnostics. Explicitly adding the referenced integration helpers and workspace package source directories made the later scan complete. The selected corpora differ, so these two real-tree timings are not a speed comparison.

## Useful next facts

Registration references would let us check whether each migration's imported binding appears in the actual registry tuple, with a matching unique numeric ID and basename. The same declaration/reference facts could link `Rpc.make` declarations to RPC groups, and `Toolkit.make` members to handler mappings. They need lexical identity and structured argument values; matching strings alone is insufficient.

Package dependency kinds would distinguish production, development, peer and optional dependencies. The current package facts merge production, peer and optional names, and omit development dependencies. Build target membership and project tags could enforce project ownership without relying exclusively on directory layout.

Versioned schema facts would support compatibility rules against a declared baseline. TypeScript aliases, Effect schemas, persisted event versions and database schema changes need different providers. Presence of a contract file cannot prove compatibility with old clients or stored events. An append-only migration policy also needs baseline file hashes and rename/deletion history; the current-tree check cannot supply that guarantee.

Compiler-derived types and Effect requirements could enforce that a handler requires only approved services. Keep these separate from Oxc lexical facts. General control flow, taint, disposal and concurrency checks need their own analysis providers and explicit incomplete states.
