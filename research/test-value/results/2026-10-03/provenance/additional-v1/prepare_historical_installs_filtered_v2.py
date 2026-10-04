"""Prepare original T3 groups with one audited workspace exclusion; no tests run."""
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import tarfile
from types import SimpleNamespace

sys.path.insert(0, '/Users/alun/.t3/worktrees/irudd-ts/t3code-6f4260d3/research/test-value')
from runner import check_cancelled, disk_check, hash_tree, install_signal_handlers, node_fingerprint, private_environment, read_json, run_command, runner_fingerprint, vitest_entrypoint, write_json

ROOT = pathlib.Path('/tmp/archguard-test-value-20261003').resolve()
PLAN = ROOT / 'historical-install-plan.json'
AUDIT = ROOT / 'historical-filter-audit.json'
BASE = ROOT / 'historical-installs-filtered-v2'
NODE = pathlib.Path('/Users/alun/.local/share/vite-plus/js_runtime/node/24.21.0/bin/node')
PMS = {'11.10.0': pathlib.Path('/Users/alun/.local/share/vite-plus/package_manager/pnpm/11.10.0/pnpm/bin/pnpm.cjs'),
       '10.24.0': ROOT / 'tooling/pnpm-10.24.0/package/bin/pnpm.cjs'}
FILTER = '--filter=!t3code-relay'
MAX_AUDIT_BYTES = 16 * 1024 * 1024
MAX_ARCHIVE_BYTES = 512 * 1024 * 1024


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def completed(process):
    return (process.get('status') == 'finished' and process.get('exitCode') == 0 and process.get('cleanupComplete') is True and
            not process.get('cleanupErrors') and process.get('residualProcessGroup') is False)


def audit_json(raw):
    require(len(raw) <= MAX_AUDIT_BYTES, 'Preparation audit exceeds its declared 16MiB budget')

    def unique(pairs):
        value = {}
        for key, item in pairs:
            require(key not in value, 'Duplicate preparation audit JSON field: ' + key)
            value[key] = item
        return value

    return json.loads(raw.decode('utf-8'), object_pairs_hook=unique)


def extract_archive(tarpath, source):
    with tarfile.open(tarpath) as tar:
        for member in tar:
            disk_check(source)
            check_cancelled()
            filtered = tarfile.data_filter(member, str(source))
            if filtered is None:
                continue
            if not filtered.isreg():
                tar.extract(member, source, filter='data')
                continue
            target = source / filtered.name
            target.parent.mkdir(parents=True, exist_ok=True)
            with tar.extractfile(member) as incoming, target.open('wb') as outgoing:
                while True:
                    disk_check(source)
                    check_cancelled()
                    chunk = incoming.read(1024 * 1024)
                    if not chunk:
                        break
                    outgoing.write(chunk)
            if filtered.mode is not None:
                os.chmod(target, filtered.mode)


