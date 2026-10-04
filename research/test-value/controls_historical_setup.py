#!/usr/bin/env python3
"""Fixed/faulty/restored setup validation for an explicitly selected historical candidate."""
import argparse
import hashlib
from pathlib import Path
import subprocess

from controls_runner import require
from history import COMMAND, HistoricalInstallations, archive, configure, workspace_import_controls
from runner import copy_owned_source, execute_case, hash_tree, install_signal_handlers, node_fingerprint, read_json, runner_fingerprint, write_json


def controls(output, candidate, installation_map, node):
    output.mkdir(parents=True, exist_ok=False)
    registry = HistoricalInstallations(installation_map, [candidate], output)
    require(not registry.profiles[candidate['id']].get('setupExclusion'), 'Selected preparation must be complete')
    dependencies = registry.dependency(candidate)
    runtime = node_fingerprint(node)
    identity = {'validationOnly': True, 'candidate': candidate, 'node': runtime, 'runnerSha256': runner_fingerprint(),
                'dependencyInstallations': registry.registration(), 'timingUse': 'setup validation only; not a cohort measurement'}
    write_json(output / 'validation-registration.json', identity)
    repository = Path(candidate['repository'])
    fix = subprocess.check_output(['git', '-C', str(repository), 'rev-parse', candidate['fix'] + '^{commit}'], text=True).strip()
    parent = subprocess.check_output(['git', '-C', str(repository), 'rev-parse', fix + '^'], text=True).strip()
    require(fix == candidate['fix'] and parent == candidate['parent'], 'Selected historical revision and parent must match the manifest')
    template = output / 'fixed-template'
    archive(repository, fix, template)
    adapters = configure(template, candidate['testFiles'], Path(__file__).with_name('vitest-reporter.ts'))
    write_json(output / 'workspace-adapters.json', adapters)
    options = {'dependencies': dependencies, 'node': node, 'expected_node': runtime, 'timeout': 90,
               'import_controls': workspace_import_controls(adapters)}

    def verify():
        registry.verify(candidate)
        require(node_fingerprint(node) == runtime and runner_fingerprint() == identity['runnerSha256'], 'Validation runtime and helpers must remain frozen')

    def execute(source, name, baseline=None):
        verify()
        result = execute_case(source, output / name, COMMAND, baseline=baseline, **options)
        require(result['cleanupComplete'] and not result['cleanupErrors'], 'Setup validation process must have confirmed cleanup')
        verify()
        print(candidate['id'], name, result['status'], 'complete=' + str(result['complete']), 'tests=' + str(len(result['tests'])), flush=True)
        return result

    fixed = execute(template, 'fixed-before')
    require(fixed['complete'] and fixed['status'] == 'survived', 'Lock-matched adapted fixed baseline must pass')
    faulty = output / 'faulty-template'
    copy_owned_source(template, faulty)
    reverted = []
    for relative in candidate['sourceFiles']:
        target = faulty / relative
        require(target.resolve().is_relative_to(faulty), 'Source reversion must remain in the owned source')
        before = subprocess.check_output(['git', '-C', str(repository), 'show', parent + ':' + relative])
        if target.read_bytes() != before:
            target.write_bytes(before)
            reverted.append(relative)
    require(reverted, 'Historical source difference must exist')
    regression = execute(faulty, 'faulty', fixed['tests'])
    restored = execute(template, 'fixed-after', fixed['tests'])
    killed = [key for key, value in regression['outcomes'].items() if value == 'killed']
    unaffected = [key for key, value in regression['outcomes'].items() if value == 'notKilled']
    passed = regression['complete'] and bool(killed) and bool(unaffected) and restored['complete'] and restored['status'] == 'survived'
    write_json(output / 'controls.json', {**identity, 'passed': passed, 'workspaceAdapters': adapters, 'revertedFiles': reverted,
               'killedBy': killed, 'unaffected': unaffected, 'fixedTemplateSha256': hash_tree(template)[0],
               'faultyTemplateSha256': hash_tree(faulty)[0], 'recordDigests': {name: hashlib.sha256((output / name).read_bytes()).hexdigest()
               for name in ['validation-registration.json', 'workspace-adapters.json', 'fixed-before/execution.json', 'faulty/execution.json', 'fixed-after/execution.json']}})
    require(passed, 'Source reversion must fail by assertion with unaffected tests, then restored source must pass')
    print('Passed historical installation setup validation', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--candidates', type=Path, required=True)
    parser.add_argument('--candidate-id', required=True)
    parser.add_argument('--dependency-installations', type=Path, required=True)
    parser.add_argument('--node', required=True)
    args = parser.parse_args()
    install_signal_handlers()
    manifest = read_json(args.candidates)
    candidates = manifest['candidates'] if isinstance(manifest, dict) else manifest
    selected = [candidate for candidate in candidates if candidate['id'] == args.candidate_id]
    require(len(selected) == 1, 'Select exactly one declared candidate')
    controls(args.output.resolve(), selected[0], args.dependency_installations, args.node)


if __name__ == '__main__':
    main()
