# Graph cache measurements

See the [cache guide](../../docs/guides/cache.md) for product behavior.

## Reproducing measurements


Build the CLI and examples with the repository's pinned Rust toolchain, then run trials while other builds and analyses are idle:

```sh
cargo build --release --locked --bins --examples
python3 research/t3code/scripts/verify_history.py /path/to/t3code
python3 research/t3code/scripts/benchmark_cache.py --binary target/release/archguard --output research/local/cache.json
```

The runner checks cached and fresh facts and diagnostics after every edit step. PR13151 replays the existing verified type-cycle reduction under generic, Android and iOS orders, including a missing-target intermediate and its correction. PR14389 changes the barrel export while its consumer import stays unchanged. These are reduced source-edit replays of merged PRs. Their chosen edit order is a measurement sequence, not a reconstruction of the original development session.

Trials compare fresh analysis, an empty persistent cache, unchanged reuse and edited reuse. Before measuring an unchanged or edited case, the runner applies every preceding edit with the same cache, including incomplete states. These priming invocations are outside the measured latency. Empty cache describes persisted data; the filesystem may already be warm. The runner retains every sample, ranges, corpus hashes, upstream evidence, source revision and executable hash. Incomplete intermediate states remain exit 2 and are not accepted as complete entries. Historical clean controls run alongside violations.

An optional complete installed-project control can be added with `--control-root`, `--control-config` and `--control-revision`. It measures the selected source inventory recorded in its profile, separately from historical PR evidence. Do not infer whole-repository coverage from a selected dependency closure.

The pre-implementation reduction baseline is retained separately from the same-binary comparisons. These cases contain only two to four selected files, so content validation and persistence can cost more than fresh parsing. Report slower cache runs and their ranges. There is no general speed claim.

## Measured result

The corrected seven-sample run was slower with caching in every measured mode. Actual graph reuse was verified. The complete installed server control reused all 1,012 files and 9,790 imports without parsing or resolution, yet its median increased from 1,711.80 ms fresh to 5,655.60 ms unchanged. Caching remains opt-in; these results provide no performance recommendation.

The [continuous-cache measurements](results/cache/process-latency.json) retain all 490 samples and 70 warmups, exact cached/fresh facts and report comparisons, and the priming state history. The missing-target intermediate remains incomplete and retains the previous complete snapshot. Its correction publishes a new complete graph. PR14389's barrel correction reparses one file and reuses two unchanged files and two imports while clearing its policy diagnostic.

| Complete installed server control | Median ms | Range ms |
| --- | ---: | ---: |
| Fresh analysis | 1,711.80 | 1,583.94 to 2,388.15 |
| Empty persistent cache | 3,799.32 | 3,317.09 to 5,040.45 |
| Unchanged graph reuse | 5,655.60 | 5,118.89 to 8,106.51 |

| Reduced merged-PR final state | Mode | Median ms | Range ms |
| --- | --- | ---: | ---: |
| PR13151 generic | Fresh analysis | 9.85 | 5.37 to 15.61 |
| PR13151 generic | Unchanged reuse | 31.81 | 22.43 to 72.09 |
| PR13151 generic | Final source edit | 35.44 | 31.65 to 52.88 |
| PR14389 | Fresh analysis | 7.16 | 5.82 to 12.91 |
| PR14389 | Empty persistent cache | 33.73 | 23.47 to 84.73 |
| PR14389 | Unchanged reuse | 24.69 | 22.05 to 51.75 |
| PR14389 | Barrel source edit | 33.73 | 23.01 to 55.40 |

The selected server closure uses upstream revision `e0db2a5e58bcbe7d738bca7667d2440ddb83e30f`, profile SHA-256 `50fb08cea2c5a7644b9c235aa0fc84a63a2d700ff5417ddf9a3f3e284e88df8c`, and recorded corpus SHA-256 `3835a8c11501c73268056d53d2da8740f4cb026004eb16e8b61691f8922bbfa1`. The executable SHA-256 is `9de47233535feac579e4c4d76604c3d182dfc6bd1e5bda42f2e990a9b54ffc13`, built from source `df3933172d834f2d9ead837bc728002e538909ff` with no tracked diff. The corrected runner and documentation revision is `d77c8659d7cd74b4576f27bcb3e5b4b5a3e915da`, also with no tracked diff. [The measured source branch](https://github.com/alundgren/irudd-ts/tree/benchmarks/incremental-cache-measured) preserves these exact revisions before semantic integration. Combined cache/compiler tests validate integration separately; these timings describe the recorded standalone executable. There are no enabled policy rules in this large control; the separate merged-PR reductions exercise policy changes and clean controls.

The original [reset-primer measurements](results/cache/process-latency-reset-primer.json) also retain 490 samples and 70 warmups, including all slower results. That run primed only the immediate previous state into an empty cache. It omitted the last complete entry when that state was incomplete, so its incomplete-state and following-correction comparisons do not represent a continuous persistent edit sequence. Its step-zero large control is unaffected, with fresh median 1,642.33 ms and unchanged median 5,616.65 ms. A later restarted run overlapped another validation job at its start and was discarded before writing an output dataset. Only the isolated restarted run supplies the continuous-cache results above.
