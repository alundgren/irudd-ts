# Graph plugins

[main.rs](main.rs) and [plugin.ts](plugin.ts) both reject dependencies from `client/` to `server/`, including paths through a shared helper. The included project is clean.

```sh
cargo build --locked --bins --examples
./target/debug/archguard check --root examples/graph-plugin --config examples/graph-plugin/archguard.json
./target/debug/archguard facts --root examples/graph-plugin --config examples/graph-plugin/archguard.json > /tmp/archguard-example-facts.json
./target/debug/examples/graph_plugin < /tmp/archguard-example-facts.json
node examples/graph-plugin/plugin.ts < /tmp/archguard-example-facts.json
```

The configured command runs the TypeScript plugin from this example directory. Add `import '../server/db.ts';` to `client/main.ts` to produce a boundary violation; remove it to restore the clean control.
