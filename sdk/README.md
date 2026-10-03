# Extension SDKs

[sdk/index.ts](index.ts) exports the TypeScript graph contract and helpers. [sdk/semantic.ts](semantic.ts) exports compiler member facts and helpers. These modules use Node builtins and have no npm dependencies.

The Rust SDK is the public `archguard` crate API, including `facts::ProjectFacts`, `facts::ProjectRule`, `roles::RepositoryPolicy`, and `semantic::SemanticFacts`. Keeping it in the product crate lets native rules share the same contracts without another package.

Start with the [plugin guide](../docs/extensions/plugins.md) or [semantic provider guide](../docs/extensions/semantic-provider.md). [Examples](../examples/README.md) show both languages; contract and parity tests live under `tests/`.
