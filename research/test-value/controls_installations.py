#!/usr/bin/env python3
"""Explicit installation-map controls with owned authored packages and real Node imports."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from unittest.mock import patch

from controls_runner import fixture, require
from history import HistoricalInstallations, INSTALLATION_SAFEGUARDS, archive
from runner import MAX_JSON, DependencyStore, copy_owned_source, hash_tree, install_signal_handlers, private_environment, read_json, run_command, write_json


def checksum(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def receipt(directory, identity, value, complete=True):
    root = fixture(directory / identity, {'package.json': '{"name":"control-installation","type":"module"}',
                                        'pnpm-lock.yaml': f'lock-control: {value}\n', 'pnpm-workspace.yaml': 'packages: []\n'})
    if complete:
        package = root / 'node_modules/control-pkg'
        package.mkdir(parents=True)
        (package / 'package.json').write_text('{"name":"control-pkg","type":"module","exports":"./index.mjs"}')
        (package / 'index.mjs').write_text(f'export const value={value};\n')
    inputs = {name: checksum(root / name) for name in ['package.json', 'pnpm-lock.yaml', 'pnpm-workspace.yaml']}
    path = directory / (identity + '-receipt.json')
    value = {'schemaVersion': 1, 'id': identity, 'subject': 't3code', 'installationRoot': str(root),
             'inputSignature': hashlib.sha256(json.dumps(sorted(inputs.items()), separators=(',', ':')).encode()).hexdigest(),
             'complete': complete, 'policy': 'lock-matched-adapted-v1', 'safeguards': INSTALLATION_SAFEGUARDS,
             'inputFiles': inputs, 'inputFilesUnchanged': True, 'changedArchivedInputs': [],
             'process': {'status': 'finished' if complete else 'error', 'exitCode': 0 if complete else 1,
                         'cleanupComplete': True, 'cleanupErrors': [], 'residualProcessGroup': False},
             'provenanceKind': 'authored receipt validation fixture; no package manager invocation'}
    write_json(path, value)
    return {'id': identity, 'subject': 't3code', 'root': str(root), 'receiptPath': str(path), 'receiptSha256': checksum(path)}


def import_package(output, store, expected, node):
    output.mkdir()
    template = fixture(output / 'template', {'package.json': '{"type":"module"}'})
    source = output / 'source'
    copy_owned_source(template, source)
    store.attach(source)
    script = ('import {value} from "control-pkg";import assert from "node:assert/strict";'
              'import fs from "node:fs";import {fileURLToPath}from"node:url";'
              'const realPath=fs.realpathSync(fileURLToPath(import.meta.resolve("control-pkg")));'
              'console.log(JSON.stringify({value,realPath}));assert.equal(value,Number(process.argv[1]));')
    result = run_command([node, '--input-type=module', '-e', script, str(expected)], source,
                         private_environment(output / 'private'), output / 'process')
    write_json(output / 'process/execution.json', result)
    resolution = json.loads((output / 'process/stdout.txt').read_text())
    require(Path(resolution['realPath']).is_relative_to(store.root), 'Real module import must resolve into the selected owned external store')
    return result, resolution


def archive_binding_control(output, registry, rows, node):
    output.mkdir()
    repository = fixture(output / 'repository', {'package.json': '{"name":"control-installation","type":"module"}',
                         'pnpm-lock.yaml': 'lock-control: 2\n', 'pnpm-workspace.yaml': 'packages: []\n',
                         'subject.ts': 'export const value=1;\n', 'subject.test.ts': 'throw new Error("must not execute");\n'})
    environment = private_environment(output / 'private')
    steps = []

    def git(*arguments):
        directory = output / ('git-' + str(len(steps)))
        result = run_command(['git', '-c', 'core.hooksPath=/dev/null', '-c', 'commit.gpgsign=false',
                              '-c', 'user.name=Research control', '-c', 'user.email=research@example.invalid', *arguments],
                             repository, environment, directory, timeout=15)
        steps.append(result)
        require(result['exitCode'] == 0 and result['cleanupComplete'], 'Owned control Git archive preparation must complete')
        return (directory / 'stdout.txt').read_text().strip()

    git('init', '-q')
    git('add', '.')
    git('commit', '-qm', 'parent')
    parent = git('rev-parse', 'HEAD')
    (repository / 'subject.ts').write_text('export const value=2;\n')
    git('add', '.')
    git('commit', '-qm', 'fixed')
    fix = git('rev-parse', 'HEAD')
    declared_archive = output / 'declared-archive'
    archive(repository, fix, declared_archive)
    candidate = {'id': 'archive-probe', 'subject': 't3code', 'repository': str(repository), 'fix': fix, 'parent': parent,
                 'sourceFiles': ['subject.ts'], 'testFiles': ['subject.test.ts'], 'relatedGroup': 'authored-archive-binding-control',
                 'installationInputSignature': registry.receipts['first']['inputSignature'],
                 'sourceArchiveTreeSha256': hash_tree(declared_archive)[0], 'sourceArchiveAlgorithm': 'runner-hash-tree-v1'}
    manifest = output / 'candidates.json'
    mapping = output / 'map.json'
    write_json(manifest, [candidate])
    write_json(mapping, {'schemaVersion': 1, 'installations': rows, 'candidates': {'archive-probe': 'first'}})
    destination = output / 'experiment'
    command = [sys.executable, str(Path(__file__).with_name('history.py')), '--candidates', str(manifest), '--output', str(destination),
               '--dependency-installations', str(mapping), '--archguard', sys.executable, '--t3-node', node, '--scope-node', node]
    process = run_command(command, output, environment, output / 'process', timeout=30)
    require(process['exitCode'] == 0 and process['cleanupComplete'], 'CLI must preserve an explicit archived-input setup exclusion')
    excluded = read_json(destination / 'attempts.json')
    require(len(excluded) == 1 and not excluded[0]['verified'] and excluded[0].get('executionAttempted') is False and
            excluded[0]['exclusion'].startswith('Setup: archived candidate inputs disagree'),
            'Wrong declared installation must be rejected even while its receipt, source installation and owned store remain unchanged')
    archived = destination / candidate['id'] / 'fixed-template'
    require(archived.exists() and not (archived / 'research-test-value.config.ts').exists() and not list(destination.glob('*/*/execution.json')),
            'Archive mismatch must stop before configuration adaptation and any baseline execution')
    corrected_candidate = {**candidate, 'id': 'two', 'installationInputSignature': registry.receipts['second']['inputSignature']}
    corrected = registry.verify_source(corrected_candidate, archived)
    registry.verify({'id': 'one'})
    registry.verify({'id': 'two'})
    require(corrected['inputSignature'] == registry.receipts['second']['inputSignature'], 'Same actual archive must match its declared second installation inputs')
    for name, relative, content in [('extra-manifest', 'packages/extra/package.json', '{"name":"new-package"}'),
                                    ('extra-installer-config', '.npmrc', 'node-linker=hoisted\n')]:
        extra = archived / relative
        extra.parent.mkdir(parents=True, exist_ok=True)
        extra.write_text(content)
        try:
            registry.verify_source(corrected_candidate, archived)
        except ValueError as error:
            require('frozen complete archive' in str(error), 'Unlisted installer input must fail complete source inventory binding')
        else:
            raise AssertionError('New archive installer input accepted: ' + name)
        extra.unlink()
        before_extra = git('rev-parse', 'HEAD')
        repository_extra = repository / relative
        repository_extra.parent.mkdir(parents=True, exist_ok=True)
        repository_extra.write_text(content)
        git('add', '.')
        git('commit', '-qm', name)
        extra_candidate = {**candidate, 'fix': git('rev-parse', 'HEAD'), 'parent': before_extra,
                           'installationInputSignature': registry.receipts['second']['inputSignature']}
        extra_manifest, extra_map = output / (name + '-candidate.json'), output / (name + '-map.json')
        write_json(extra_manifest, [extra_candidate])
        write_json(extra_map, {'schemaVersion': 1, 'installations': rows, 'candidates': {'archive-probe': 'second'}})
        extra_destination = output / (name + '-experiment')
        extra_command = list(command)
        for option, value in [('--candidates', extra_manifest), ('--dependency-installations', extra_map), ('--output', extra_destination)]:
            extra_command[extra_command.index(option) + 1] = str(value)
        extra_process = run_command(extra_command, output, environment, output / (name + '-process'), timeout=30)
        extra_attempt = read_json(extra_destination / 'attempts.json')[0]
        require(extra_process['exitCode'] == 0 and extra_process['cleanupComplete'] and extra_attempt.get('executionAttempted') is False and
                'frozen complete archive' in extra_attempt['exclusion'], 'Real CLI archive with an extra input must exclude before baseline')
        require(not list(extra_destination.glob('*/*/execution.json')) and
                not list(extra_destination.glob('*/fixed-template/research-test-value.config.ts')), 'Extra input must stop before configuration and test execution')
        registry.verify({'id': 'two'})
        repository_extra.unlink()
        git('add', '.')
        git('commit', '-qm', 'restore declared control archive')
    registry.verify_source(corrected_candidate, archived)
    return {'control': 'actual-archive-wrong-installation-and-extra-input-rejection-before-baseline-and-correct-binding', 'passed': True,
            'exclusion': excluded[0]['exclusion'], 'correctInputSignature': corrected['inputSignature'],
            'extraInputNegatives': ['new-package-manifest', 'new-installer-config']}


def controls(output, node):
    output.mkdir(parents=True, exist_ok=False)
    prepared = output / 'prepared'
    prepared.mkdir()
    rows = [receipt(prepared, 'first', 1), receipt(prepared, 'second', 2), receipt(prepared, 'failed', 3, False)]
    candidates = []
    for name, row in zip(['one', 'two', 'also-two', 'failed', 'unmapped', 'missing-installation'],
                         [rows[0], rows[1], rows[1], rows[2], rows[0], rows[0]]):
        declaration = read_json(row['receiptPath'])
        source = fixture(output / ('declared-source-' + name), {file: (Path(row['root']) / file).read_text()
                                                              for file in declaration['inputFiles']})
        candidates.append({'id': name, 'subject': 't3code', 'installationInputSignature': declaration['inputSignature'],
                           'sourceArchiveTreeSha256': hash_tree(source)[0], 'sourceArchiveAlgorithm': 'runner-hash-tree-v1'})
    manifest = {'schemaVersion': 1, 'installations': rows,
                'candidates': {'one': 'first', 'two': 'second', 'also-two': 'second',
                               'failed': 'failed', 'missing-installation': 'not-declared'}}
    path = output / 'installation-map.json'
    write_json(path, manifest)
    original_copy = DependencyStore.copy
    copied = []

    def counted_copy(store):
        copied.append(str(store.installed_root))
        return original_copy(store)

    with patch.object(DependencyStore, 'copy', counted_copy):
        registry = HistoricalInstallations(path, candidates, output / 'owned')
    require(len(copied) == 2, 'Three candidates mapped to two complete installations must copy exactly two stores')
    require(registry.dependency(candidates[1]) is registry.dependency(candidates[2]), 'Identical dependency inputs must reuse one owned store')
    for candidate in candidates[3:]:
        require(registry.dependency(candidate) is None and registry.profiles[candidate['id']].get('setupExclusion'),
                'Failed, unmapped and absent installations must never select a fallback donor')
    before, before_resolution = import_package(output / 'before-wrong-global-donor', registry.dependency(candidates[0]), 2, node)
    require(before['exitCode'] == 1 and 'ERR_ASSERTION' in (output / 'before-wrong-global-donor/process/stderr.txt').read_text(),
            'One global donor must reproduce a real wrong-version import assertion')
    actual = []
    for candidate, expected in [(candidates[0], 1), (candidates[1], 2), (candidates[2], 2)]:
        registry.verify(candidate)
        result, resolution = import_package(output / ('correct-' + candidate['id']), registry.dependency(candidate), expected, node)
        require(result['exitCode'] == 0 and result['status'] == 'finished' and result['cleanupComplete'], 'Explicit per-candidate installation must satisfy the actual import assertion')
        actual.append(resolution)
    registration = registry.registration()
    require(len(registration['ownedStores']) == 2 and len(registration['candidateProfiles']) == 6 and len(registration['receipts']) == 3,
            'Registration must retain complete, incomplete and absent route provenance')
    write_json(output / 'registered.json', registration)
    records = [{'control': 'real-wrong-global-donor-before-and-explicit-two-store-correction', 'passed': True,
                'beforeResolution': before_resolution, 'correctResolutions': actual, 'copiedStores': copied}]
    records.append(archive_binding_control(output / 'archive-binding', registry, rows, node))

    def reject(name, value, selected=candidates):
        target = output / (name + '.json')
        write_json(target, value)
        with patch.object(DependencyStore, 'copy') as copying:
            try:
                HistoricalInstallations(target, selected, output / (name + '-owned'))
            except (ValueError, OSError) as error:
                require(copying.call_count == 0, 'Invalid map/receipt must fail before any dependency copy')
                records.append({'control': name, 'passed': True, 'error': str(error)})
            else:
                raise AssertionError('Invalid dependency map accepted: ' + name)

    reject('duplicate-installation', {**manifest, 'installations': rows + [rows[0]]})
    missing_archive_identity = [{key: value for key, value in candidates[0].items() if key != 'sourceArchiveTreeSha256'}] + candidates[1:]
    reject('missing-complete-candidate-archive-identity', manifest, missing_archive_identity)
    reject('unsupported-complete-archive-algorithm', manifest, [{**candidates[0], 'sourceArchiveAlgorithm': 'unsupported'}] + candidates[1:])
    duplicate_inputs = receipt(prepared, 'second-same-inputs-different-bytes', 2)
    (Path(duplicate_inputs['root']) / 'node_modules/control-pkg/index.mjs').write_text('export const value=999;\n')
    reject('same-inputs-different-prepared-dependencies', {**manifest, 'installations': rows + [duplicate_inputs]})
    replaced_receipt = output / 'different-command-receipt.json'
    write_json(replaced_receipt, {**read_json(rows[0]['receiptPath']), 'command': ['different-preparation-command']})
    replaced_map = output / 'different-command-map.json'
    write_json(replaced_map, {**manifest, 'installations': [{**rows[0], 'receiptPath': str(replaced_receipt),
                              'receiptSha256': checksum(replaced_receipt)}] + rows[1:]})
    with patch.object(DependencyStore, 'copy') as copying:
        try:
            HistoricalInstallations(replaced_map, candidates, output / 'owned')
        except ValueError as error:
            require(copying.call_count == 0 and 'Resumed store source installation' in str(error), 'Changed preparation receipt must reject pre-registration copied stores')
            records.append({'control': 'resumed-store-changed-preparation-receipt', 'passed': True, 'error': str(error)})
        else:
            raise AssertionError('Changed preparation receipt accepted with previous copied store')
    with patch.object(DependencyStore, 'copy') as copying:
        restored_registry = HistoricalInstallations(path, candidates, output / 'owned')
        require(copying.call_count == 0 and restored_registry.registration() == registry.registration(), 'Unchanged receipt must resume exactly the registered owned stores')
    reject('duplicate-candidate', manifest, candidates + [candidates[0]])
    reject('extra-candidate', {**manifest, 'candidates': {**manifest['candidates'], 'undeclared': 'first'}})
    reject('candidate-subject-mismatch', manifest, [{**candidates[0], 'subject': 'scope'}] + candidates[1:])
    reject('receipt-sha-mismatch', {**manifest, 'installations': [{**rows[0], 'receiptSha256': '0' * 64}] + rows[1:]})
    for name, changes in [('unsafe-pnpm-hook', {'safeguards': {**INSTALLATION_SAFEGUARDS, 'pnpmHooks': True}}),
                          ('wrong-root', {'installationRoot': str(prepared)}),
                          ('wrong-receipt-subject', {'subject': 'scope'}),
                          ('input-signature-mismatch', {'inputSignature': '0' * 64}),
                          ('changed-preparation-inputs', {'inputFilesUnchanged': False}),
                          ('incomplete-process-claimed-complete', {'process': {'exitCode': 1, 'status': 'error'}})]:
        modified = output / (name + '-receipt.json')
        write_json(modified, {**read_json(rows[0]['receiptPath']), **changes})
        reject(name, {**manifest, 'installations': [{**rows[0], 'receiptPath': str(modified), 'receiptSha256': checksum(modified)}] + rows[1:]})
    input_file = Path(rows[0]['root']) / 'pnpm-lock.yaml'
    original = input_file.read_bytes()
    input_file.write_text('tampered actual lock bytes, unchanged receipt\n')
    reject('changed-actual-input-before-copy', manifest)
    try:
        registry.verify(candidates[0])
    except RuntimeError:
        records.append({'control': 'changed-actual-input-after-registration', 'passed': True})
    else:
        raise AssertionError('Changed actual input accepted with unchanged receipt SHA')
    input_file.write_bytes(original)
    input_file.unlink()
    reject('deleted-actual-input-before-copy', manifest)
    input_file.write_bytes(original)
    outside = output / 'outside-lock.yaml'
    outside.write_bytes(original)
    input_file.unlink()
    input_file.symlink_to(outside)
    reject('escaping-actual-input-before-copy', manifest)
    input_file.unlink()
    input_file.write_bytes(original)
    for name, target in [('registered-receipt-tamper', Path(rows[0]['receiptPath'])), ('registered-map-tamper', path),
                         ('registered-store-tamper', registry.dependency(candidates[0]).root / 'node_modules/control-pkg/index.mjs')]:
        original = target.read_bytes()
        target.write_bytes(original + b'\n')
        try:
            registry.verify(candidates[0])
        except RuntimeError:
            records.append({'control': name, 'passed': True})
        else:
            raise AssertionError('Registered input mutation accepted: ' + name)
        finally:
            target.write_bytes(original)
    registry.verify(candidates[0])
    # This CLI run has only setup exclusions. Deliberately absent repositories prove
    # that an unmapped/failed installation cannot archive or execute a candidate.
    excluded = [{**candidate, 'repository': str(output / 'never-archive'), 'fix': 'not-a-commit', 'parent': 'not-a-parent',
                 'sourceFiles': ['never.ts'], 'testFiles': ['never.test.ts']} for candidate in candidates[3:]]
    candidate_path, exclusion_map = output / 'excluded-candidates.json', output / 'exclusion-map.json'
    excluded_manifest = {'schemaVersion': 1, 'sourceBindingAuditSha256': 'a' * 64, 'candidates': excluded}
    write_json(candidate_path, excluded_manifest)
    write_json(exclusion_map, {**manifest, 'candidates': {key: value for key, value in manifest['candidates'].items() if key in {row['id'] for row in excluded}}})
    cli_output = output / 'cli-exclusions'
    command = [sys.executable, str(Path(__file__).with_name('history.py')), '--candidates', str(candidate_path), '--output', str(cli_output),
               '--dependency-installations', str(exclusion_map), '--archguard', sys.executable, '--t3-node', node, '--scope-node', node]
    process = run_command(command, output, private_environment(output / 'cli-private'), output / 'cli-process', timeout=30)
    require(process['exitCode'] == 0, 'CLI must record explicit setup exclusions without test execution')
    attempts = read_json(cli_output / 'attempts.json')
    require(len(attempts) == 3 and all(row.get('executionAttempted') is False and not row['verified'] and
                                      row['exclusion'].startswith('Setup:') for row in attempts), 'CLI exclusion ledger must be explicit for all unavailable routes')
    require(not list(cli_output.glob('*/fixed-template')) and not list(cli_output.glob('*/*/execution.json')), 'Unavailable map routes must not archive or run a test')
    registered_manifest = read_json(cli_output / 'preregistration.json')
    require(registered_manifest['candidatesManifestSha256'] == checksum(candidate_path) and
            registered_manifest['candidatesManifestMetadata'] == {'schemaVersion': 1, 'sourceBindingAuditSha256': 'a' * 64},
            'Registration must bind whole manifest bytes and retain the independent audit metadata')
    write_json(candidate_path, {**excluded_manifest, 'sourceBindingAuditSha256': 'b' * 64})
    changed = run_command(command, output, private_environment(output / 'changed-manifest-private'), output / 'changed-manifest-process', timeout=30)
    require(changed['exitCode'] != 0 and changed['cleanupComplete'] and
            'experiment identity changed' in (output / 'changed-manifest-process/stderr.txt').read_text(), 'Same candidates with altered audit metadata must reject existing registration')
    require(read_json(cli_output / 'preregistration.json') == registered_manifest, 'Rejected manifest metadata change must preserve the original registration')
    write_json(candidate_path, excluded_manifest)
    for name, raw, diagnostic in [('duplicate-candidates', b'{"candidates":[],"candidates":[]}', 'Duplicate JSON field: candidates'),
                                  ('duplicate-audit-metadata', b'{"sourceBindingAuditSha256":"a","sourceBindingAuditSha256":"b","candidates":[]}', 'Duplicate JSON field: sourceBindingAuditSha256'),
                                  ('oversized-candidate-manifest', b' ' * (MAX_JSON + 1), 'JSON exceeds byte budget')]:
        candidate_path.write_bytes(raw)
        invalid = run_command(command, output, private_environment(output / (name + '-private')), output / (name + '-process'), timeout=30)
        require(invalid['exitCode'] != 0 and invalid['cleanupComplete'] and diagnostic in (output / (name + '-process/stderr.txt')).read_text(),
                'Candidate manifest must preserve strict bounded duplicate-free decoding: ' + name)
        require(read_json(cli_output / 'preregistration.json') == registered_manifest, 'Malformed manifest must preserve existing registration')
        records.append({'control': name, 'passed': True})
    write_json(candidate_path, excluded_manifest)
    mixed = subprocess.run(command + ['--t3-dependencies', str(prepared), '--scope-dependencies', str(prepared)], capture_output=True, timeout=15)
    require(mixed.returncode == 2 and b'never both modes' in mixed.stderr, 'CLI must reject donor fallback mixed with explicit installation mode')
    records.append({'control': 'cli-missing-failed-unknown-exclusions-and-no-global-fallback', 'passed': True})
    write_json(output / 'controls.json', {'passed': True, 'controls': records, 'timingUse': 'validation only'})
    print(f'Passed {len(records)} installation control groups', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--node', default='/Users/alun/.local/share/vite-plus/js_runtime/node/24.21.0/bin/node')
    args = parser.parse_args()
    controls(args.output.resolve(), str(Path(args.node).resolve()))


if __name__ == '__main__':
    install_signal_handlers()
    main()
