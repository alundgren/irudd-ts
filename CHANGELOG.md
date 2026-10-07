# Changelog

## 0.1.0 (2026-10-07)


This is Archguard's first public release. The CLI, configuration, and extension
contracts are versioned, but the project has not declared 1.0 stability.

### Features

* Check TypeScript, JavaScript, and supported Rust source imports with built-in architecture rules and shared Rust/TypeScript graph-plugin contracts ([888586e](https://github.com/alundgren/irudd-ts/commit/888586e)).
* Configure source resolution, package dependencies, file roles, required companions, and registry import policies ([c4df6a9](https://github.com/alundgren/irudd-ts/commit/c4df6a9), [cafd34f](https://github.com/alundgren/irudd-ts/commit/cafd34f)).
* Require installed external package resolution explicitly and retain lookup failures as incomplete analysis ([0a62589](https://github.com/alundgren/irudd-ts/commit/0a62589)).
* Query compiler member facts through the optional, separately installed TypeScript 7.0.2 provider ([760f99c](https://github.com/alundgren/irudd-ts/commit/760f99c)).
* Reuse complete source graphs through an opt-in cache that revalidates source and resolver inputs ([85c93f3](https://github.com/alundgren/irudd-ts/commit/85c93f3)).
* Inspect TypeScript/TSX structural duplicates with `dryer`, plan exact source mutations with `mutator plan`, and run explicitly configured trusted tests in private worker copies with `mutator run` ([4b38712](https://github.com/alundgren/irudd-ts/commit/4b38712), [b6017bb](https://github.com/alundgren/irudd-ts/commit/b6017bb)).
* Integrate mutation test results through the Node-builtin TypeScript SDK and the supplied Vitest reporter ([7521944](https://github.com/alundgren/irudd-ts/commit/7521944)).
* Evaluate architecture, duplication, and test behavior within explicit time and disk budgets using the bundled agent skill ([31e789c](https://github.com/alundgren/irudd-ts/commit/31e789c)).
* Install native Linux and macOS binaries on x86_64 and arm64, with SHA256 checksums, dependency license notices, examples, guides, and a composite GitHub Action ([7b3aa1a](https://github.com/alundgren/irudd-ts/commit/7b3aa1a)).


### Bug Fixes

* respect release workflow cancellation ([f1d5d3a](https://github.com/alundgren/irudd-ts/commit/f1d5d3ae19c18caa493ba00c2efc964e6394a5ec))
* verify release identities and resume public publication ([12d6e3d](https://github.com/alundgren/irudd-ts/commit/12d6e3deb0e256839f40780837cff20d0e4999da))

### Requirements and behavior

* Native Linux archives require glibc 2.35 and libgcc; release smoke checks run on Ubuntu 22.04. macOS archives require macOS 15 or newer. Windows binaries are not provided.
* Structural checks need no Rust or Node installation. SDK scripts and the optional compiler provider require Node 24. Building from source requires Rust 1.96 or newer.
* Architecture checks exit 0 for complete clean analysis, 1 for complete policy violations, and 2 for incomplete analysis or invalid configuration. Complete duplicate reports and mutation runs exit 0 even when they find similar functions or surviving mutations.
* Source resolution does not promise compiler or runtime parity. Duplicate scores and mutation survivors are review evidence; the commands do not impose a score threshold. Configured plugin, provider, and test commands must be trusted.
