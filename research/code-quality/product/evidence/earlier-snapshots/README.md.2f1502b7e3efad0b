# Product evidence archive

These are bounded correctness experiments for the Archguard duplicate and mutation commands. Read the [product findings](../FINDINGS.md) and [run index](run-index.md). The archive retains initial, corrected and adverse reports with their original bytes. It does not establish full-repository coverage, a runtime improvement, native macOS execution, or approval of a later product revision.

The T3 source is frozen at `31a9da179ed0763335f05681c577474aec5d2309`. Each run's product revision, executable identity, explicit configuration and source hashes remain in its provenance. Labels such as `final` and `r12` are original experiment names. They do not identify the eventual delivery commit or mean that analysis was complete.

## What is retained

| Directory | Evidence |
| --- | --- |
| `raw/` | Full recorded product reports, plans, provenance, classifications, adverse outcomes and direct correction controls |
| `duplicate-fixtures/` | All A through F comparisons under default, erased-property, erased-local and preserved-literal settings, with threshold zero |
| `duplicate-fixtures-coordinator-final/` | Separately labeled current-CLI replay of the same twenty-four comparisons when available |
| `sources/` | Exact authored fixture text, initial domain source, selected frozen T3 source/tests, native editor sample, lockfile and licenses |
| `profiles/` | Explicit profile configurations, supplemental tests, runner adaptation and fixed instrumentation copies. State and installations are excluded |
| `proofs/` | Per-mutation path and SDK source/test/output copies, SDK eleven-to-eighteen-test controls, and selected host-boundary review evidence |
| `logs/` | Execution pressure control, execution delivery checks, selected syntax failure/correction controls, SDK controls and an earlier required-check log |
| `reviews/` | Independent classification notes captured during the experiment |
| `validation/` | Package or later validation provenance supplied explicitly during archive capture |
| `visual/` | Two bounded final browser PNG captures; draft images are excluded |
| `tools/` | Exact archive updater source for adding finished reports and regenerating inventories |
| `recovery/git-cleanup/` | Selected journals, assigned requests, original/mutated source and configuration from two uncertain-cleanup Git runs, without their dependency copies |
| `earlier-snapshots/` | Earlier exact bytes if a later capture changes an already retained artifact |

`raw/report-template.html` is an original research report template, not a generated product result. Its bytes are preserved. The older reference-tool and mature-tool comparison archive remains in its original location and is outside this archive.

The initial domain fixture called an addition/subtraction-of-zero mutation equivalent. Negative zero disproved that claim. The initial source/report and corrected fixture/report remain visible. Path corrections retain the two useful behavior tests, equivalent cases and unresolved drive-root representation question. CSV, Git and SDK outcomes retain runtime errors, timeouts, cleanup failures and cancellation. The host profile's 153 survivors require inspection of the selected tests and public behavior; their count is not 153 proven defects.

## Integrity and report claims

Run the standalone verifier with Python's standard library:

```sh
python3 research/code-quality/product/evidence/verify.py
```

It verifies the exact retained-file inventory, byte counts, SHA-256 values, checksum list, selected report summaries and source/report hash links. It also independently counts retained mutation outcomes, reuse and execution records. The eleven SDK correction controls check original byte edits, mutated-source identities, passing baselines, original and corrected test outcomes, TAP counts and output hashes. Omitted results and problems remain explicit. `complete: false` stays false even when every planned site was attempted. A complete report must have a passing baseline and no recorded incomplete outcomes or omitted evidence.

`manifest.json` lists every retained file except itself and `SHA256SUMS`. `SHA256SUMS` covers the manifest and every other file except its own checksum. This catches accidental edits and missing files; it is not a signed authenticity claim. `report-summary.json` copies checkable report values. When the full host-survivor classification is retained, the verifier checks its totals, exact mutation edits and coverage of the original survivor IDs against the source report and direct probes. It does not certify the interpretation of equivalent mutations or the correctness of a test oracle.

The reporter checks bind all 85 original survivor classifications to the original 154-site report, preserve its three incomplete timeout outcomes, and verify eight original-five-test/strengthened-V3 proof pairs. They check exact byte edits, source and test hashes, Node test counts and statuses, a twelve-test passing baseline and a separate timed-out-site witness. The later unchanged-input complete reuse report remains a separate artifact. Classification decisions require their source explanations; matching output alone proves no equivalence.

## Reproduction and path relocation

Use the [reproduction helpers](../README.md) and the current [authored profiles](../../../../examples/code-quality/README.md) for fresh runs. Build the current release CLI with `cargo build --release --locked --bins --examples`. Record the resulting product revision and executable hash as a new experiment. A current executable need not reproduce old report bytes or old failures.

