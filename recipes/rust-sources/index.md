# Check direct Rust module dependencies

Apply a dependency rule to crate::module imports in Rust source.

Category: Control dependencies
Capabilities: rust

You need: Built Archguard CLI on your PATH.

Diagram: client.rs → crate::server → server.rs · blocked

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.rs"
  ],
  "rules": [
    {
      "id": "client-server",
      "kind": "forbiddenDependency",
      "files": [
        "src/client.rs"
      ],
      "targets": [
        "src/server.rs"
      ]
    }
  ]
}
```

## src/lib.rs

```rust
pub mod client;
pub mod server;
```

## src/client.rs · before

```rust
use crate::server;
pub fn run() {}
```

## src/server.rs

```rust
pub fn serve() {}
```

## src/client.rs · correction

```rust
pub fn run() {}
```

## Run from your project

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

The direct crate::server import produces a client-server finding. Remove it for a clean check.

## Keep in mind

Only the first module component resolves. self::, super::, inline modules, nested resolution, macros, and compiler types are unsupported.

## Reference and source

- [docs/guides/configuration.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/configuration.md)

## Related recipes

- [block-server](https://alundgren.github.io/irudd-ts/recipes/block-server/index.md)
