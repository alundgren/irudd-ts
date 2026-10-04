# T3 Code research

[Adoption profiles](../../examples/t3code/README.md) are examples. This directory owns the evidence used to assess them.

| Material | Location |
| --- | --- |
| Verified graph reductions and upstream notice | [history/](history/), [source evidence](historical-evidence.json) |
| Compiler regression and Effect installation | [semantic reproduction](semantic/README.md) |
| Synthetic repository conventions | [Structure experiment](structure.md) |
| Installed source profiles and remaining failures | [Installed resolution](installed-resolution.md), [profiles](installed/profiles/) |
| Continuous and reset-primer cache trials | [Cache measurements](cache.md), [raw results](https://github.com/alundgren/irudd-ts/blob/194bf91ce887498bf798cff2163d51bd897aa942/research/t3code/results/cache) |
| Regression acceptance tests | [tests/](tests/) |
| Archived summaries | [overnight report](reports/overnight.html), [followup report](reports/followup.html) |

```sh
npm ci --prefix providers/typescript7 --ignore-scripts --no-audit --no-fund
npm ci --prefix research/t3code/semantic --ignore-scripts --no-audit --no-fund
cargo test --locked --features semantic-tests,research-tests
python3 research/t3code/scripts/verify_history.py /path/to/t3code
python3 research/t3code/scripts/verify_semantic_history.py /path/to/t3code
```

Verifiers need a local clone containing the exact recorded upstream commits. Tests use copied reductions and authored negative controls; they do not require a full T3 dependency install. Their Cargo target names are `t3_history`, `t3_cache`, `t3_profiles`, and `t3_semantic`.

For new measurements, first build `cargo build --release --locked --bins --examples`. Run `python3 research/t3code/scripts/simulate_structure.py`, `python3 research/t3code/scripts/benchmark_cache.py`, `python3 research/t3code/scripts/benchmark_semantic.py`, or `python3 research/t3code/scripts/installed_resolution.py` from the repository root. Use `--help` to see their inputs. Pass an explicit output under `research/local/` when rerunning installed-source measurements.

Historical graph reproductions, synthetic conventions, and current adoption candidates establish different claims. A fixed revision must stop triggering the reproduced failure. Retain incomplete inventories and unsupported imports; do not add exclusions solely to produce a clean result.