The raw command arrays and configurations still contain the original absolute experiment paths. [relocations.json](relocations.json) maps retained files to archived copies. Moving a report does not move its input root, dependency installation, output path or state directory. Recreate those paths in owned temporary directories, or explicitly change configuration paths and record the new configuration hash. Preserve the frozen T3 revision when replaying its source. Never treat an excluded dependency installation or mutable current checkout as interchangeable with the originally hashed input.

Archived source, tests and selected JavaScript/Python proof tools are text. No archived fixture is executed by the verifier. Generated `support/` copies are explicit instrumentation inputs and contain the recorded runner adaptation. They are not the installed dependency tree. The eleven-to-eighteen-test SDK controls use direct Node commands and exact input copies; they are separate from the full mutation backend replay. Their root driver source is retained once. Recreate each recorded byte edit against the original source hash before using its test command. Failure to reconstruct an original runtime, installation or temporary path limits reproduction and must be recorded.

Logs are byte snapshots. A log's final-looking test line alone does not establish that its enclosing command exited successfully. The earlier `archguard-quality-product-cli-reviewed-required-check.log` records an interrupted check. Use separately retained process provenance or a later recorded command result for completion claims. Resource observations in the pressure log apply to that Linux control and its explicit limits, not every target command.

## Exclusions and pending work

[exclusions.json](exclusions.json) records deliberate exclusions and unavailable named inputs. Installed dependencies, compiled binaries, `target/`, reusable state stores and private runtime workspaces are excluded. Two failed Git runs have an explicit exception for minimal forensic journal, request, configuration and source text. Those copies cannot restore live process ownership or a usable execution workspace. Copied source retains upstream licenses. Repeated SDK driver source copies and unrelated intermediate build logs are excluded.

Refresh from retained local experiment inputs with the archived updater:

```sh
python3 research/code-quality/product/evidence/tools/update.py \
  --repository /absolute/archguard \
  --raw-root /absolute/original/research/local/code-quality-product \
  --include-log /absolute/finished-check.log \
  --include-proof /absolute/final-validation-provenance.json
```

Omit optional `--include-log` and `--include-proof` arguments when there is no new validation artifact. The named experiment directories under `/tmp/` are original input locations. Unavailable directories are recorded as exclusions. Add finished reports at the original raw location before refreshing. The updater keeps earlier bytes when a captured input changes, rebuilds inventories and verifies the result. Run it only against explicitly trusted text inputs. Updating hashes records a new capture; it does not establish that a changed result is correct.

The archive updater can add newly finished reports without replacing an unfavorable earlier snapshot. Process-only records remain process records. At initial assembly, the reporter and resumed CSV mutation runs were still in progress. The resumed CSV report is now retained with 52 attempted sites, 50 assertion kills and two timeouts, so it remains incomplete. Its full declared-input digest differs from the cancelled run despite identical configuration and executable hashes. The old individual inventory was not retained, so the evidence does not identify which input changed. Consult the run index and provenance for later outcomes.

## Fresh VM handoff

The committed product, [profile generators](../README.md), authored examples and this archive are the durable inputs. Original `/tmp` paths and ignored output directories may no longer exist. Recreate a separate trusted T3 checkout at the exact frozen revision, install its locked toolchain using its upstream development instructions, and supply an explicit self-contained Vite Plus dependency directory to the committed profile generators. The archive retains T3 package/lock text, exact selected source/tests, supplemental controls and SDK instrumentation text. Optional compiler and historical installs use the commands in [development setup](../../../../docs/development/README.md). Build the current release CLI before a new runtime reproduction and record new source, configuration, runtime and executable hashes.

The cleanup correction is committed at `e2f3d3740253578d5d66e9bb847c4f8d8f19975a`. `sources/cleanup-e2f3d37/` retains its source inventory, subprocess implementation, reporter tests and license. Earlier executable hashes remain the identities of earlier runs. The correction allows five seconds for process-group teardown while retaining the separate 250-millisecond pipe deadline. Its fifteen focused subprocess controls passed. The corrected Git replay now retains all 26 attempted sites, 22 kills, three survivors and one loop timeout after a passing 22-test baseline, without cleanup errors. Source preservation and journal removal are recorded separately. Its timeout keeps the analysis incomplete. The final required repository check exited zero, with 31 Node controls, license inventories, 54 maintained Markdown checks and a self-policy run reporting no violations or analysis problems. Its completed log is retained under `logs/archguard-quality-product-cleanup-final-required-check.log`. Keep both earlier Git cleanup failures visible when evaluating that correction.
