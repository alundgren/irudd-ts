"""Execution and source-selection controls for the external upstream runner."""
import dataclasses
import shutil
from pathlib import Path
from evaluate import RecordingRunner, run, setup, source_hashes, upstream


def probe(root, t3):
    setup(root, 'corrected', t3)
    before = source_hashes(root)
    results = {}
    for label, worker_action in [('harness-error', 'raise SystemExit(12)'), ('timeout', 'import time; time.sleep(5)')]:
        # The root command succeeds. Only mutant workers take the failure path.
        command = 'python3 -c "import pathlib; ' + "exec(" + repr(worker_action) + ") if 'worker-' in str(pathlib.Path.cwd()) else None\""
        results[label] = upstream(root, 'src/account.ts', lines={16}, command=command)
    from mutator.functions import sites_in_file
    from mutator.workers import run_mutants
    target = root / 'src/account.ts'
    original = target.read_bytes()
    site = next(s for s in sites_in_file(original.decode(), target, root, 'src/account.ts') if s.line == 16)
    invalid = dataclasses.replace(site, mutant='(', category='deliberate-invalid-control')
    runner = RecordingRunner()
    baseline = runner.run('./node_modules/.bin/vitest run --reporter=json --reporter=./execution-reporter.js', root, None)
    outcomes = {}
    run_mutants(root, target, original, [invalid], 1, runner,
        './node_modules/.bin/vitest run --reporter=json --reporter=./execution-reporter.js', root, 5, 'src/account.ts', outcomes)
    results['invalid-mutant'] = {'baseline': dataclasses.asdict(baseline), 'site': dataclasses.asdict(invalid),
        'upstreamOutcomes': outcomes, 'observedStatus': 'invalid-mutant', 'calls': runner.calls}

    class RaisingRunner:
        verbose = False
        def run(self, command, cwd, timeout):
            raise RuntimeError('deliberate runner exception')
    try:
        run_mutants(root, target, original, [site], 1, RaisingRunner(), 'unused', root, 2, 'src/account.ts', {})
    except RuntimeError as error:
        results['exception'] = {'error': str(error), 'workersRemoved': not any((root/'target/mutation-workers').glob('run-*'))}
    if source_hashes(root) != before:
        raise AssertionError('source bytes changed after exceptional probes')
    results['sourceRestored'] = True
    results['baselineAfterExceptions'] = run(['./node_modules/.bin/vitest', 'run', '--reporter=json'], root)
    if results['baselineAfterExceptions']['code']:
        raise AssertionError('restored baseline failed')

    # Git changed-file selection does not expand to importers or production sources for test edits.
    for command in [['git', 'init', '-q'], ['git', 'add', 'src'], ['git', '-c', 'user.name=Research', '-c', 'user.email=research@example.invalid', 'commit', '-qm', 'frozen research controls']]:
        if run(command, root)['code']:
            raise RuntimeError('temporary git setup failed')
    from mutator.cli import _changed_sources
    from mutator.crapper_link import ensure_crapper
    test = root / 'src/account.test.ts'
    test.write_text(test.read_text() + '\n// test-only edit\n')
    results['changedTestFiles'] = [p.relative_to(root).as_posix() for p in _changed_sources(root, ensure_crapper())]
    helper = root / 'src/helper.ts'
    helper.write_text(helper.read_text() + '\n// imported-helper edit\n')
    results['changedHelperFiles'] = [p.relative_to(root).as_posix() for p in _changed_sources(root, ensure_crapper())]
    results['changedFunctionSeed'] = upstream(root, 'src/account.ts', lines={16})
    target.write_text(target.read_text().replace('return true;', 'return false;'))
    test.write_text("import {expect,it} from 'vitest'; import {initialEligibility} from './account'; it('changed default',()=>expect(initialEligibility()).toBe(false));\n")
    results['changedFunctionRun'] = upstream(root, 'src/account.ts', all_sites=False, lines={16})
    return results
