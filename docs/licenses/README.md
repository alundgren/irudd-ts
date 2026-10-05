# Dependency licenses

Archguard is [MIT licensed](../../LICENSE). Check the locked inventories before redistributing binaries or upstream assets:

- [Cargo packages](cargo.json), maintained by `scripts/licenses.py`.
- [TypeScript provider](typescript7.json), maintained by `scripts/semantic_licenses.py`.
- [Historical Effect environment](../../research/t3code/semantic/licenses.json), maintained by `scripts/semantic_licenses.py --history`.
- [Benchmark toolchain](../../research/benchmarks/toolchain/licenses.json), retained with its experiment dependencies.
- [Website fonts](../../website/assets/fonts/inventory.json), pinned IBM Plex files with their retained [SIL Open Font License](../../website/assets/fonts/OFL.txt).

The provider inventory includes SHA-256 hashes of installed TypeScript `LICENSE` and `NOTICE.txt`. Retain those upstream notices when redistributing compiler files. Copied historical T3 fixtures retain their own MIT notices under research. Research dependencies do not become product dependencies.
