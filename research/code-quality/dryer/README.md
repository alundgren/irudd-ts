# TypeScript structural similarity experiment

This directory contains an isolated research comparison for coding-agent review. A report means two functions have structural similarity under the recorded normalization. It does not require a refactor, prove shared behavior, or establish runtime equivalence. Nothing connects this installation to the product CLI or CI.

The external dryer checkout is pinned at `6892667b3441b88379bc8d0439fc2152b0fdb341`. The author's post supplied for this task explicitly invites agents to customize the tools or write alternatives. That supports the intended research adaptation. The checkout has no license file or declared package license at this revision, which we retain as metadata. Its code is inspected and executed outside this repository; no dryer implementation is copied here. Technical reliability drives the findings.

The established comparison is the jscpd 4.3.0 TypeScript engine. Version 5 uses a Rust engine, so this experiment deliberately chooses version 4. TypeScript 5.9.3 supplies the independent syntax parser. The independent detector is original experiment code and uses no compiler types or dependency resolution.

## Reproduce

Use clean external checkouts. GitHub operations use `gh`.

```sh
gh repo clone unclebob/dryer /tmp/dryer-upstream
git -C /tmp/dryer-upstream checkout --detach 6892667b3441b88379bc8d0439fc2152b0fdb341
gh repo clone pingdotgg/t3code /tmp/dryer-t3
git -C /tmp/dryer-t3 checkout --detach 31a9da179ed0763335f05681c577474aec5d2309
npm ci --prefix research/code-quality/dryer --ignore-scripts --no-audit --no-fund
uv venv --python 3.12 /tmp/dryer-venv
uv pip install --python /tmp/dryer-venv/bin/python --require-hashes -r research/code-quality/dryer/python-requirements.lock
npm test --prefix research/code-quality/dryer
python3 research/code-quality/dryer/runner_test.py
```

Build the repository release binaries before a recorded run, as required by research instructions. Reserve the measurement window across agents. The parser's first use downloads the pinned language-pack 1.20.0 grammar; warm that download before timing.

```sh
cargo build --release --locked --bins --examples
PYTHONPATH=/tmp/dryer-upstream/src /tmp/dryer-venv/bin/python -c 'from dryer.treesitter import parse; parse("function f(){ return 1; }", "typescript")'
python3 research/code-quality/dryer/run.py \
  --dryer /tmp/dryer-upstream --python /tmp/dryer-venv/bin/python \
  --t3 /tmp/dryer-t3 --out research/local/dryer-trial
```

The output directory must not already exist. The runner verifies clean revisions and every selected T3 source hash before executing. It verifies both checkouts remain clean afterward. It runs dryer through its unmodified CLI and through an observation wrapper, the independent detector, and jscpd on the same file allowlist. Malformed input has a separate trial. Tool errors stop the run, and raw output remains available. A CLI exit of 0 from upstream dryer means it wrote a report, including when input was malformed.

Run only the independent detector with explicitly named files:

```sh
node research/code-quality/dryer/detector.mjs \
  research/code-quality/dryer/fixtures A.ts B.ts C.ts D.ts E.ts F.ts
```

It exits 0 for complete syntax analysis, 2 for malformed or unreadable input. It does not issue a policy-violation exit code. Its JSON retains file, function name, line range, size exclusions, diagnostics, all pair scores, and threshold sensitivity.

## Frozen inputs and comparisons

[corpus.json](corpus.json) selects ten real T3 files before comparison, with exact hashes and per-file reasons. These are full selected modules at the pinned revision. Tests, other modules, and installed dependencies are excluded. This bounded corpus does not represent the whole repository.

| Control | Intended review question |
| --- | --- |
| A | Detect local renames and numeric literal changes. |
| B | Detect a copied implementation with an added guard and audit call. |
| C | Avoid treating a different implementation of the same calculation as a copy. |
| D | Show unrelated account and battery behavior sharing syntax after property erasure. |
| E | Show deliberate resource-request boilerplate. |
| F | Detect a 45-line authored copy with local renames, a changed condition, and changed numbers. |

Controls are authored examples, not copied T3 bugs. Manual interpretation uses useful, incidental, or unclassified. C can share the business concept while differing structurally; clone scores cannot settle design intent. E is deliberate boilerplate, so a similarity report alone offers little review value.

Both structural detectors start with four lines and twenty normalized nodes; they count different syntax representations. jscpd uses four lines, fifty tokens, and its explicit `mild` mode. That mode excludes whitespace and newlines and keeps names/literals. These checks are different, so elapsed observations do not support a speed ranking.

The upstream score is set Jaccard over every normalized subtree and atom. The wrapper also computes occurrence-count Jaccard and an experimental occurrence-count score weighted by subtree node count. The independent detector computes the same three formulas over its own TypeScript syntax representation, both erasing and preserving ordinary member names. Its default report preserves member names. Neither representation models variable binding, types, side effects, or execution.

Weights can penalize small edits heavily because an edit changes every ancestor subtree. Set scores discard repetitions and shared generic atoms can dominate short functions. Treat the extra metrics as diagnostic comparisons, not tuned acceptance rules. All exact scores and memberships should be read from the retained raw JSON.

## Licensing and retained evidence

[dependency-licenses.json](dependency-licenses.json) inventories 119 installed npm packages and two Python packages. [DEPENDENCY-NOTICES.txt](DEPENDENCY-NOTICES.txt) retains their notices. Regenerate after a deliberate dependency change:

```sh
python3 research/code-quality/dryer/licenses.py --python /tmp/dryer-venv/bin/python
```

The language-pack downloads its grammar binary separately. Each run records the downloaded release manifest, bundle hash, and loaded grammar library hash. The Python distribution declares MIT; this experiment does not redistribute the downloaded grammar binary. T3 source fragments emitted in raw jscpd JSON carry [T3-NOTICE.txt](T3-NOTICE.txt). No T3 source checkout is modified.

New runs go under ignored `research/local/`. Retained evidence includes raw stdout/stderr, jscpd JSON, input and implementation hashes, revisions, runtime versions and executable hashes, grammar artifact hashes, commands, and one elapsed observation per invocation. Elapsed values come from a shared development host and are not controlled benchmarks.

Read [FINDINGS.md](FINDINGS.md) for the measured scores, failures, source inspection, and limitations. Final corrected raw bytes are retained under [evidence/run3](evidence/run3/provenance.json). The superseded [run2](evidence/run2/provenance.json) retains its earlier measured implementation; [evidence/run1](evidence/run1/provenance.json) preserves the initial runner execution error. `provenance.json` includes ancillary run-time documentation/license/test snapshots alongside consumed implementation and configuration inputs. Prose added after measurement does not change the measured code or fixtures.
