# Development

Use Rust 1.96 or newer and Node 24. Structural builds and the default test suite do not need compiler, Effect, or benchmark installations:

```sh
cargo build --locked --bins --examples
cargo test --locked
```

Native-provider tests are explicit:

```sh
npm ci --prefix providers/typescript7 --ignore-scripts --no-audit --no-fund
cargo test --locked --features semantic-tests --test semantic_provider
```

Before a PR, install the pinned historical declaration environment and run complete validation:

```sh
npm ci --prefix research/t3code/semantic --ignore-scripts --no-audit --no-fund
scripts/check.sh
```

`check.sh` runs formatting, all-target/all-feature Clippy, generic and native-provider tests, historical regression/profile tests, both npm license inventories, the Cargo license inventory, active documentation links, and Archguard's own architecture policy. It installs nothing and fails when required environments are missing. Historical checks use `research-tests`; that feature affects test selection only, not product behavior. Neither test feature adds a Rust dependency.

See [architecture](architecture.md) for module and dependency ownership. README is for adopters. AGENTS.md is for agent instructions. Research-specific conventions belong in `research/AGENTS.md`; research setup and commands belong in [the research index](../../research/README.md).

To review a distribution, use `cargo package --locked --allow-dirty --no-verify`, inspect its normalized manifest and file list, then build or test the extracted archive. Do not publish as part of this validation. Installed `node_modules`, historical research tests, and measurement archives must not enter the crate.

License inventories can be refreshed with `python3 scripts/licenses.py --write`, `python3 scripts/semantic_licenses.py --write`, and `python3 scripts/semantic_licenses.py --history --write`. Review the resulting license and notice records.

Optional actual Vitest compatibility controls use an explicitly installed target repository:

```sh
node scripts/check_mutator_vitest.mjs /absolute/installed/t3code
```

This creates owned temporary fixtures and leaves the target source untouched. The Node SDK controls are part of `check.sh`. Focused T3 quality profiles and dependency-copy requirements are described in the [authored examples](../../examples/code-quality/README.md). Run those separately from validation when recording runtime or resource evidence.
