# Dependency licenses

Archguard is [MIT licensed](../../LICENSE). Check the locked inventories before redistributing binaries or upstream assets:

- [Cargo packages](cargo.json), maintained by `scripts/licenses.py`.
- [TypeScript provider](typescript7.json), maintained by `scripts/semantic_licenses.py`.
- [Historical Effect environment](../../research/t3code/semantic/licenses.json), maintained by `scripts/semantic_licenses.py --history`.
- [Benchmark toolchain](../../research/benchmarks/toolchain/licenses.json), retained with its experiment dependencies.

The provider inventory includes SHA-256 hashes of installed TypeScript `LICENSE` and `NOTICE.txt`. Retain those upstream notices when redistributing compiler files. Copied historical T3 fixtures retain their own MIT notices under research. Research dependencies do not become product dependencies.

Binary archives copy the actual license and notice files from the locked Cargo
packages, including nested Unicode notices and attribution records. They also
retain the pinned Rust standard library's copyright and license files. Some
published Cargo packages omit their licenses. For those packages,
`upstream/index.json` maps the exact package version and upstream commit to
byte-checked MIT notice files in `upstream/`. The commit comes from the Cargo
package's `.cargo_vcs_info.json`; the files come from that repository revision.
A new package version without an actual notice or reviewed mapping fails
packaging. Review and refresh those records when updating dependencies.
The optional compiler source adapter ships without installed npm packages.
