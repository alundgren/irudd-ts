# Research execution

`runner.py` produces a per-test mutation matrix for an explicitly configured
trusted command. It does not install dependencies, select a framework, alter
product protocols, or execute commands found in the scanned repository.

The command requires Python 3 and Node with TypeScript stripping. Native
`node:test` reporting also requires the event identity fields validated by the
controls. A runner with missing fields fails as incomplete. The Vitest adapter
extends the SDK v1 reporter, so compatibility includes its shutdown APIs.

## Run a selected test slice

Supply an absolute installed Archguard binary and a config such as:

```json
{
  "sourceRoot": "/absolute/clean/source",
  "dependencyRoot": "/absolute/installed/source",
  "archguard": "/absolute/archguard",
  "subject": { "name": "selected-subject", "revision": "exact-git-commit" },
  "workspaceInclude": ["**"],
  "workspaceExclude": ["**/dist-electron/**"],
  "command": [
    "{root}/node_modules/.bin/vp", "test", "run",
    "--config", "vite.config.ts", "--reporter={reporter}",
    "selected/file.test.ts"
  ],
  "planConfig": {
    "schemaVersion": 1,
    "selection": { "include": ["selected/source.ts"] }
  },
  "timeoutSeconds": 60,
  "importControls": [
    {
      "specifier": "@workspace/package/subpath",
      "expectedWorkspacePath": "packages/package/src/subpath.ts"
    }
  ]
}
```

Replace the example paths and import specifier with actual selected inputs.
Inspect the entire explicit command and its test setup before execution. Native
Node commands use `--test-reporter={nodeReporter}` and explicit test paths.
Command arguments also support `{sdk}` and `{node}`. `cwd` is relative to the
owned source root. Configured environment values are hashed; keep credentials
out of the configuration and artifacts.

```sh
python3 research/test-value/runner.py run \
  --config /absolute/config.json --output /absolute/new-output-directory
```

Use `planPath` instead of `planConfig` to supply an existing complete Archguard
plan. The runner verifies each source SHA256, expected UTF-8 bytes and byte range
before applying its edit. A changed source hash prevents test execution.

`workspaceInclude` declares execution inputs separately from mutation selection.
Include tests, configuration, helpers, package manifests and lockfiles. The
default execution snapshot excludes `.git`, `node_modules`, `target`, `dist`
directories, `.repos`, `.t3`, Vite caches and local `.env` files. Dependencies
come only from `dependencyRoot`.
Selected tests and source do not establish whole-repository coverage.

An optional predeclared `mutantLimit` bounds the planned prefix and is retained
with the original plan size. It changes the experimental corpus; it is not
coverage pruning. The runner never stops a column after its first assertion.

## What completion means

Each baseline test has an ID containing project, relative file, the full name
hierarchy, and duplicate occurrence. Raw framework IDs are also retained. Per
test durations are optional, descriptive observations.

The baseline must finish with a nonempty active inventory and all tests passing.
Every accepted mutant must execute exactly that inventory once, without retries,
repetitions, skipped, pending, missing, extra or duplicate tests. An assertion
failure kills a mutant only when no import, hook, runtime, unhandled or teardown
error accompanies it. A complete passing column survives.

The research sidecar records individual tests. Its request identity and SHA256
bind it to the final SDK v1 result. After the child exits, `protocol-check.ts`
validates that result with the product SDK. The parent compares the actual exit
status, aggregate counts, raw assertion IDs and individual records. Matching
aggregate counts alone cannot establish a complete test inventory.

Timeouts, invalid edits and infrastructure failures retain `unknown` for every
baseline cell. Failed baselines produce evidence and `notRun` columns. They do
not produce mutation-score evidence. `matrix.json` is the analysis input;
`execution.json`, the raw result, inventory and process logs are the supporting
evidence. An empty baseline remains recorded even when downstream analysis
rejects its empty inventory.

## Owned copies and dependencies

The runner first copies the configured source into a hashed template. Each
execution copies that template into a new source tree and creates private HOME,
temporary, cache, config and data directories. Configured artifact paths can be
retained through `capturedArtifacts`. Execution sources are removed afterward
unless `keepSource` is explicitly enabled.

Installed external dependencies are copied once into an owned store. Dependency
links resolve within that store. Workspace source links are removed from the
store and recreated against each execution's source. Required workspace targets
must exist in the declared source snapshot. Generated package launchers are
copied into each execution and their installed-root paths are replaced with the
owned root. The runner never links the whole live `node_modules` directory.

`importControls` records Node's resolved URL and real filesystem path. A
declared workspace import must resolve to the expected file inside that fresh
source copy. Dependencies and source manifests, locks included when declared,
runner files, SDK, Node executable, Archguard executable, command and config
contribute to provenance. The final dependency-store hash must match its initial
hash; a changed store invalidates the matrix.

External modules share the copied store. They must not write that store or rely
on discovering source workspace packages through an external package's own
physical dependency directory. Such imports fail rather than reach live source.
This is trusted-command isolation, not protection against deliberately escaping
commands or undisclosed external state.

Disk checks run before copies and during subprocess execution. At or below 12%
free space the runner stops its captured process group and leaves evidence. A
timeout or output-budget failure also stops only that group. A process group
left behind after the command exits makes the execution incomplete. The leader
remains reserved through group observation and every signal. Unknown ownership
or uncertain group observation suppresses numeric signals and fails completion.
The CLI handles SIGINT and SIGTERM so command cleanup runs before it exits.

## Controls

```sh
python3 research/test-value/controls_runner.py \
  --output /absolute/new-controls-directory \
  --archguard /absolute/archguard \
  --installed /absolute/installed/vitest/repository
```

The optional `--installed` argument enables real Vitest controls. Without it,
native Node and dependency-copy controls still run. Controls cover independent
kill sets, duplicate names, skipped/missing/extra tests, assertion plus runtime,
imports, assertion correction, late teardown and process exit, retries, source
hash mismatch, workspace import ownership, and artifact-writing tests in both
mutation orders. They create owned fixtures and never edit the installed source.
Their durations are validation observations, not reserved benchmark measurements.
Process controls also cover a live descendant after leader exit, deadlines,
lost child ownership, delayed group observation and unavailable group inventory.

## Historical caller API

Import `execute_case` and pass an owned source template, a new evidence directory
and the explicit command. Its return contains `tests`, `complete`, `status`,
`outcomes`, `infrastructureErrors`, `protocol` and process metadata. Freeze
`fixed_result["tests"]` as `baseline` for faulty and corrected executions.
`dependencies` accepts one reusable `DependencyStore(...).copy()` instance.
Standalone callers can use `install_signal_handlers()` to route interruption
through owned subprocess cleanup. It returns previous handlers for restoration.
The caller must record revision identities, classify the replay method and
separate existing from fix-added tests. An assertion-only faulty replay accepted
against a fixed inventory is evidence for that selected test slice, not a full
historical dependency replay.
