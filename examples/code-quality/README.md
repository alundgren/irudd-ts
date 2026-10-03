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

## Focused T3 Code examples

With a separate T3 checkout and an explicitly installed, self-contained dependency directory:

```sh
node examples/code-quality/prepare-t3.ts /absolute/t3code /absolute/node_modules /tmp/archguard-t3-profiles
archguard dryer --root /absolute/t3code --config /tmp/archguard-t3-profiles/dryer.json
archguard dryer --root /absolute/t3code --config /tmp/archguard-t3-profiles/dryer-native.json
archguard dryer --root /absolute/t3code --config /tmp/archguard-t3-profiles/dryer-native-erased-locals.json
archguard mutator run --root /absolute/t3code --config /tmp/archguard-t3-profiles/path.json
archguard mutator run --root /absolute/t3code --config /tmp/archguard-t3-profiles/path-strengthened.json
archguard mutator run --root /absolute/t3code --config /tmp/archguard-t3-profiles/hostClassification.json
archguard mutator run --root /absolute/t3code --config /tmp/archguard-t3-profiles/delimitedPreview.json
archguard mutator run --root /absolute/t3code --config /tmp/archguard-t3-profiles/gitPatchPath.json
archguard mutator run --root /absolute/t3code --config /tmp/archguard-t3-profiles/gitPatchPath-strengthened.json
```

These are focused reproductions of unchanged source and tests using an authored Vitest configuration. They do not run T3's entire suite or its root setup. Profiles keep each module/test pair together and copy the SDK reporter and dependencies explicitly. The dependency directory must contain its actual package contents and links whose targets stay inside that directory; external package-store links need an explicit self-contained copy. The generator installs nothing and leaves the T3 checkout unchanged.

Generated profiles use the running Node binary directly. This avoids version-manager launchers depending on the original user's home in a private workspace. Static domain profiles are also supplied for systems where `node` resolves to a standalone runtime.

The strengthened path profile adds two authored tests in the private worker copy. They cover `../repo` and `.\\repo` relative paths and trimming `x/` to `x`, with ordinary paths and root paths as controls. The original T3 checkout stays untouched. These examples follow existing call sites and trailing-separator expectations; they do not prescribe how dispatch should represent `C:/`.

The strengthened Git patch-path profile checks one-character filename text next to escaped tab/newline characters, quoted and unquoted decoding, and round trips. Multi-character and ordinary filenames are negative controls. It leaves quoted-empty-token behavior as an API specification question. A nonadvancing parser-loop mutation can time out; the product reports that run as incomplete rather than counting a kill.

The native dryer profile compares three TSX editor implementations. Platform variants are an example of duplication that may be intentional. The separate exploratory profile lowers the threshold to 0.70 so a reviewer can inspect additional candidates. Another profile erases local identifiers while preserving property and call names. Comparing it with the default shows how binding relationships affect matches after declarations are inserted. None of these profiles establishes that an abstraction would improve the code.
