# Compiler member checks

The project accesses an existing `account.name` member. Its separate [semantic.json](semantic.json) selects one compiler context and explicitly runs the maintained provider.

```sh
npm ci --prefix providers/typescript7 --ignore-scripts --no-audit --no-fund
cargo build --locked --bins --examples
./target/debug/archguard check --root examples/semantic --config examples/semantic/archguard.json --semantic-config examples/semantic/semantic.json --json
./target/debug/archguard semantic-facts --root examples/semantic --config examples/semantic/archguard.json --semantic-config examples/semantic/semantic.json
```

Change `account.name` to `account.missing` to see a member finding and compiler diagnostic. The check exits 2 because compiler errors make analysis incomplete. Restore `account.name` for a clean result. With `@ts-ignore` suppressing the compiler error, the independent member rule can still produce exit 1.

[request.rs](request.rs) emits the provider request from graph facts:

```sh
./target/debug/archguard facts --root examples/semantic --config examples/semantic/archguard.json > /tmp/archguard-semantic-graph.json
./target/debug/examples/semantic_request examples/semantic/semantic.json < /tmp/archguard-semantic-graph.json
```
