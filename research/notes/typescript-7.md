# TypeScript 7 capabilities relevant to Archguard

Live research checked October 3, 2026. Published package metadata and the native API replay verify the released API. TypeScript 7.0.2 includes experimental `typescript/unstable/sync` and AST helper exports. Its programmatic API is not stable. The source repository's internal package template version is not the published package version.

The current [7.1 iteration plan](https://github.com/microsoft/TypeScript/issues/63703), updated September 12, schedules beta release on October 6. That is a plan, not a released version or a promise of API compatibility.

| Capability | Availability and comparison with 6 | Possible use here |
| --- | --- | --- |
| Public `Type.isErrorType()` | Verified through native 7.0.2. No equivalent predicate in the compared public 6 declarations. | Preserve the difference between intentional `any` and compiler error recovery. The semantic PR reproduction already exercises both. |
| Native sync/async clients, batched checker queries | Published experimental API. Type and symbol facts already existed in 6. | Measure individual versus batched property queries before choosing an optimization. |
| Explicit compiler snapshots and projects | Experimental 7 API arrangement. Version 6 already offered incremental programs and language services. | A later measured compiler-cache case could justify persistent snapshots. This does not replace the independent Rust source-graph cache. |
| Content mappers | Merged development work for 7.1. External transformations and source mappings can supply framework files and virtual sources. | Potential explicit trusted Vue or similar provider; no automatic execution from scanned repository configuration. |
| Emit and language-service API stabilization | The 7.1 plan includes both; version 6 already exposed emit and language services. | Treat new native API integration as an adapter upgrade requiring measured consumer cases, not a new rule capability by itself. |
| WebAssembly and Android ARM64 compiler builds | Listed infrastructure work in the 7.1 plan; not established by the pinned provider. | Possible future portable compiler provider; validate artifact availability and licensing before bringing it in. |
| Source-phase imports | [PR63915](https://github.com/microsoft/TypeScript/pull/63915), merged October 2 at `2f9fd09a104ef14268944a0aeb9cdc4d80abb6a6`; 7.1 work, not a 7.0.2 capability. | A future measured import-contract case should distinguish source imports from ordinary value imports. The initial compiler implementation does not resolve these source modules. |
| Ambient import-attribute matching | [PR63931](https://github.com/microsoft/TypeScript/pull/63931), merged September 1 at `253c5e2074a43301c93e34e5616c7249d86d71f6`; 7.1 work. | Typed import contracts can depend on attributes as well as a specifier. This does not establish bundler or runtime resolution. |

The best immediate addition is error-type classification. Most other checker facts are new to Archguard but existed in TypeScript 6. Native concurrency and reported compiler speedups need our own equivalent-workload measurements. Node 26 package maps remain an investigation in the iteration plan.

Primary references: [published 7.0.2 metadata](https://registry.npmjs.org/typescript/7.0.2), [7.0 release announcement](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/), [7.0.2 API declarations at its published source commit](https://github.com/microsoft/typescript-go/blob/2bd066d87f5bafd315be9f40889d0a60b9e58e0b/_packages/native-preview/src/api/sync/types.ts), [6 public declarations](https://github.com/microsoft/TypeScript/blob/v6.0.2/lib/typescript.d.ts), [content-mapper PR4712](https://github.com/microsoft/typescript-go/pull/4712).

No implementation is added merely because a capability appears in this list. Each requires a reproducible measured case and explicit version support. No upstream code was copied for this research.
