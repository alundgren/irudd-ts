"""Validate declared fixed baselines only; never produce historical fault labels."""
import argparse
import hashlib
import importlib.util
import pathlib
import sys

LIBRARY = pathlib.Path('/Users/alun/.t3/worktrees/irudd-ts/t3code-6f4260d3/research/test-value')
sys.path.insert(0, str(LIBRARY))
import history
from runner import REPOSITORY, disk_check, hash_tree, install_signal_handlers, node_fingerprint, parse_json, private_environment, read_json, run_command, runner_fingerprint, write_json


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidates', required=True, type=pathlib.Path)
    parser.add_argument('--dependency-installations', required=True, type=pathlib.Path)
    parser.add_argument('--prepared-output', required=True, type=pathlib.Path)
    parser.add_argument('--output', required=True, type=pathlib.Path)
    parser.add_argument('--archive-helper', required=True, type=pathlib.Path)
    parser.add_argument('--t3-node', required=True, type=pathlib.Path)
    parser.add_argument('--scope-node', required=True, type=pathlib.Path)
    parser.add_argument('--revision', required=True)
    args = parser.parse_args()
    install_signal_handlers()
    raw = args.candidates.read_bytes()
    manifest = parse_json(raw, args.candidates)
    candidates = manifest['candidates']
    prepared = args.prepared_output.resolve()
    mapping = read_json(args.dependency_installations)
    rows = {row['id']: row for row in mapping['installations']}
    # Validation reuses already owned stores. Preparation remains a separate phase.
    for candidate in candidates:
        row = rows.get(mapping['candidates'].get(candidate['id']))
        if candidate.get('preflightExclusion') or row is None:
            continue
        receipt = read_json(row['receiptPath'])
        if receipt.get('complete'):
            key = row['subject'] + '-' + receipt['inputSignature']
            if not (prepared / 'installation-metadata' / (key + '.json')).is_file() or not (prepared / 'installation-dependencies' / key).is_dir():
                raise ValueError('Explicit owned store preparation is missing: ' + key)
    installations = history.HistoricalInstallations(args.dependency_installations, candidates, prepared)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    head = run_command(['git', '-C', str(REPOSITORY), 'rev-parse', 'HEAD'], output, private_environment(output / 'identity-private'), output / 'identity', timeout=30)
    if head['status'] != 'finished' or head['exitCode'] != 0 or head['cleanupComplete'] is not True or head['cleanupErrors'] or head['residualProcessGroup']:
        raise RuntimeError('Cannot confirm frozen source revision')
    revision = (output / 'identity/stdout.txt').read_text().strip()
    if revision != args.revision:
        raise ValueError('Frozen source revision disagrees with baseline validation command')
    helper = args.archive_helper.resolve()
    helper_sha = sha(helper)
    spec = importlib.util.spec_from_file_location('approved_archive_helper', helper)
    archive_helper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(archive_helper)
    nodes = {'t3code': args.t3_node.resolve(), 'scope': args.scope_node.resolve()}
    node_identities = {name: node_fingerprint(str(path)) for name, path in nodes.items()}
    source_sha = runner_fingerprint()
    sdk_sha = hash_tree(REPOSITORY / 'sdk')[0]
    script_sha = sha(pathlib.Path(__file__))
    metadata_sha = hash_tree(prepared / 'installation-metadata')[0]
    registration = {'schemaVersion': 1, 'validationOnly': True, 'faultVerificationPerformed': False, 'mutationExecutionPerformed': False,
                    'candidates': candidates, 'candidatesManifestSha256': hashlib.sha256(raw).hexdigest(),
                    'candidatesManifestMetadata': {key: value for key, value in manifest.items() if key != 'candidates'},
                    'preparedOutput': str(prepared), 'preparedMetadataSha256': metadata_sha,
                    'dependencyInstallations': installations.registration(), 'runnerSha256': source_sha, 'sdkSha256': sdk_sha,
                    'repositoryRevision': revision, 'baselineHelperSha256': script_sha, 'archiveHelperSha256': helper_sha, 'node': node_identities,
                    'workspaceAdapterPolicy': history.WORKSPACE_ADAPTER,
                    'command': history.COMMAND, 'baselineReuseInFinalRun': False}
    write_json(output / 'registration.json', registration)

    def verify(candidate):
        disk_check(output)
        if (args.candidates.read_bytes() != raw or runner_fingerprint() != source_sha or hash_tree(REPOSITORY / 'sdk')[0] != sdk_sha or
                sha(pathlib.Path(__file__)) != script_sha or sha(helper) != helper_sha or
                hash_tree(prepared / 'installation-metadata')[0] != metadata_sha or
                any(node_fingerprint(str(path)) != node_identities[name] for name, path in nodes.items())):
            raise RuntimeError('Baseline validation identity changed; stop and preserve evidence')
        installations.verify(candidate)

    results = []
    for candidate in candidates:
        verify(candidate)
        directory = output / candidate['id']
        directory.mkdir()
        record = {'candidate': candidate, 'validationOnly': True, 'baselineAccepted': False, 'executionAttempted': False}
        preflight = candidate.get('preflightExclusion')
        profile = installations.profiles[candidate['id']]
        if preflight or profile.get('setupExclusion'):
            record['exclusion'] = 'Preflight: ' + preflight if preflight else profile['setupExclusion']
        else:
            template = directory / 'template'
            template.mkdir()
            tarpath = directory / 'source.tar'
            process = run_command(['git', '-C', candidate['repository'], 'archive', '--output=' + str(tarpath), candidate['fix']],
                                  directory, private_environment(directory / 'archive-private', {'PATH': '/usr/bin:/bin'}),
                                  directory / 'archive', timeout=120, max_output=4 * 1024 * 1024)
            write_json(directory / 'archive/execution.json', process)
            if process.get('cleanupComplete') is not True or process.get('cleanupErrors'):
                raise RuntimeError('Baseline archive cleanup uncertain; stop and retain evidence')
            if not archive_helper.completed(process) or tarpath.stat().st_size > archive_helper.MAX_ARCHIVE_BYTES:
                raise RuntimeError('Bounded baseline archive failed; stop and retain evidence')
            archive_helper.extract_archive(tarpath, template)
            record.update(archiveSha256=sha(tarpath), archivePayloadBytes=tarpath.stat().st_size)
            try:
                record['candidateInstallationInputs'] = installations.verify_source(candidate, template)
                adapters = history.configure(template, candidate['testFiles'], LIBRARY / 'vitest-reporter.ts')
                record['workspaceAdapters'] = adapters
                execution = history.execute_case(template, directory / 'baseline', history.COMMAND, dependencies=installations.dependency(candidate),
                                                 node=str(nodes[candidate['subject']]), expected_runtime=node_identities[candidate['subject']],
                                                 import_controls=history.workspace_import_controls(adapters), timeout=90)
                record.update(executionAttempted=execution.get('capturedPid') is not None, baselineAccepted=execution['complete'] and execution['status'] == 'survived', baselineComplete=execution['complete'],
                              baselineStatus=execution['status'], testCount=len(execution['tests']), evidence='baseline/execution.json',
                              infrastructureErrors=execution['infrastructureErrors'], evidenceSha256=sha(directory / 'baseline/execution.json'))
            except (ValueError, FileNotFoundError) as error:
                record['exclusion'] = 'Setup: ' + str(error)
        verify(candidate)
        write_json(directory / 'validation.json', record)
        results.append(record)
        write_json(output / 'results.json', {'validationOnly': True, 'registrationSha256': sha(output / 'registration.json'), 'results': results})
        print(candidate['id'], 'baseline-pass' if record['baselineAccepted'] else 'baseline-unavailable', len(results), len(candidates), flush=True)


if __name__ == '__main__':
    main()
