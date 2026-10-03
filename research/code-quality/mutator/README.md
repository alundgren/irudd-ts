# Mutation research prototype

This directory runs a bounded comparison of the pinned external mutator and Stryker with Vitest. It produces per-site review input. It does not set a quality gate, integrate into CI, or modify the product CLI.

The targets were selected before trials in `targets.json`. `fixtures/account.ts` is an authored order and integer-credit workflow. Five weak behavior assertions intentionally miss a shipping boundary, mixed payment flags, nonzero addition, initial eligibility, and the exact service fee. `corrected.test.ts` adds behavior assertions and negative controls. Subtracting zero remains equivalent for the specified integer-credit business values, so a perfect score would be the wrong goal.

The real-source target is T3 Code's calendar helpers at `31a9da179ed0763335f05681c577474aec5d2309`. `evaluate.py` copies source bytes into a disposable package and changes only the test import from `vite-plus/test` to `vitest`. This is a reduced reproduction. It does not establish behavior across the mobile application. The MIT notice is in `T3-NOTICE.txt`.

## Reproduce

Run from the repository root. Clone the external tools and T3 into directories outside this repository, then check out the exact revisions. The experiment runs mutator and crapper in external checkouts. Neither checkout supplied a formal license file or package license field at the inspected revisions. The user supplied the author's invitation to customize these tools or write alternatives; this supports adaptation and is not a research blocker. External execution here preserves the exact evaluated implementation.

```bash
gh repo clone unclebob/mutator /tmp/mutation-research/mutator
git -C /tmp/mutation-research/mutator checkout bb9262a7b1d884a446bc14dc56568151dab02d71
gh repo clone unclebob/crapper /tmp/mutation-research/crapper
git -C /tmp/mutation-research/crapper checkout ce3895211e407e5d51d71da711d8a8670a05f7cf
gh repo clone pingdotgg/t3code /tmp/mutation-research/t3code
git -C /tmp/mutation-research/t3code checkout 31a9da179ed0763335f05681c577474aec5d2309
uv venv --python 3.14 /tmp/mutation-research/venv
uv pip install --python /tmp/mutation-research/venv/bin/python --constraint research/code-quality/mutator/python-constraints.txt --build-constraint research/code-quality/mutator/python-constraints.txt -e /tmp/mutation-research/mutator -e /tmp/mutation-research/crapper
npm ci --ignore-scripts --prefix research/code-quality/mutator
/tmp/mutation-research/venv/bin/python -c "from tree_sitter_language_pack import download, get_parser; download(['typescript']); get_parser('typescript')"
/tmp/mutation-research/venv/bin/python research/code-quality/mutator/evaluate.py --mutator /tmp/mutation-research/mutator --crapper /tmp/mutation-research/crapper --t3 /tmp/mutation-research/t3code --output research/local/mutation-trial
python3 research/code-quality/mutator/summarize.py research/local/mutation-trial
/tmp/mutation-research/venv/bin/python research/code-quality/mutator/stryker_cache.py --t3 /tmp/mutation-research/t3code --output research/local/mutation-stryker-cache
```

`evaluate.py` refuses an existing output directory and mismatched or dirty external checkouts. Keep new output in ignored `research/local/` until reviewing its provenance. Installations and builds are outside measured runs. Reserve one timing window for the evaluation; use one mutation worker. A repository release build must finish before recording measurements, as required by `research/AGENTS.md`.

The npm lock pins Stryker and its Vitest runner to 10.0.0 and Vitest and coverage-v8 to 4.1.11. Node 24.21.0 and Python 3.14.4 were used. Python parser versions and build constraints are explicit. `license-inventory.json` covers locked npm packages, Python parser packages, and external checkouts. The parser package may download its pinned grammar bundle during first preparation; initialize its TypeScript parser before the timing window.

## How to read the output

Upstream reports `killed`, `survived`, and `uncovered`. It treats every nonzero command exit and timeout as killed, including invalid syntax and configuration errors. The retained `calls` record preserves exit code, timeout flag, output, duration and an independent `observedStatus`. Only recognized Vitest assertion failures qualify as observed kills after the explicit execution reporter confirms there are no suite, hook or unhandled errors. Missing reporter records or ambiguous output stay execution errors. A suite or command error stays an execution error. No translation changes the upstream report.

The reliability controls deliberately exercise a failed baseline, failing worker command, timeout, invalid mutant and raised runner exception, then verify original bytes and rerun a passing baseline. Cache probes compare differential reuse with a forced run after test, config, helper and dependency edits. Changed-file selection records whether test edits identify production targets. There is no implicit importer expansion.

`summarize.py` retains both operator inventories and the original/replacement text, site, status, classification and reproduction command for survivors. Compare matching sites and replacements. The two tools have different operator sets, so their total scores do not measure the same cases. Review input is a starting point for an agent or person to inspect the behavior contract and propose a test; it is not permission to add assertions about irrelevant details.

Stryker's [incremental documentation](https://stryker-mutator.io/docs/stryker-js/incremental/) says its dry run remains required and Vitest reports test locations. It also documents that helpers outside mutated/test files, configs, dependency updates and environment changes are not detected. A safe changed-scope experiment should force reruns or use an external digest covering those inputs. A cache reuse count alone does not show lower runtime.

Read `findings.md` for the observed outcomes and limitations. `evidence/manifest.sha256` covers retained raw and derived evidence. Stryker cache records are in `evidence/stryker-cache.json`; they were recorded in a separate sequential phase of the same reserved window. The T3 correction demonstrates a candidate minute-precision contract. A reviewer must confirm that milliseconds matter before requesting a test.

Verify the retained results and report adapter without running mutations:

```bash
python3 research/code-quality/mutator/verify_evidence.py research/code-quality/mutator/evidence
python3 -m unittest discover -s research/code-quality/mutator -p 'test_*.py'
cd research/code-quality/mutator/evidence
sha256sum -c manifest.sha256
```

The report adapter uses one-based Stryker columns measured in UTF-16 units. Its focused controls cover recorded boundary and boolean text, multiline text, an astral Unicode character, and invalid positions. An initial off-by-one derived excerpt error was corrected after review; raw reports were preserved. The correction is recorded in `retention.json`.

The initial observer incorrectly accepted a failing assertion when another suite or runtime also failed. `classifier-correction.json` retains before/after results for actual Vitest controls. `execution-reporter.js` records suite, hook and unhandled errors that JSON alone may omit. The final observer rejects missing execution metadata, startup/import errors, unrelated runtime exceptions, unhandled errors, teardown failures and mixed failures. `initial/` preserves the first comparison and original evaluator bytes; the final comparison was rerun with the corrected observer.

Reproduce the focused reliability controls in a new ignored directory:

```bash
/tmp/mutation-research/venv/bin/python research/code-quality/mutator/classifier_controls.py --t3 /tmp/mutation-research/t3code --output research/local/mutation-classifier-controls
```
