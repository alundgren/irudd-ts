# Installed source resolution

Archguard can require external package lookup with `requireExternalResolution: true`. The facts still use protocol version 1 and report `resolution.mode: "installed-source"`. A failed lookup is an unresolved edge and an analysis problem. Node builtins remain external. Dependency package source and assets are graph endpoints. The default `source` mode continues to permit uninstalled external packages and retains lookup errors in edge details.

This checks the selected source graph. It does not check TypeScript types, reproduce a compiler's declaration lookup, load Metro configuration, or reproduce runtime resolution. An installed project can still have missing generated files, custom resolver mappings and nonliteral dependencies.

## Pinned installation and source inventory

The upstream repository is [T3 Code](https://github.com/pingdotgg/t3code), pinned to `e0db2a5e58bcbe7d738bca7667d2440ddb83e30f`, the merged commit of [PR #13295](https://github.com/pingdotgg/t3code/pull/13295). The current-revision scans are coverage controls and do not claim to diagnose that PR's server behavior. Its manifest requires Node `^24.13.1` and pnpm `11.10.0`. We used Node `24.21.0` and the exact pnpm package, verified against its registry SHA-512 integrity. pnpm and the inspected Vite+ generator tooling are MIT licensed, used externally and not redistributed. [Install audit](installed/install-audit.json) records tool licenses, input hashes and generated outputs.

We created an isolated detached worktree and ran:

```bash
node /tmp/archguard-next/tools/pnpm/bin/pnpm.cjs install \
  --frozen-lockfile --ignore-scripts --ignore-pnpmfile --reporter=append-only
```

The install passed without changing tracked files, the lockfile or workspace configuration. The root `prepare` command, Effect compiler patch, project setup and native build scripts did not run. [pnpm's install documentation](https://pnpm.io/cli/install) defines the frozen-lockfile and ignore-scripts options.

The inventory includes every tracked TypeScript and JavaScript file under `apps`, `packages`, `infra`, `scripts` and `oxlint-plugin-t3code`, plus the root `vite.config.ts` needed by server build configuration. It includes tests, declarations, application configuration and every platform variant. There are 3,972 files. `.repos`, `node_modules`, native code and GitHub scripts are outside this inventory. The replay harness asserts that the neutral, Node-condition and browser-condition profiles select exactly that file set.

| Profile | Files | Imports | Problems | Exit |
| --- | ---: | ---: | ---: | ---: |
| Whole inventory, `types/import/default` | 3,972 | 29,438 | 131 | 2 |
| Whole inventory, `node/import/default` | 3,972 | 29,438 | 28 | 2 |
| Whole inventory, `browser/import/default` | 3,972 | 29,438 | 28 | 2 |
| Mobile and packages, Android | 1,364 | 8,252 | 12 | 2 |
| Mobile and packages, iOS | 1,346 | 8,171 | 14 | 2 |
| Explicit server source closure | 1,012 | 9,790 | 0 | 0 |

[Raw records](installed/measurements/installed.json) retain every problem, status count, excluded tracked source, source corpus hash, executable hash, configuration hash and normalized facts hash. The mobile profiles use `react-native/import/default`, explicit platform suffix ordering and inactive-platform exclusions. These are configured source profiles, separate from adoption policy presets. Neither their condition names nor their suffix order establish Metro equivalence.

The server control starts with all 864 tracked files under `apps/server/src` and adds their 148 analyzed source dependencies, including type imports. We recorded that exact file list as the `server-closure` profile and then checked it afresh with required package resolution. This is an explicitly selected large clean control. Archguard does not automatically expand selectors to dependency closure.

## Remaining incomplete analysis

The old installed baseline selected 3,971 files and retained ten problems while classifying 126 failed external lookups as external. [Baseline data](installed/measurements/legacy-baseline.json) preserves those errors. Adding the root Vite configuration corrects one excluded source edge. Required package resolution exposes the remaining failures rather than treating installation as proof of completeness.

The whole Node/browser condition scans still contain:

- Thirteen GNOME `gi://` and `resource:///` host imports. Archguard does not implement that host resolver.
- Two desktop `.cjs` build outputs that are absent from the source checkout.
- Two mobile virtual module identifiers configured by Metro's `extraNodeModules`.
- Three erased `mdast` imports requiring compiler declaration lookup, and one erased Tabler `/types` import with no selected source export.
- Seven nonliteral dependencies that cannot identify a target from syntax alone.

The `types` condition additionally selects missing Tabler per-icon declaration targets. Switching to an explicitly configured source condition resolves 103 of those edges through present runtime export targets. Archguard never retries a different condition to conceal a missing configured target. Type-only imports remain type-only facts even when a source profile resolves their specifier.

Mobile profiles also retain imports into inactive platform files and configuration helpers outside their selected roots. Excluding an inactive implementation does not erase an explicit import of it. These facts explain the mobile problem counts; none of these profiles claims a complete clean graph.

## Generated inputs

The Uniwind theme JSON and CSS inputs are tracked and were present after installation. The device-stream and mobile license modules were absent because Metro generates them at startup. We inspected the generator functions and called them directly in the isolated installed worktree, without loading Metro configuration or starting its watchers:

- `generateDeviceStreamScript()` from `apps/mobile/scripts/generate-device-stream.mts` invokes installed Vite+ with `configFile: false` and `write: false`, then writes the generated module and package manifest. Its output module was 83,159 bytes.
- `generateThirdPartyLicenseManifest()` from `scripts/lib/third-party-licenses.ts` used the mobile manifest, the tracked license configuration and `allowMissingGeneratedNotices: true`, matching Metro's development behavior. A small wrapper wrote its result and package manifest. Its 661-entry module was 1,070,570 bytes. This option skips missing generated SPDX notices and is not a complete redistribution-license audit.

The [audit](installed/install-audit.json) records exact output hashes. Generating these files does not implement Metro's custom mappings. Their import identifiers still remain unresolved in Archguard, and the generated JavaScript is not part of the tracked source inventory. No generated-source completeness claim is made.

## Merged platform-change replays

These are full selected mobile/workspace source replays from actual merged PRs, with retained incomplete-analysis problems. We did not substitute synthetic examples or claim complete clean scans.

[PR #12380](https://github.com/pingdotgg/t3code/pull/12380) changed `remoteRegistration.ts` from a value import of `widgets/AgentActivity.tsx` to a platform adapter. Its actual merged parent is `e0649ed7d8f8b5f21d114ff2b09ef803184f1d05`; the fixed commit is `53830d413474b2a5749c040d4382c96002894b88`. The PR API's current base reference was not that parent, so it was not used for the replay.

| Revision and profile | Files | Imports | Resolved static value path to AgentActivity | Problems |
| --- | ---: | ---: | --- | ---: |
| Before, Android | 1,246 | 7,804 | Present, direct import | 8 |
| Fixed, Android | 1,247 | 7,807 | Absent | 8 |
| Before, iOS | 1,238 | 7,765 | Present, direct import | 9 |
| Fixed, iOS | 1,240 | 7,769 | Present, through `.ios.ts` adapter | 9 |

The fixed Android type-inclusive graph still reaches AgentActivity. This control confirms that the measured change concerns value imports and does not discard the retained type dependency. The absent value path applies to resolved static edges in this selected inventory. Unresolved and unsupported edges could introduce paths unavailable to the analysis. Every historical scan exits 2 because incomplete-analysis problems remain. The assertion does not measure module execution, native application size or the dependency package's internal source graph.

[PR #12381](https://github.com/pingdotgg/t3code/pull/12381) then adds `HomeHeader.android.tsx`. Its parent is the PR #12380 fixed commit; its fixed commit is `7e7cd32463b528838cb10ef6b89ef6ceac1f7a4c`. The full replay confirms that `HomeRouteScreen.tsx` resolves `./HomeHeader` to the Android file on Android and the generic `.tsx` file on iOS. The selected inventories are 1,249 Android files and 1,241 iOS files, with the same eight and nine problems respectively.

The historical dependency set was installed once at the actual parent with the same frozen, no-lifecycle command. All 37 relevant manifests, TypeScript configurations, the workspace configuration and lockfile are byte-identical across the three historical revisions. Source checkouts were sequential and reused only these verified installed inputs. [Historical evidence](historical-evidence.json) records source hashes and exact commits. `python3 research/t3code/scripts/verify_history.py /path/to/t3code` verifies those source references.

## Reproduction and measurement

Install each pinned input set as above in a detached worktree. Build both the CLI and examples with the locked release command:

```bash
cargo build --release --locked --bins --examples
python3 research/t3code/scripts/installed_resolution.py /path/to/installed-t3code \
  --cli target/release/archguard --archguard-revision YOUR_SOURCE_COMMIT \
  --case installed --profiles inventory node browser android ios server-closure \
  --measure --runs 7 --warmups 1 --output /tmp/installed-resolution.json
```

Use `--case 12380-before`, `12380-fixed` or `12381-fixed` with `--profiles android ios` at their exact recorded revisions. The harness refuses another upstream revision, tracked modifications, mismatched neutral inventories or failed platform controls. It compares facts byte-for-byte between repeated runs. Without `--measure`, it records correctness data without performance samples.

Samples measure fresh CLI processes through facts serialization and stdout capture. One warmup precedes seven samples; filesystem caches are warm. Source inventory hashing and JSON report processing are outside the timed subprocess. All raw samples are retained. These are measurements of each configured scan. The old CLI cannot request the new package-resolution requirement, and the historical PRs change the selected file counts, so these results do not establish a before/after tool speed comparison.

| Scan | Minimum ms | Median ms | Maximum ms |
| --- | ---: | ---: | ---: |
| Current whole inventory | 4,893.50 | 5,326.61 | 5,656.18 |
| Current Node conditions | 4,045.29 | 5,399.76 | 5,981.10 |
| Current browser conditions | 4,080.63 | 5,706.15 | 7,609.63 |
| Current Android | 1,430.14 | 1,792.07 | 2,030.58 |
| Current iOS | 1,605.66 | 1,693.63 | 2,240.72 |
| Current server closure | 3,238.39 | 3,409.31 | 3,631.89 |
| PR #12380 before, Android | 1,244.22 | 1,371.17 | 1,820.92 |
| PR #12380 fixed, Android | 1,433.55 | 1,535.63 | 1,868.36 |
| PR #12380 before, iOS | 1,282.75 | 1,376.66 | 1,653.06 |
| PR #12380 fixed, iOS | 1,350.09 | 1,549.60 | 2,138.48 |
| PR #12381 fixed, Android | 1,707.95 | 2,254.21 | 2,759.20 |
| PR #12381 fixed, iOS | 1,809.87 | 2,065.97 | 2,383.25 |

The retained ranges include slower fixed-revision scans and the high browser-profile sample. The release executable SHA-256 is `a07931c05061ba669903e321036554720cf3fa04b510b280f9e794d342e0c9f4`, built from source commit `0a625897a578e03bd255407c7dc437a7c4372fd1`. Documentation and measurement-record commits do not change that executable's source provenance.
