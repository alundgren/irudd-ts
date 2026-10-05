# Use the Rust graph-rule API

Check immutable project facts in Rust or return findings from an executable plugin.

Category: Add your own checks
Capabilities: Rust SDK, ProjectRule

You need: Rust 1.96+ to build the repository example. The Rust snippet is an API excerpt; use the linked complete main.rs.

Diagram: ProjectFacts v1 → ProjectRule::check → Diagnostics

## Archguard checkout

```shell
cargo build --locked --bins --examples
./target/debug/archguard facts --root examples/graph-plugin --config examples/graph-plugin/archguard.json > /tmp/graph.json
./target/debug/examples/graph_plugin < /tmp/graph.json
```

## examples/graph-plugin/shared/message.ts · try a violation

```typescript
import '../server/db.ts';
export const message = 'hello';
```

## Core API · excerpt from examples/graph-plugin/main.rs

```rust
use archguard::facts::ProjectRule;

let diagnostics = rule.check(&project)?;
println!("{}", serde_json::json!({
    "schemaVersion": 1,
    "diagnostics": diagnostics
}));
```

## What to expect

The bundled graph is clean: diagnostics is empty. Apply the shared/message.ts edit below, then export facts again and rerun the plugin to see the client-server finding. Restore the file afterward.

## Keep in mind

Executable plugins read one JSON project on stdin and write one versioned diagnostics object on stdout. Put logs on stderr.

## Reference and source

- [examples/graph-plugin/main.rs](https://github.com/alundgren/irudd-ts/blob/main/examples/graph-plugin/main.rs)
- [docs/extensions/plugins.md](https://github.com/alundgren/irudd-ts/blob/main/docs/extensions/plugins.md)

## Related recipes

- [typescript-plugin](https://alundgren.github.io/irudd-ts/recipes/typescript-plugin/index.md)