def main():
    install_signal_handlers()
    plan_raw, audit_raw = PLAN.read_bytes(), AUDIT.read_bytes()
    plan, audit = audit_json(plan_raw), audit_json(audit_raw)
    require(audit['protectedPrimaryPlan']['sha256'] == hashlib.sha256(plan_raw).hexdigest(), 'Closure audit references a different plan')
    require(audit['filterUnderReview'] == FILTER and audit['summary']['filterSafeAcrossAllCandidates'] is True,
            'Uniform relay exclusion lacks a complete static closure audit')
    groups = [group for group in plan['groups'] if group['subject'] == 't3code']
    audited_groups = {group['id']: group for group in audit['groups']}
    require(len(groups) == 21 and set(audited_groups) == {group['id'] for group in groups}, 'Original T3 group audit must cover exactly 21 groups')
    require(all(row['allCandidatesStaticFilterSafe'] and not row['relayInAnySelectedClosure'] and
                not row['sourceImportHitsToRelay'] and not row['workspaceEdgesToRelay'] for row in audited_groups.values()), 'Relay occurs in a selected dependency closure')
    selected = set(sys.argv[1:])
    require(not selected - set(audited_groups), 'Unknown requested original installation group')
    BASE.mkdir(exist_ok=False)
    helper_sha, runner_sha = sha(pathlib.Path(__file__)), runner_fingerprint()
    node = node_fingerprint(str(NODE))
    pm_hashes = {version: hash_tree(path.parent.parent)[0] for version, path in PMS.items()}
    original_map = read_json(ROOT / 'historical-installs/dependency-installations.json')
    scope_rows = [row for row in original_map['installations'] if row['subject'] == 'scope']
    require(len(scope_rows) == 4, 'Reuse only the four declared original Scope installations')
    for row in scope_rows:
        require(sha(pathlib.Path(row['receiptPath'])) == row['receiptSha256'] and read_json(row['receiptPath'])['complete'], 'Original Scope preparation receipt changed or failed')
    mapping = {'schemaVersion': 1, 'installations': scope_rows,
               'candidates': {candidate['id']: candidate['groupId'] for candidate in plan['candidates']},
               'installationPlanSha256': hashlib.sha256(plan_raw).hexdigest(), 'closureAuditSha256': hashlib.sha256(audit_raw).hexdigest(),
               't3WorkspaceFilter': FILTER, 'preparationHelperSha256': helper_sha}

    def verify():
        check_cancelled()
        disk_check(BASE)
        require(PLAN.read_bytes() == plan_raw and AUDIT.read_bytes() == audit_raw and sha(pathlib.Path(__file__)) == helper_sha and
                runner_fingerprint() == runner_sha and node_fingerprint(str(NODE)) == node, 'Preparation inputs, helper or Node changed')
        require(all(hash_tree(path.parent.parent)[0] == pm_hashes[version] for version, path in PMS.items()), 'Pinned package-manager bytes changed')

    write_json(BASE / 'preparation-registration.json', {**mapping, 'runnerSha256': runner_sha, 'node': node, 'pmPackageTreeSha256': pm_hashes})
    for group in groups:
        if selected and group['id'] not in selected:
            continue
        verify()
        require(not group['activeConfigHooks'] and not group['configDependencies'], 'Executable installer configuration requires separate audit')
        version = group['packageManager']['exactVersion']
        require(version in PMS and not group['node']['declaredPins'], 'Unsupported original package manager or automatic runtime pin')
        root = BASE / group['id']
        root.mkdir()
        source = root / 'source'
        source.mkdir()
        archive_environment = private_environment(root / 'archive-private', {'PATH': '/usr/bin:/bin'})
        archive_process = run_command(['git', '-C', group['repository'], 'archive', group['revision']], root,
                                      archive_environment, root / 'archive', timeout=120, max_output=MAX_ARCHIVE_BYTES)
        write_json(root / 'archive/execution.json', archive_process)
        require(archive_process['cleanupComplete'] and not archive_process.get('cleanupErrors'), 'Archive process cleanup uncertain; stop')
        require(archive_process['status'] == 'finished' and archive_process['exitCode'] == 0, 'Bounded owned archive command failed')
        tarpath = root / 'archive/stdout.txt'
        require(tarpath.stat().st_size <= MAX_ARCHIVE_BYTES, 'Archive exceeds declared 512MiB byte budget')
        extract_archive(tarpath, source)
        input_files = {item['path']: item['sha256'] for item in group['inputFiles']}

        def changed_inputs():
            return [relative for relative, expected in input_files.items() if not (source / relative).resolve().is_relative_to(source) or
                    (source / relative).is_symlink() or not (source / relative).is_file() or sha(source / relative) != expected]

        require(not changed_inputs(), 'Fresh archive disagrees with audited installer inputs')
        environment = private_environment(root / 'private', {'PATH': str(NODE.parent) + ':/usr/bin:/bin', 'NO_COLOR': '1',
                        'npm_config_userconfig': '/dev/null', 'npm_config_ignore_pnpmfile': 'true', 'pnpm_config_ignore_pnpmfile': 'true'})
        environment['PNPM_HOME'] = str(root / 'private/pnpm')
        pathlib.Path(environment['PNPM_HOME']).mkdir()
        if version == '10.24.0':
            environment.update(npm_config_manage_package_manager_versions='false', npm_config_package_manager_strict_version='true')
        pm = [str(NODE), str(PMS[version])]
        process = run_command(pm + ['--version'], source, environment, root / 'pm-version', timeout=30)
        require(completed(process) and (root / 'pm-version/stdout.txt').read_text().strip() == version, 'Pinned package manager version probe failed')
        command = pm + ['install', '--frozen-lockfile', '--ignore-scripts', '--ignore-pnpmfile', FILTER,
                        '--reporter=append-only', '--store-dir=' + str(BASE / 'shared-store')]
        if version != '10.24.0':
            command += ['--no-runtime', '--pm-on-fail=error']
        safeguards = {'frozenLockfile': True, 'lifecycleScripts': False, 'pnpmHooks': False, 'automaticPackageManagerSwitch': False, 'automaticRuntimeInstall': False}
        identity = {'schemaVersion': 1, 'id': group['id'], 'subject': 't3code', 'installationRoot': str(source),
                    'inputSignature': hashlib.sha256(json.dumps(sorted(input_files.items()), separators=(',', ':')).encode()).hexdigest(),
                    'policy': 'lock-matched-adapted-v1', 'safeguards': safeguards, 'repository': group['repository'], 'revision': group['revision'],
                    'sourceTree': group['rootTreeId'], 'archiveSha256': sha(tarpath), 'archivePayloadBytes': tarpath.stat().st_size,
                    'sourceSha256': hash_tree(source)[0], 'inputFiles': input_files,
                    'archiveProcess': archive_process, 'maxArchiveBytes': MAX_ARCHIVE_BYTES,
                    'installationPlanSha256': mapping['installationPlanSha256'], 'closureAuditSha256': mapping['closureAuditSha256'],
                    'workspaceFilter': FILTER, 'command': command, 'environmentPolicy': {key: value for key, value in environment.items() if key.startswith(('npm_config_', 'pnpm_config_'))},
                    'preparationHelperSha256': helper_sha, 'preparationRunnerSha256': runner_sha, 'pmPackageTreeSha256': pm_hashes[version],
                    'pmVersion': version, 'pmSha256': sha(PMS[version]), 'nodeVersion': node['version'], 'nodeSha256': node['sha256'],
                    'adaptation': 'Lock-matched adapted dependencies with uniform audited relay exclusion; scripts/hooks ignored; no full historical fidelity claim'}
        write_json(root / 'preparation.json', identity)
        process = run_command(command, source, environment, root / 'install', timeout=1200, max_output=16 * 1024 * 1024)
        changed = changed_inputs()
        complete = completed(process) and not changed
        runtime_errors = []
        importers, route, inventory_process = None, None, None
        if complete:
            try:
                inventory_process = run_command(pm + [FILTER, 'list', '--depth', '-1', '--json'], source, environment, root / 'effective-importers', timeout=30)
                write_json(root / 'effective-importers/execution.json', inventory_process)
                require(completed(inventory_process), 'Effective importer inventory failed')
                importers = read_json(root / 'effective-importers/stdout.txt')
                inventory = {(row['name'], pathlib.Path(row['path']).resolve().relative_to(source).as_posix()) for row in importers}
                require(not any(name == 't3code-relay' or path == 'infra/relay' for name, path in inventory), 'Excluded relay remains in effective importer inventory')
                for candidate in audit['candidates']:
                    if candidate['id'] in group['candidateIds']:
                        require(all((package['name'], package['path']) in inventory for package in candidate['dependencyClosure']), 'Selected workspace dependency closure is absent from effective importer inventory')
                route = vitest_entrypoint(source, SimpleNamespace(root=source), str(NODE), environment)
                write_json(root / 'direct-vitest-route.json', route)
            except (ValueError, OSError, subprocess.SubprocessError) as error:
                complete = False
                runtime_errors.append(str(error))
        changed = changed_inputs()
        complete = complete and not changed
        receipt = {**identity, 'process': process, 'inputFilesUnchanged': not changed, 'changedArchivedInputs': changed,
                   'effectiveImporters': importers, 'inventoryProcess': inventory_process, 'directVitest': route, 'validationErrors': runtime_errors, 'complete': complete}
        receipt_path = root / 'receipt.json'
        write_json(receipt_path, receipt)
        mapping['installations'].append({'id': group['id'], 'subject': 't3code', 'root': str(source), 'receiptPath': str(receipt_path), 'receiptSha256': sha(receipt_path)})
        write_json(BASE / 'dependency-installations.json', mapping)
        print(group['id'], 'complete' if complete else 'setup-failed', process['exitCode'], flush=True)
        require(process['cleanupComplete'] and not process.get('cleanupErrors'), 'Installer cleanup uncertain; stop and preserve evidence')
        require(inventory_process is None or inventory_process['cleanupComplete'] and not inventory_process.get('cleanupErrors'), 'Importer inventory cleanup uncertain; stop and preserve evidence')
        verify()


if __name__ == '__main__':
    main()
