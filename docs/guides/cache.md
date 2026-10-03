# Persistent graph cache

Caching is opt-in. Supply a cache file outside selected source and package-manifest paths:

```sh
archguard check --root /path/to/project --config archguard.json --cache /tmp/project.archguard-cache.json --json
archguard facts --root /path/to/project --config archguard.json --cache /tmp/project.archguard-cache.json
```

The cache stores syntax facts and resolved imports. Every invocation discovers files and packages again, reads selected source bytes, checks resolver filesystem inputs and reruns policy rules and configured plugins. An unchanged graph can reuse resolved imports; a changed source file is parsed and resolved again. Changes to a target's exports still affect policy because rules inspect the newly assembled project.

Cache identity includes the canonical project root, effective configuration, facts and cache versions, executing binary digest and embedded Cargo lock digest. Changing parser, resolver, host code or resolver options requires a new entry. Source validation uses SHA-256 of the bytes rather than modification time or file size.

The resolver records content reads, file-kind checks, missing paths, symbolic links and canonical paths through its public filesystem interface. This includes ancestor, inherited and referenced tsconfigs and installed package metadata, even when they are outside the project root. Workspace package inputs and the set of successfully parsed source files are also checked. A changed resolver input invalidates all cached resolved imports. This conservative invalidation keeps shared resolver lookups accounted for. Matching syntax entries can still be reused.

Only complete source graphs are written. A parser failure or unresolved internal dependency remains an analysis problem. An incomplete edit retains the previous complete entry, and matching files can still reuse it during the next correction. A project that has never produced a complete entry cannot reuse a persisted graph.

The cache envelope has an integrity checksum and is replaced atomically. A corrupt cache is discarded; the invocation analyzes fresh source and reports a cache problem with exit 2. When fresh analysis completes and persistence works, it replaces the bad entry so a later invocation can recover. A changed cache version is an ordinary miss. Cache read or write failures also produce exit 2. Cache data is local trusted data; the checksum detects accidental corruption and is not an authentication mechanism.

The `facts` contract is unchanged. JSON `check` reports include optional `cache` statistics when caching is requested: parsed and reused file counts, resolved and reused import counts, observed resolver input count and whether a new entry was published. Existing no-cache reports omit that field. Reuse statistics establish avoided parsing and resolution work; they do not establish a latency improvement.

`--cache` can be combined with `check --semantic-config`. The cache stores the source graph; the compiler provider reruns independently. A complete source graph may be published while a missing compiler context makes the combined check exit 2. Reusing that graph still reports the semantic failure. Correcting the compiler context can produce exit 0 without reparsing or resolving the unchanged source graph. Legacy graph plugins receive the same strict ProjectFacts1 payload, and semantic problems are added to the final report after those plugins run.


The recorded [cache measurements](../../research/t3code/cache.md) were slower than fresh checks. Avoid assuming reuse counts mean faster execution.
