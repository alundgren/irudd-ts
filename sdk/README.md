# Extension SDKs

[sdk/index.ts](index.ts) exports the TypeScript graph contract and helpers. [sdk/semantic.ts](semantic.ts) exports compiler member facts and helpers. These modules use Node builtins and have no npm dependencies.

The Rust SDK is the public `archguard` crate API, including `facts::ProjectFacts`, `facts::ProjectRule`, `roles::RepositoryPolicy`, and `semantic::SemanticFacts`. Keeping it in the product crate lets native rules share the same contracts without another package.

Start with the [plugin guide](../docs/extensions/plugins.md) or [semantic provider guide](../docs/extensions/semantic-provider.md). [Examples](../examples/README.md) show both languages; contract and parity tests live under `tests/`.

The mutation helpers require a bounded regular request file. They compare bigint file identities and byte metadata between the path and opened descriptor before and after reading, and reject linked files, detected replacements, and unavailable identities. Unix also uses atomic no-follow opens. Windows checks rely on Node's 64-bit file index; [Node's filesystem library](https://docs.libuv.org/en/v1.x/fs.html#file-open-constants) does not expose atomic no-follow opens or the Rust host's 128-bit native identity. Requests and results retain the same protocol on both platforms.
