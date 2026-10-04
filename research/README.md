# Research and reproductions

Research is optional for using Archguard. Product builds, graph plugins, and small examples do not install comparison tools or upstream application dependencies.

| Material | Location |
| --- | --- |
| T3 historical graph and compiler reproductions | [T3 research](t3code/README.md) |
| Individual test value, mutation matrices and historical comparisons | [Test value experiment](test-value/README.md) |
| Generic equivalent-work benchmarks | [Benchmark method and results](benchmarks/README.md) |
| Tool choices and licensing research | [Tooling notes](notes/tooling.md) |
| Possible future facts | [Fact research](notes/facts.md) |
| Dated compiler API investigation | [TypeScript 7 notes](notes/typescript-7.md) |

Raw measurement JSON and archived HTML reports keep their original bytes, revisions, hashes, and recorded paths. [relocations.json](relocations.json) maps each preserved artifact's original location to its current location and verifies its SHA-256 hash. Historical path strings inside those artifacts describe the original run; use maintained commands in these READMEs for a new run. Archived reports may contain links from their original checkout layout.

Write new output under the ignored `research/local/` directory. Run builds, validation, and measurements in separate windows. Keep negative results and raw ranges. A clean selected closure does not establish full upstream coverage.
