"""Retain failed preparation receipts and validate their declared historical runner separately."""
import hashlib
import json
import os
import pathlib
import re
import sys
from types import SimpleNamespace

LIBRARY = pathlib.Path('/Users/alun/.t3/worktrees/irudd-ts/t3code-6f4260d3/research/test-value')
sys.path.insert(0, str(LIBRARY))
from history import verify_installation_inputs
from runner import disk_check, hash_tree, install_signal_handlers, node_fingerprint, private_environment, read_json, run_command, runner_fingerprint, vitest_entrypoint, write_json

ROOT = pathlib.Path('/tmp/archguard-test-value-20261003').resolve()
OUTPUT = ROOT / 'historical-bundled-runner-corrected-receipts'
MAPS = [ROOT / 'historical-installs-filtered-v3/dependency-installations.json', ROOT / 'historical-installs-supplement-filtered-v3/dependency-installations.json']
RECEIPTS = [ROOT / 'historical-installs-filtered-v3/t3code-install-3f6293e594ec/receipt.json',
            ROOT / 'historical-installs-supplement-filtered-v3/t3code-supp-install-027d2efae574/receipt.json']
NODE = '/Users/alun/.local/share/vite-plus/js_runtime/node/24.21.0/bin/node'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def completed(process):
    return process.get('status') == 'finished' and process.get('exitCode') == 0 and process.get('cleanupComplete') is True and not process.get('cleanupErrors') and process.get('residualProcessGroup') is False


def module_hashes(root):
    values = {}
    for directory, dirs, _ in os.walk(root, followlinks=False):
        disk_check(root)
        dirs[:] = sorted(name for name in dirs if name not in {'.git', '.repos', 'target', 'dist'})
        if pathlib.Path(directory).name == 'node_modules':
            values[pathlib.Path(directory).relative_to(root).as_posix()] = hash_tree(pathlib.Path(directory))[0]
            dirs[:] = []
    if not values:
        raise ValueError('Prepared donor contains no node_modules')
    return values


def main():
    install_signal_handlers()
    OUTPUT.mkdir(exist_ok=False)
    helper_sha, runner_sha, node = sha(pathlib.Path(__file__)), runner_fingerprint(), node_fingerprint(NODE)
    old_hashes = {str(path): sha(path) for path in RECEIPTS}
    map_hashes = {str(path): sha(path) for path in MAPS}
    original_rows = {row['id']: row for mapping in MAPS for row in read_json(mapping)['installations']}
    write_json(OUTPUT / 'registration.json', {'validationOnly': True, 'helperSha256': helper_sha, 'runnerSha256': runner_sha,
                                            'node': node, 'originalReceipts': old_hashes, 'originalPreparationMaps': map_hashes, 'derivationPolicy': 'declared-bundled-runner-validation-v1'})
    for path in RECEIPTS:
        disk_check(OUTPUT)
        old = read_json(path)
        if (old.get('complete') is not False or len(old.get('validationErrors', [])) != 1 or
                not old['validationErrors'][0].startswith('Copied vite-plus context cannot resolve its declared Vitest entrypoint:') or
                not completed(old.get('process', {})) or not completed(old.get('inventoryProcess', {})) or
                not completed(old.get('archiveProcess', {})) or old.get('inputFilesUnchanged') is not True or old.get('changedArchivedInputs') != []):
            raise ValueError('Original preparation failure is not solely the supported runner lookup')
        installed = pathlib.Path(old['installationRoot']).resolve()
        row = original_rows[old['id']]
        if row['receiptSha256'] != old_hashes[str(path)] or pathlib.Path(row['receiptPath']).resolve() != path or pathlib.Path(row['root']).resolve() != installed or row['subject'] != old['subject']:
            raise ValueError('Original preparation receipt disagrees with its frozen map')
        verify_installation_inputs(row, old)
        before = module_hashes(installed)
        evidence = OUTPUT / old['id']
        evidence.mkdir()
        environment = private_environment(evidence / 'private', {'PATH': str(pathlib.Path(node['path']).parent) + ':/usr/bin:/bin'})
        route = vitest_entrypoint(installed, SimpleNamespace(root=installed), node['path'], environment)
        if route['runnerPackage'] != '@voidzero-dev/vite-plus-test':
            raise ValueError('Correction must use the declared historical package')
        process = run_command([node['path'], route['path'], '--version'], installed, environment, evidence / 'version', timeout=30)
        write_json(evidence / 'version/execution.json', process)
        text = (evidence / 'version/stdout.txt').read_text().strip()
        version = re.match(r'vp test/([^\s]+)', text)
        if not completed(process) or version is None or version.group(1) != route['version'] or 'node-' + node['version'] not in text:
            raise ValueError('Declared historical CLI does not confirm its exact Vitest and Node versions')
        verify_installation_inputs(row, old)
        after = module_hashes(installed)
        if before != after:
            raise ValueError('Prepared node_modules changed during declared runner validation')
        if (runner_fingerprint() != runner_sha or node_fingerprint(NODE) != node or sha(pathlib.Path(__file__)) != helper_sha or
                any(sha(pathlib.Path(original)) != checksum for original, checksum in {**old_hashes, **map_hashes}.items())):
            raise RuntimeError('Corrected preparation identity changed; stop and preserve evidence')
        receipt = {**old, 'complete': True, 'validationErrors': [], 'directVitest': route,
                   'correctionOf': {'path': str(path), 'sha256': old_hashes[str(path)], 'complete': False,
                                    'reason': 'Original installation and importer inventory succeeded; supported declared historical runner was rejected'},
                   'runnerValidation': {'process': process, 'stdout': text, 'node': node, 'runnerSha256': runner_sha, 'helperSha256': helper_sha,
                                        'nodeModulesHashAlgorithm': 'runner-hash-tree-v1', 'nodeModulesBeforeAndAfter': before,
                                        'unchangedScope': 'Unchanged during this resolver and CLI validation; no original post-install store digest was recorded'}}
        write_json(evidence / 'receipt.json', receipt)
        print(old['id'], 'corrected-declared-runner', route['version'], flush=True)


if __name__ == '__main__':
    main()
