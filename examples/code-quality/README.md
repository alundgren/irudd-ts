# Authored review examples

Run these commands from the Archguard repository root after building the CLI:

```sh
cargo build --locked --bins --examples
cargo run --locked -- dryer --root . --config examples/code-quality/dryer.json
cargo run --locked -- mutator plan --root . --config examples/code-quality/plan.json --json
node examples/code-quality/prepare-domain.ts /tmp/archguard-domain-profiles
cargo run --locked -- mutator run --root . --config /tmp/archguard-domain-profiles/weak.json
cargo run --locked -- mutator run --root . --config /tmp/archguard-domain-profiles/strong.json
```

The mutation commands need Node 24. They copy only the domain, harness and SDK protocol file. No npm installation is necessary. The tiny synchronous harness executes real assertions and identifies each failed test. For an existing Vitest project, use the [Vitest reporter](../../docs/mutator-test-protocol.md).

`domain.ts` supplies shipping, invoice and approval examples. `run-domain.ts` contains deliberately weak tests and strengthened behavior checks. Both pass against the original implementation. Inspect surviving mutants rather than aiming for a percentage. The absolute-magnitude display normalizes negative zero before adding zero, so subtracting zero is an equivalent arithmetic mutation. The strengthened tests explicitly check signed zero.

`clones.ts` contains authored invoice/order copy controls and similar port/retry adapters. The copy controls deserve inspection. The adapters express different input contracts, so their shared structure alone does not justify merging them.

See the [guide](../../docs/code-quality.md) for configuration, execution bounds, cache assumptions and review use.
