# TypeScript 7 semantic provider

This optional provider owns its compiler installation. Structural checks and graph plugins do not install or use it.

```sh
npm ci --prefix providers/typescript7 --ignore-scripts --no-audit --no-fund
```

The manifest and lock pin TypeScript 7.0.2, including its native platform package. The provider uses `typescript/unstable/sync`; another compiler version is not a fallback.

Read the [provider guide](../../docs/extensions/semantic-provider.md) for contexts, completion checks, and trust requirements. Try the [semantic example](../../examples/semantic/README.md).

The historical T3 reproduction installs Effect separately under [research/t3code/semantic](../../research/t3code/semantic/README.md). Those dependencies are unnecessary for this provider.
