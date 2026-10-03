# Research and design choices

Checked on 2026-10-02; compiler-provider status updated on 2026-10-03. Archguard combines source architecture checks with an optional separately versioned compiler fact provider. It does not replace Effect-specific diagnostics or behavioral tests.

## Oxc and Oxlint

Oxc provides MIT-licensed Rust syntax, lexical binding and resolution libraries. The implementation pins Oxc crates to 0.152.0 and Oxc Resolver to 11.24.3. [Oxc](https://github.com/oxc-project/oxc), [Resolver](https://github.com/oxc-project/oxc-resolver).

Oxlint 1.86.0 already supports native import graph checks, JS plugins and TypeScript-Go-backed type-aware rules. The public JS interface is ESLint-compatible and its current `SourceCode.parserServices` is an empty frozen object. Internal workspace switching handles lint configuration rather than exposing a graph to third-party rules. This supports the user's observation about missing graph access, although plugins can still build their own graphs using filesystem access and another resolver. [JS API](https://oxc.rs/docs/guide/usage/linter/writing-js-plugins), [source contract](https://github.com/oxc-project/oxc/blob/7f65b757df7c3225320d8f7e051e64873f80f988/apps/oxlint/src-js/plugins/source_code.ts#L234), [native multi-file analysis](https://oxc.rs/docs/guide/usage/linter/multi-file-analysis).

The proposed value is a reusable project graph and repository facts for custom rules, with shared resolution and completeness handling. Basic cycle detection, restricted imports and ordinary local syntax checks are already available elsewhere. No claim that architecture analysis is impossible in an Oxlint plugin follows from the missing built-in graph API.

## TypeScript 7

TypeScript 7.0 shipped July 8, 2026 as a Go implementation. Microsoft reports substantial compiler speedups through native execution and parallel checking. Stable 7.0 does not ship the old programmatic compiler API. Projects needing it can retain TypeScript 6 alongside the native compiler. The stable registry version checked here is 7.0.2. [Release](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/).

The 7.1 plan schedules beta on October 6 and stable on November 24; those are plans, not released features. API stabilization, language-service APIs and additional compiler optimizations are in progress. Released 7.0.2 exposes experimental public JavaScript API entry points. The user selected these unstable APIs; the [semantic provider](semantic-provider.md) pins them and versions Archguard's contract separately. It uses no internal Go implementation packages. Updating the pin requires native replay and completion checks; planned 7.1 stabilization does not promise compatibility. [Iteration plan](https://github.com/microsoft/TypeScript/issues/63703), [API roadmap](https://github.com/microsoft/TypeScript/issues/63875).

Effect's MIT `@effect/tsgo` 0.48.0 adds semantic diagnostics, including Effect error/context checks and duplicate package diagnostics. T3 already patches its compiler with this tool. The standalone diagnostics command can emit JSON but repeats checking; prefer its existing compiler integration for those rules. It does not document a reusable general module graph. [Effect integration](https://github.com/Effect-TS/tsgo), [license](https://github.com/Effect-TS/tsgo/blob/main/LICENSE).

## Roslyn

Roslyn exposes the C#/VB compiler as reusable APIs. Syntax trees, compilations and symbols are immutable. A compilation combines source, references and options; a semantic model answers binding/type questions for a file. Workspaces represent solutions and projects and manage dependencies. Diagnostic analyzers run over these compiler objects and report findings through the build/editor diagnostic system. [Compiler model](https://learn.microsoft.com/en-us/dotnet/csharp/roslyn-sdk/compiler-api-model), [semantics](https://learn.microsoft.com/en-us/dotnet/csharp/roslyn-sdk/work-with-semantics), [immutable trees](https://learn.microsoft.com/en-us/dotnet/csharp/roslyn-sdk/get-started/syntax-transformation).

The useful design lesson is to build one project snapshot and let many analyzers query it. Archguard adopts that arrangement at a smaller scope: syntax facts, lexical imported-symbol provenance and resolved dependencies. Roslyn's type/data-flow guarantees do not transfer to Oxc facts. Archguard now has an [opt-in source graph cache](cache.md) and separate compiler-backed receiver/member/symbol facts. Compiler snapshot reuse and general data-flow queries remain future work. The measured graph cache is slower than fresh analysis and is not enabled by default. Roslyn itself is MIT licensed. [Source/license](https://github.com/dotnet/roslyn).

## Other tools

| Tool | License checked | Useful role | Decision |
| --- | --- | --- | --- |
| [dependency-cruiser](https://github.com/sverweij/dependency-cruiser) 18.5.0 | MIT | Mature resolved graph policies | Architecture baseline; do not claim basic boundaries are new |
| [Knip](https://github.com/webpro-nl/knip) 6.39.0 | ISC | Workspace unused files/exports/dependencies | Companion; T3 already uses it |
| [ESLint boundaries](https://github.com/javierbrea/eslint-plugin-boundaries) 7.2.0 | MIT | File groups and permitted dependencies | Configuration/behavior reference |
| [Sheriff](https://github.com/sheriff-arch/sheriff) | MIT | Module encapsulation and dependency policies | Architecture baseline |
| [ast-grep](https://github.com/ast-grep/ast-grep) 0.45.3 | MIT | Rust structural matching and rewriting | Consider for optional structural rule authoring; no graph/type checker |
| [Tree-sitter](https://github.com/tree-sitter/tree-sitter) 0.25.1 | MIT core; grammar licenses separate | Incremental multi-language parsing | Future language providers; not needed for initial TS facts |
| [Biome](https://github.com/biomejs/biome) 2.5.15 | MIT OR Apache-2.0 | Rust tooling, GritQL plugins | Reference, avoid adding another TS parser now |
| [GritQL](https://github.com/biomejs/gritql) | MIT in checked Biome fork | Structural query language | Future declarative option; verify exact package before adoption |
| [ts-morph](https://github.com/dsherret/ts-morph) 28.0.0 | MIT | Compiler API project inspection | Optional slower semantic integration; TS7 API migration matters |
| [ts-arch](https://github.com/ts-arch/ts-arch) 5.4.1 | MIT | Architecture assertions in tests | Rule/fixture reference |
| [Nx](https://github.com/nrwl/nx) | MIT | Tagged project graph boundaries | Architecture baseline for Nx projects |
| [Semgrep CE](https://github.com/semgrep/semgrep) | LGPL-2.1 engine; maintained rules have separate restrictive terms | Structural/taint rules | Do not import rules or embed it in this MIT core |

Licensing must distinguish an engine from its rules and hosted service. Semgrep's maintained rules have redistribution restrictions separate from the engine. None were adopted. [Rules terms](https://semgrep.dev/legal/rules-license/).

Scouts also verified MIT licensing for [tRPC](https://github.com/trpc/trpc) and [Hono](https://github.com/honojs/hono) as possible additional practice repositories. T3 is sufficient for the first experiments; adding targets is useful only when it tests a specific portability question.

## Plugin recommendation

| Approach | Advantages | Costs |
| --- | --- | --- |
| Rust plugins only | Native access to project facts, compile-time checks, no JS startup | Rule authors need Rust; static extensions need a build; Rust dynamic ABI is unstable |
| TypeScript plugins only | Familiar rule authoring for target teams, Node 24 runs stripped TS | Node deployment/startup, JSON transfer, smaller typed fact vocabulary than a compiler API |
| Both plugin languages | One analysis reused by fast native rules and accessible TS plugins | A versioned protocol and two SDK contracts need compatibility tests |

The CLI host stays in Rust in all three choices. Recommend supporting both plugin languages. Use built-in Rust rules for common graph operations and TypeScript for project-specific contracts over the same facts. Start with one subprocess per plugin per project, not one per file. This keeps the implementation auditable and avoids tying third-party plugins to Rust's ABI. Benchmark startup and serialization separately before introducing a persistent worker. Keep Effect-specific diagnostics with Effect's compiler integration. Archguard's public member facts do not implement Effect's complete error/context analysis.
