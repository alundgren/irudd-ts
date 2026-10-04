#!/usr/bin/env python3
"""Historical source-reversion experiments with an explicit adapted test environment."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from analyze import analyze, compact_evaluation, evaluate_fault, summarize_faults
from runner import HERE, REPOSITORY, DependencyStore, copy_owned_source, direct_vitest_command, disk_check, execute_case as raw_execute_case, hash_tree, install_signal_handlers, node_fingerprint, owned_source_path, read_json, runner_fingerprint, run_command, write_json
from rust_slice import archive

COMMAND = direct_vitest_command('research-test-value.config.ts')
WORKSPACE_ADAPTER = {'specifier': '@irudd-scope/sqlite', 'package': 'packages/sqlite/package.json',
                     'export': './src/maintenance.ts', 'consumers': ['apps/desktop/package.json', 'apps/hub/package.json'],
                     'resolution': 'exact-specifier Vite alias into the fresh source tree; actual resolution path/hash retained when imported'}
INSTALLATION_SAFEGUARDS = {'frozenLockfile': True, 'lifecycleScripts': False, 'pnpmHooks': False,
                           'automaticPackageManagerSwitch': False, 'automaticRuntimeInstall': False}


def sha256_text(value):
    return isinstance(value, str) and len(value) == 64 and all(letter in '0123456789abcdef' for letter in value)


def verify_installation_inputs(row, receipt):
    root = Path(row['root']).resolve()
    for relative, expected in receipt['inputFiles'].items():
        path = owned_source_path(root / relative, root)
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError('Prepared dependency input changed or is unavailable: ' + relative)


class HistoricalInstallations:
    """Explicit candidate routes to prepared installations; no installation commands."""

    def __init__(self, manifest_path, candidates, output):
        self.path = Path(manifest_path).resolve()
        self.manifest_sha256 = hashlib.sha256(self.path.read_bytes()).hexdigest()
        manifest = read_json(self.path)
        if (not isinstance(manifest, dict) or type(manifest.get('schemaVersion')) is not int or
                manifest['schemaVersion'] != 1 or not isinstance(manifest.get('installations'), list) or
                not isinstance(manifest.get('candidates'), dict)):
            raise ValueError('Invalid dependency installation map')
        self.manifest = manifest
        self.rows, self.receipts, self.stores, self.profiles = {}, {}, {}, {}
        if (not isinstance(candidates, list) or any(not isinstance(candidate, dict) or not isinstance(candidate.get('id'), str) or
                not candidate['id'] or candidate['id'] in {'.', '..'} or Path(candidate['id']).is_absolute() or
                candidate.get('subject') not in {'t3code', 'scope'} for candidate in candidates)):
            raise ValueError('Invalid historical candidate identity or subject')
        expected = {candidate['id']: candidate for candidate in candidates}
        if len(expected) != len(candidates) or any(not isinstance(key, str) or not key or Path(key).parts != (key,)
                                                  for key in expected):
            raise ValueError('Duplicate or invalid historical candidate ID')
        if set(manifest['candidates']) - expected.keys():
            raise ValueError('Dependency map contains undeclared historical candidates')
        for row in manifest['installations']:
            if (not isinstance(row, dict) or not isinstance(row.get('id'), str) or not row['id'] or
                    row['id'] in self.rows or row.get('subject') not in {'t3code', 'scope'} or
                    not isinstance(row.get('root'), str) or not Path(row['root']).is_absolute() or
                    not isinstance(row.get('receiptPath'), str) or not Path(row['receiptPath']).is_absolute() or
                    not sha256_text(row.get('receiptSha256'))):
                raise ValueError('Duplicate or invalid declared dependency installation')
            receipt_path = Path(row['receiptPath'])
            if hashlib.sha256(receipt_path.read_bytes()).hexdigest() != row['receiptSha256']:
                raise ValueError('Dependency preparation receipt hash mismatch')
            receipt = read_json(receipt_path)
            if (not isinstance(receipt, dict) or type(receipt.get('schemaVersion')) is not int or receipt['schemaVersion'] != 1 or
                    receipt.get('id') != row['id'] or receipt.get('subject') != row['subject'] or
                    not isinstance(receipt.get('installationRoot'), str) or not Path(receipt['installationRoot']).is_absolute() or
                    Path(receipt['installationRoot']).resolve() != Path(row['root']).resolve() or
                    not sha256_text(receipt.get('inputSignature')) or type(receipt.get('complete')) is not bool or
                    receipt.get('policy') != 'lock-matched-adapted-v1' or not isinstance(receipt.get('safeguards'), dict) or
                    any(receipt['safeguards'].get(key) is not value for key, value in INSTALLATION_SAFEGUARDS.items())):
                raise ValueError('Dependency preparation receipt identity or safeguards disagree')
            inputs = receipt.get('inputFiles')
            if (not isinstance(inputs, dict) or not inputs or any(not isinstance(path, str) or not path or
                    Path(path).is_absolute() or '..' in Path(path).parts or Path(path).as_posix() != path or
                    not sha256_text(checksum) for path, checksum in inputs.items()) or
                    hashlib.sha256(json.dumps(sorted(inputs.items()), separators=(',', ':')).encode()).hexdigest() != receipt['inputSignature']):
                raise ValueError('Dependency preparation input records disagree with their signature')
            if receipt['complete']:
                if receipt.get('inputFilesUnchanged') is not True or receipt.get('changedArchivedInputs') != []:
                    raise ValueError('Complete dependency preparation changed archived inputs')
                process = receipt.get('process', {})
                if (not isinstance(process, dict) or process.get('status') != 'finished' or type(process.get('exitCode')) is not int or
                        process['exitCode'] != 0 or process.get('cleanupComplete') is not True or
                        process.get('cleanupErrors') or process.get('residualProcessGroup')):
                    raise ValueError('Complete dependency preparation has incomplete process evidence')
                verify_installation_inputs(row, receipt)
            self.rows[row['id']], self.receipts[row['id']] = row, receipt
        declared_inputs = set()
        for row in self.rows.values():
            signature = row['subject'], self.receipts[row['id']]['inputSignature']
            if signature in declared_inputs:
                raise ValueError('Duplicate prepared input signature; map matching candidates to one declared installation')
            declared_inputs.add(signature)
        for candidate in candidates:
            identity = candidate['id']
            installation_id = manifest['candidates'].get(identity)
            if identity in manifest['candidates'] and (not isinstance(installation_id, str) or not installation_id):
                raise ValueError('Invalid candidate dependency installation ID')
            if installation_id is None:
                profile = {'installationId': None, 'setupExclusion': 'Setup: no explicit dependency installation is mapped for this candidate'}
            elif installation_id not in self.rows:
                profile = {'installationId': installation_id, 'setupExclusion': 'Setup: mapped dependency installation is absent: ' + installation_id}
            else:
                row, receipt = self.rows[installation_id], self.receipts[installation_id]
                if row['subject'] != candidate['subject']:
                    raise ValueError('Candidate and dependency installation subjects disagree')
                profile = {'installationId': installation_id, 'receiptPath': row['receiptPath'], 'receiptSha256': row['receiptSha256'],
                           'inputSignature': receipt['inputSignature'], 'complete': receipt['complete'], 'policy': receipt['policy']}
                if not receipt['complete']:
                    profile['setupExclusion'] = 'Setup: explicitly registered dependency installation is incomplete: ' + installation_id
            self.profiles[identity] = profile
        # Every receipt is validated before any successful installation is copied.
        for candidate in candidates:
            profile = self.profiles[candidate['id']]
            if profile.get('setupExclusion') or candidate.get('preflightExclusion'):
                continue
            row = self.rows[profile['installationId']]
            key = row['subject'] + '-' + profile['inputSignature']
            if key not in self.stores:
                root = Path(output) / 'installation-dependencies' / key
                metadata = Path(output) / 'installation-metadata' / (key + '.json')
                store = DependencyStore(row['root'], root)
                if metadata.exists():
                    info = read_json(metadata)
                    if info.get('inputSignature') != profile['inputSignature'] or info.get('subject') != row['subject']:
                        raise ValueError('Resumed installation store has a different declared identity')
                    source_id = info.get('sourceInstallationId')
                    source_row, source_receipt = self.rows.get(source_id), self.receipts.get(source_id)
                    if (not source_row or not source_receipt['complete'] or source_receipt['inputSignature'] != profile['inputSignature'] or
                            source_row['subject'] != row['subject'] or info.get('sourceReceiptSha256') != source_row['receiptSha256'] or
                            Path(info['installedRoot']).resolve() != Path(source_row['root']).resolve()):
                        raise ValueError('Resumed store source installation is absent or disagrees with its receipt')
                    store.modules, store.workspace_links, store.owned_directories = info['modules'], info['workspaceLinks'], set(info['ownedDirectories'])
                    store.sha256 = info['sha256']
                    if store.current_digest() != store.sha256:
                        raise ValueError('Resumed installation dependency store has changed')
                else:
                    store.copy()
                    info = {'modules': store.modules, 'workspaceLinks': store.workspace_links,
                            'ownedDirectories': sorted(store.owned_directories), 'sha256': store.sha256,
                            'installedRoot': str(Path(row['root']).resolve()), 'sourceInstallationId': row['id'],
                            'sourceReceiptSha256': row['receiptSha256'],
                            'subject': row['subject'], 'inputSignature': profile['inputSignature']}
                    write_json(metadata, info)
                    write_json(metadata.with_name(key + '-files.json'), store.manifest)
                self.stores[key] = (store, info['sourceInstallationId'])
            store, source_installation_id = self.stores[key]
            profile.update(storeKey=key, ownedStoreSha256=store.sha256, sourceInstallationId=source_installation_id)

    def registration(self):
        return {'schemaVersion': 1, 'manifestPath': str(self.path), 'manifestSha256': self.manifest_sha256,
                'declaredMap': self.manifest,
                'receipts': {key: {'receiptSha256': self.rows[key]['receiptSha256'], 'complete': receipt['complete'],
                                   'inputSignature': receipt['inputSignature']} for key, receipt in self.receipts.items()},
                'candidateProfiles': self.profiles,
                'ownedStores': {key: {'sha256': store.sha256, 'sourceInstallationId': installation_id,
                                     'sourceReceiptSha256': self.rows[installation_id]['receiptSha256']}
                                for key, (store, installation_id) in self.stores.items()}}

    def dependency(self, candidate):
        key = self.profiles[candidate['id']].get('storeKey')
        return self.stores[key][0] if key else None

    def verify_source(self, candidate, template):
        receipt = self.receipts[self.profiles[candidate['id']]['installationId']]
        try:
            verify_installation_inputs({'root': str(template)}, receipt)
        except (ValueError, OSError) as error:
            raise ValueError('Setup: archived candidate inputs disagree with the mapped installation: ' + str(error)) from error
        return {'inputSignature': receipt['inputSignature'], 'inputFiles': receipt['inputFiles']}

    def verify(self, candidate):
        if hashlib.sha256(self.path.read_bytes()).hexdigest() != self.manifest_sha256:
            raise RuntimeError('Registered dependency installation map changed; stop and retain evidence')
        for row in self.rows.values():
            if hashlib.sha256(Path(row['receiptPath']).read_bytes()).hexdigest() != row['receiptSha256']:
                raise RuntimeError('Registered dependency preparation receipt changed; stop and retain evidence')
            if self.receipts[row['id']]['complete']:
                try:
                    verify_installation_inputs(row, self.receipts[row['id']])
                except (ValueError, OSError) as error:
                    raise RuntimeError('Registered prepared dependency input changed; stop and retain evidence: ' + str(error)) from error
        store = self.dependency(candidate)
        if store is not None and store.current_digest() != store.sha256:
            raise RuntimeError('Registered owned installation dependency store changed; stop and retain evidence')


def execute_case(*args, **kwargs):
    expected_runtime = kwargs.pop('expected_runtime', None)
    result = raw_execute_case(*args, **kwargs)
    if result.get('cleanupComplete') is not True or result.get('cleanupErrors'):
        raise RuntimeError('process cleanup uncertain; stop experiment and retain evidence')
    if expected_runtime is not None and result.get('node') != expected_runtime:
        result.update(complete=False, status='error', outcomes={key:'unknown' for key in result['outcomes']})
        result['infrastructureErrors'].append('Node executable changed from the frozen experiment identity')
        write_json(Path(args[1])/'execution.json',result)
    return result


def scope_workspace_adapter(template):
    template = Path(template).resolve()
    package_path = template / WORKSPACE_ADAPTER['package']
    if not package_path.exists():
        return None
    package = read_json(owned_source_path(package_path, template))
    if package.get('name') != WORKSPACE_ADAPTER['specifier']:
        return None
    if package.get('exports') != {'.': WORKSPACE_ADAPTER['export']}:
        raise ValueError('Historical SQLite workspace export is unsupported')
    consumers = []
    for relative in WORKSPACE_ADAPTER['consumers']:
        path = template / relative
        if path.exists():
            consumer = read_json(owned_source_path(path, template))
            declaration = consumer.get('dependencies', {}).get(WORKSPACE_ADAPTER['specifier'])
            if isinstance(declaration, str) and declaration.startswith('workspace:'):
                consumers.append({'file': relative, 'declaration': declaration, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
    if not consumers:
        raise ValueError('Historical SQLite workspace has no declared desktop/hub consumer')
    export_path = package_path.parent / WORKSPACE_ADAPTER['export']
    resolved = owned_source_path(export_path, template)
    if not resolved.is_file():
        raise ValueError('Historical SQLite workspace export is unavailable')
    return {**WORKSPACE_ADAPTER, 'packageSha256': hashlib.sha256(package_path.read_bytes()).hexdigest(),
            'consumerDeclarations': consumers, 'sourcePath': export_path.relative_to(template).as_posix(),
            'exportSha256': hashlib.sha256(resolved.read_bytes()).hexdigest()}


def workspace_import_controls(adapters):
    return [{'specifier': adapter['specifier'], 'expectedWorkspacePath': adapter['sourcePath'],
             'resolverEvidence': 'research-workspace-resolution.json', 'allowUnused': True} for adapter in adapters]


def configure(template, tests, reporter):
    # Original tests and application imports remain unchanged. Discovery, worker
    # count, reporting and the declared workspace route use a trusted replay config.
    config = {'environment': 'node', 'include': tests, 'pool': 'forks', 'maxWorkers': 1,
              'fileParallelism': False, 'retry': 0, 'bail': 0, 'testTimeout': 30000,
              'hookTimeout': 30000, 'reporters': [str(reporter)]}
    if (template / 'packages/shared/src/testing/longTempDir.ts').exists():
        config['setupFiles'] = ['packages/shared/src/testing/longTempDir.ts']
    settings = {'test':config}
    if (template / 'apps/web/src').exists():
        settings['resolve'] = {'alias':{'~':str((template / 'apps/web/src').resolve())}}
    # The alias must follow each fresh execution rather than point into the template.
    adapter = scope_workspace_adapter(template)
    text = 'import {defineConfig} from "vite-plus/test/config";import {fileURLToPath} from "node:url";import fs from "node:fs";import crypto from "node:crypto";\nconst config=' + json.dumps(settings) + ';\n'
    if 'resolve' in settings:
        text += 'config.resolve.alias["~"]=fileURLToPath(new URL("./apps/web/src",import.meta.url));\n'
    if adapter:
        text += ('const root=fileURLToPath(new URL("./",import.meta.url)).replace(/\\/$/,"");'
                 'const sqlite=fileURLToPath(new URL("./packages/sqlite/src/maintenance.ts",import.meta.url));'
                 'config.resolve={alias:[{find:/^@irudd-scope\\/sqlite$/,replacement:sqlite,customResolver(id){'
                 'const realPath=fs.realpathSync(id);if(realPath!==fs.realpathSync(sqlite)||!realPath.startsWith(root+"/"))'
                 'throw new Error("SQLite alias resolved outside owned source");'
                 'fs.writeFileSync(new URL("./research-workspace-resolution.json",import.meta.url),JSON.stringify({'
                 'specifier:"@irudd-scope/sqlite",sourceRoot:root,realPath,sha256:crypto.createHash("sha256").update(fs.readFileSync(realPath)).digest("hex")}));'
                 'return realPath;}}]};\n')
    text += 'export default defineConfig(config);\n'
    (template / 'research-test-value.config.ts').write_text(text)
    return [adapter] if adapter else []


def plan(template, files, cli, output, max_raw_units=None):
    config = {'schemaVersion': 1, 'selection': {'include': files}, 'operators': ['comparison', 'equality', 'logical']}
    if max_raw_units is not None:
        if type(max_raw_units) is not int or not 1 <= max_raw_units <= 16384:
            raise ValueError('Historical planning maxRawUnits must be between 1 and 16384')
        config['limits'] = {'maxRawUnits': max_raw_units}
    write_json(output / 'plan-config.json', config)
    command = [str(cli), 'mutator', 'plan', '--root', str(template), '--config', str(output / 'plan-config.json'), '--json']
    process = run_command(command, template, dict(__import__('os').environ), output / 'planning', timeout=60)
    if process['status'] != 'finished' or process['exitCode']:
        raise ValueError('historical mutation plan failed')
    value = read_json(output / 'planning/stdout.txt')
    if not value.get('complete'):
        raise ValueError('historical mutation plan incomplete')
    write_json(output / 'plan.json', value)
    return value


def run_fault(candidate, dependencies, cli, output, reporter, limit, node, runtime_identity, verify_identity, dependency_installation=None, max_raw_units=None, verify_source=None):
    output.mkdir(parents=True)
    preflight=candidate.get('preflightExclusion')
    if preflight is not None:
        if not isinstance(preflight,str) or not preflight.strip() or len(preflight)>1000:
            raise ValueError('invalid historical preflight exclusion')
        return {**candidate,'verified':False,'executionAttempted':False,'exclusion':'Preflight: '+preflight}
    template = output / 'fixed-template'
    repository = Path(candidate['repository'])
    fix = subprocess.check_output(['git', '-C', str(repository), 'rev-parse', candidate['fix'] + '^{commit}'], text=True).strip()
    parent = subprocess.check_output(['git', '-C', str(repository), 'rev-parse', fix + '^'], text=True).strip()
    if fix != candidate['fix'] or parent != candidate['parent']:
        raise ValueError('historical commit identity disagrees with manifest')
    archive(repository, fix, template)
    if verify_source is not None:
        try:
            candidate_inputs = verify_source(template)
        except ValueError as error:
            return {**candidate, 'verified': False, 'executionAttempted': False, 'exclusion': str(error)}
        write_json(output / 'candidate-installation-inputs.json', candidate_inputs)
    adapters = configure(template, candidate['testFiles'], reporter)
    write_json(output / 'workspace-adapters.json', adapters)
    options = dict(dependencies=dependencies, timeout=90, node=str(node), expected_runtime=runtime_identity,
                   import_controls=workspace_import_controls(adapters))
    fixed = execute_case(template, output / 'fixed-before', COMMAND, **options)
    if not fixed['complete'] or fixed['status'] != 'survived':
        return {**candidate, 'verified': False, 'exclusion': 'fixed adapted baseline incomplete or failing', 'fixedEvidence': 'fixed-before/execution.json',
                'workspaceAdapters': adapters}
    faulty = output / 'faulty-template'
    copy_owned_source(template, faulty)
    reverted = []
    for file in candidate['sourceFiles']:
        target = faulty / file
        if not target.resolve().is_relative_to(faulty.resolve()):
            raise ValueError('historical path leaves owned source')
        before = subprocess.check_output(['git', '-C', str(repository), 'show', parent + ':' + file])
        if before != target.read_bytes():
            target.write_bytes(before)
            reverted.append(file)
    if not reverted:
        return {**candidate, 'verified': False, 'exclusion': 'no historical source difference', 'workspaceAdapters': adapters}
    regression = execute_case(faulty, output / 'faulty', COMMAND, baseline=fixed['tests'], **options)
    corrected = execute_case(template, output / 'fixed-after', COMMAND, baseline=fixed['tests'], **options)
    killed = [identity for identity, outcome in regression['outcomes'].items() if outcome == 'killed']
    unaffected = [identity for identity, outcome in regression['outcomes'].items() if outcome == 'notKilled']
    verified = regression['complete'] and bool(killed) and bool(unaffected) and corrected['complete'] and corrected['status'] == 'survived'
    fault = {**candidate, 'verified': verified, 'replay': 'adapted source-reversion replay', 'revertedFiles': reverted,
             'killedBy': killed, 'unaffected': unaffected,
             'fixedTemplateSha256': hash_tree(template)[0], 'faultyTemplateSha256': hash_tree(faulty)[0],
             'dependencySha256': dependencies.sha256,
             'node': runtime_identity, 'workspaceAdapters': adapters,
             'fixedBefore': 'fixed-before/execution.json', 'faulty': 'faulty/execution.json', 'fixedAfter': 'fixed-after/execution.json'}
    # Presence in the parent file distinguishes existing inventory from retrospective
    # fix-revision inventory; exact individual test additions require manual diff audit.
    fault['testFileOrigins'] = {}
    for file in candidate['testFiles']:
        exists = subprocess.run(['git', '-C', str(repository), 'cat-file', '-e', parent + ':' + file], capture_output=True).returncode == 0
        fault['testFileOrigins'][file] = 'parent file existed; individual additions not classified' if exists else 'fix-added regression file'
    parent_tests = output / 'parent-tests-template'
    copy_owned_source(template, parent_tests)
    existing_files = []
    for file in candidate['testFiles']:
        if fault['testFileOrigins'][file].startswith('parent file existed'):
            (parent_tests / file).write_bytes(subprocess.check_output(['git', '-C', str(repository), 'show', parent + ':' + file]))
            existing_files.append(file)
        else:
            (parent_tests / file).unlink()
    if existing_files:
        configure(parent_tests, existing_files, reporter)
        earlier = execute_case(parent_tests, output / 'parent-tests-fixed-source', COMMAND, **options)
        if earlier['complete'] and earlier['status'] == 'survived':
            earlier_ids = {test['id'] for test in earlier['tests']}
            fixed_ids = {test['id'] for test in fixed['tests']}
            fault['unchangedIdentityFromParentTests'] = sorted(earlier_ids & fixed_ids)
            fault['fixAddedOrRenamedTestIdentities'] = sorted(fixed_ids - earlier_ids)
            fault['parentTestInventoryEvidence'] = 'parent-tests-fixed-source/execution.json'
        else:
            fault['parentTestInventoryUnavailable'] = 'parent tests did not pass under adapted fixed source'
    else:
        fault['unchangedIdentityFromParentTests'] = []
        fault['fixAddedOrRenamedTestIdentities'] = [test['id'] for test in fixed['tests']]
    if not verified:
        fault['exclusion'] = 'faulty assertion regression, unaffected control, or restored pass not established'
        return fault
    mutation_plan = plan(template, reverted, cli, output, max_raw_units)
    sites = mutation_plan['sites'][:limit] if limit is not None else mutation_plan['sites']
    matrix = {'schemaVersion': 1, 'baselineComplete': True, 'subject': {'name': candidate['subject'], 'revision': fix, 'faultId': candidate['id']},
              'tests': [{k: test[k] for k in ['id', 'name', 'file', 'durationMs'] if k in test} for test in fixed['tests']], 'mutants': [],
              'provenance': {'replay': fault['replay'], 'sourceSha256': fault['fixedTemplateSha256'], 'dependencySha256': dependencies.sha256,
                             'workspaceAdapters': adapters,
                             'archguardSha256': hashlib.sha256(cli.read_bytes()).hexdigest(), 'planSha256': hash_tree(output / 'planning')[0],
                             'plannedMutants': len(mutation_plan['sites']), 'predeclaredPrefixLimit': limit,
                             'planningMaxRawUnits': max_raw_units if max_raw_units is not None else 8192}}
    if dependency_installation is not None:
        matrix['provenance']['dependencyInstallation'] = dependency_installation
    if verify_source is not None:
        matrix['provenance']['candidateInstallationInputs'] = read_json(output / 'candidate-installation-inputs.json')
    for index, mutation in enumerate(sites):
        run = execute_case(template, output / f'mutant-{index:04d}', COMMAND, baseline=fixed['tests'], mutation=mutation, **options)
        matrix['mutants'].append({'id': mutation['id'], 'status': run['status'], 'outcomes': run['outcomes'], 'mutation': mutation,
                                  'infrastructureErrors': run['infrastructureErrors'], 'evidence': f'mutant-{index:04d}/execution.json'})
        write_json(output / 'matrix.json', matrix)
        print(candidate['id'], index + 1, len(sites), run['status'], flush=True)
    if dependencies.current_digest() != dependencies.sha256:
        matrix['baselineComplete'] = False
        fault['verified'] = False
        fault['exclusion'] = 'owned external dependency store changed'
    verify_identity()
    write_json(output / 'matrix.json', matrix)
    analysis = analyze(matrix)
    write_json(output / 'analysis.json', analysis)
    if analysis['complete']:
        write_json(output / 'evaluation.json', compact_evaluation(evaluate_fault(matrix, fault), matrix))
    else:
        fault['rankingExcluded'] = 'one or more mutation columns incomplete'
    return fault


def register_experiment(output, registration):
    registered = output / 'preregistration.json'
    if registered.exists() and read_json(registered) != registration:
        raise ValueError('experiment identity changed; use a new output directory')
    if not registered.exists():
        write_json(registered, registration)


def verify_experiment_identity(registration, cli, runtimes):
    observed = {'runnerSha256':runner_fingerprint(), 'sdkSha256':hash_tree(REPOSITORY/'sdk')[0],
                'archguardSha256':hashlib.sha256(cli.read_bytes()).hexdigest(),
                'node':{name:node_fingerprint(str(path)) for name,path in runtimes.items()}}
    if any(observed[key]!=registration[key] for key in observed):
        raise RuntimeError('historical experiment implementation or runtime changed; stop and retain evidence')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidates', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--archguard', required=True, type=Path)
    parser.add_argument('--t3-dependencies', type=Path)
    parser.add_argument('--scope-dependencies', type=Path)
    parser.add_argument('--dependency-installations', type=Path)
    parser.add_argument('--mutant-limit', type=int, default=None)
    parser.add_argument('--max-raw-units', type=int, default=None)
    parser.add_argument('--prepare-only', action='store_true')
    parser.add_argument('--t3-node', required=True, type=Path)
    parser.add_argument('--scope-node', required=True, type=Path)
    args = parser.parse_args()
    if args.max_raw_units is not None and not 1 <= args.max_raw_units <= 16384:
        parser.error('--max-raw-units must be between 1 and the product ceiling of 16384')
    if args.dependency_installations:
        if args.t3_dependencies or args.scope_dependencies:
            parser.error('Use an explicit installation map or both global dependency donors, never both modes')
    elif not args.t3_dependencies or not args.scope_dependencies:
        parser.error('Both global dependency donors are required without an explicit installation map')
    install_signal_handlers()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    reporter = Path(__file__).resolve().parent / 'vitest-reporter.ts'
    dependencies = {}
    manifest = read_json(args.candidates)
    candidates = manifest['candidates'] if isinstance(manifest, dict) else manifest
    installations = HistoricalInstallations(args.dependency_installations, candidates, output) if args.dependency_installations else None
    if installations:
        write_json(output / 'dependency-installations.json', installations.registration())
    else:
        for name, installed in [('t3code', args.t3_dependencies), ('scope', args.scope_dependencies)]:
            store_root = output / (name + '-dependencies')
            # Store metadata is loaded only to resume this explicitly owned experiment.
            metadata = output / (name + '-dependencies.json')
            store = DependencyStore(installed, store_root)
            if metadata.exists():
                info = read_json(metadata)
                store.modules, store.workspace_links, store.owned_directories = info['modules'], info['workspaceLinks'], set(info['ownedDirectories'])
                store.sha256 = info['sha256']
                if store.current_digest() != store.sha256:
                    raise ValueError('resumed dependency store has changed')
            else:
                store.copy()
                write_json(metadata, {'modules': store.modules, 'workspaceLinks': store.workspace_links,
                                      'ownedDirectories': sorted(store.owned_directories), 'sha256': store.sha256,
                                      'installedRoot': str(installed.resolve())})
                write_json(output / (name + '-dependency-files.json'), store.manifest)
            dependencies[name] = store
    if args.prepare_only:
        return
    runtimes = {'t3code':args.t3_node.resolve(), 'scope':args.scope_node.resolve()}
    registration = {'candidates': candidates, 'mutantPrefixLimit': args.mutant_limit,
               'planningMaxRawUnits': args.max_raw_units if args.max_raw_units is not None else 8192,
               'testPool': 'fixed-revision tests; retrospective test changes remain a confounder',
               'replay': ('adapted fixed source + first-parent changed-source reversion + explicit lock-matched prepared dependencies' if installations else
                          'adapted fixed source + first-parent changed-source reversion + borrowed current dependencies'),
               'seedCount': 100, 'testFractions': [0.25, 0.5, 0.75, 1], 'unit': 'historical fault, clustered by relatedGroup',
               'operators': ['comparison','equality','logical'], 'runnerSha256': runner_fingerprint(),
               'sourceCopyPolicy': 'preserved validated owned links; internal absolute links rewritten into each fresh copy',
               'workspaceAdapterPolicy': WORKSPACE_ADAPTER,
               'experimentSha256': runner_fingerprint(), 'sdkSha256': hash_tree(REPOSITORY/'sdk')[0],
               'node': {name:node_fingerprint(str(path)) for name,path in runtimes.items()},
               'archguardSha256': hashlib.sha256(args.archguard.resolve().read_bytes()).hexdigest(),
               'dependencySha256': ({key: store.sha256 for key, (store, _) in installations.stores.items()} if installations else
                                    {name:store.sha256 for name,store in dependencies.items()})}
    if installations:
        registration['dependencyInstallations'] = installations.registration()
    register_experiment(output, registration)

    def verify_candidate(candidate):
        verify_experiment_identity(registration, args.archguard.resolve(), runtimes)
        if installations:
            installations.verify(candidate)

    results = []
    for candidate in candidates:
        disk_check(output)
        verify_candidate(candidate)
        directory = output / candidate['id']
        if (directory / 'fault.json').exists():
            result = read_json(directory / 'fault.json')
            for file, expected in result.get('recordDigests', {}).items():
                if hashlib.sha256((directory/file).read_bytes()).hexdigest()!=expected:
                    raise ValueError('retained historical evidence changed')
            verify_candidate(candidate)
        else:
            try:
                profile = installations.profiles[candidate['id']] if installations else None
                if profile and profile.get('setupExclusion') and not candidate.get('preflightExclusion'):
                    result = {**candidate, 'verified': False, 'executionAttempted': False, 'exclusion': profile['setupExclusion']}
                else:
                    store = installations.dependency(candidate) if installations else dependencies[candidate['subject']]
                    result = run_fault(candidate, store, args.archguard.resolve(), directory, reporter, args.mutant_limit,
                                       runtimes[candidate['subject']], registration['node'][candidate['subject']],
                                       lambda:verify_candidate(candidate), dependency_installation=profile, max_raw_units=args.max_raw_units,
                                       verify_source=(lambda template:installations.verify_source(candidate, template)) if installations else None)
            except (ValueError, OSError, subprocess.SubprocessError) as error:
                result = {**candidate, 'verified': False, 'exclusion': str(error)}
            verify_candidate(candidate)
            if installations:
                result['dependencyInstallation'] = installations.profiles[candidate['id']]
            if (directory / 'workspace-adapters.json').exists():
                result['workspaceAdapters'] = read_json(directory / 'workspace-adapters.json')
            if (directory / 'candidate-installation-inputs.json').exists():
                result['candidateInstallationInputs'] = read_json(directory / 'candidate-installation-inputs.json')
            result['recordDigests'] = {file:hashlib.sha256((directory/file).read_bytes()).hexdigest() for file in
                 ['matrix.json','analysis.json','evaluation.json','workspace-adapters.json','candidate-installation-inputs.json','fixed-before/execution.json','faulty/execution.json','fixed-after/execution.json'] if (directory/file).exists()}
            write_json(directory / 'fault.json', result)
        results.append(result)
        write_json(output / 'attempts.json', results)
        evaluations = [read_json(output / value['id'] / 'evaluation.json') for value in results
                       if value.get('verified') and (output / value['id'] / 'evaluation.json').exists()]
        write_json(output / 'aggregate.json', summarize_faults(evaluations))
        print(candidate['id'], 'verified' if result.get('verified') else 'excluded', result.get('exclusion', ''), flush=True)


if __name__ == '__main__':
    main()
