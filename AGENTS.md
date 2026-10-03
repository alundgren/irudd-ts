# Working on Archguard

Read [the architecture](docs/development/architecture.md) and [development setup](docs/development/README.md) before changing module boundaries or validation. Keep README focused on people adopting the tool; put agent instructions here and experiments under `research/`.

## Required checks

Run `scripts/check.sh` before a PR. It needs the explicit provider and historical installs documented in development setup. Do not add CI. Tests must cover a meaningful failure, its correction, and relevant negative controls. Run the CLI on `archguard.json` when changing SDKs, examples, or module organization, and update its import/role policies when paths change.

## Product contracts

- Keep syntax/source resolution, compiler semantics, configured repository policy, and runtime behavior distinct. Oxc and Oxc Resolver do not supply compiler types or runtime parity.
- Preserve parser errors and every import's resolution status. Exit 0 requires complete clean analysis; exit 1 means complete policy violations; exit 2 means incomplete analysis or invalid configuration.
- Execute only explicitly configured trusted plugin/provider commands. Never discover executable code in the scanned repository. Keep bounded transport and protocol validation shared.
- Keep Rust and TypeScript graph contracts equivalent. Version semantic facts separately from project facts. Preserve language-parity tests and reuse one TypeScript dependency query for many roots.
- Roles filter analyzed sources; they do not expand discovery. Companions require analyzed source files. Registry checks require static value import declarations and do not prove registry membership or execution.
- Required external resolution must preserve lookup failures and unsupported schemes. Do not infer whole-repository coverage from a complete selected dependency closure.
- Pin the semantic compiler and its unstable API explicitly. Never fall back silently. Reject missing inventories, mismatched hashes, unsupported backend identities, and unavailable required facts. Keep compiler diagnostics separate from policy findings. Update adapter, historical replay, completion controls, and SDK parity together when changing the pin.
- Cache only complete source graphs. Revalidate source bytes, discovery, resolver inputs, missing paths, inherited configs, package metadata, and symbolic links. Rerun all policies and plugins over reused facts. Reuse counts do not establish faster checks.

## Changes and review

Check licenses before adding dependencies or copying source. Keep locked license inventories current and retain upstream notices. Optional provider and research dependencies belong to their own installations.

Use direct, concrete language and follow the session's prohibited-word instructions. Use the `github-use` skill and `gh` for GitHub work. Obtain independent review before merging; use only branch and merge authorization provided for the current task. Do not carry an old task's branch or merge permission into new work.

For historical evidence, benchmark windows, and archived artifacts, follow [research/AGENTS.md](research/AGENTS.md).
