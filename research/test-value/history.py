#!/usr/bin/env python3
"""Historical source-reversion experiments with an explicit adapted test environment."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
from analyze import analyze, compact_evaluation, evaluate_fault, summarize_faults
from runner import HERE, REPOSITORY, DependencyStore, direct_vitest_command, disk_check, execute_case as raw_execute_case, hash_tree, install_signal_handlers, node_fingerprint, read_json, runner_fingerprint, run_command, write_json
from rust_slice import archive

COMMAND = direct_vitest_command('research-test-value.config.ts')


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


def configure(template, tests, reporter):
    # Original tests and application imports remain unchanged. Only test discovery,
    # worker count and reporting are replaced, so every result is an adapted replay.
    config = {'environment': 'node', 'include': tests, 'pool': 'forks', 'maxWorkers': 1,
              'fileParallelism': False, 'retry': 0, 'bail': 0, 'testTimeout': 30000,
              'hookTimeout': 30000, 'reporters': [str(reporter)]}
    if (template / 'packages/shared/src/testing/longTempDir.ts').exists():
        config['setupFiles'] = ['packages/shared/src/testing/longTempDir.ts']
    settings = {'test':config}
    if (template / 'apps/web/src').exists():
        settings['resolve'] = {'alias':{'~':str((template / 'apps/web/src').resolve())}}
    # The alias must follow each fresh execution rather than point into the template.
    text = 'import {defineConfig} from "vite-plus/test/config";import {fileURLToPath} from "node:url";\nconst config=' + json.dumps(settings) + ';\n'
    if 'resolve' in settings:
        text += 'config.resolve.alias["~"]=fileURLToPath(new URL("./apps/web/src",import.meta.url));\n'
    text += 'export default defineConfig(config);\n'
    (template / 'research-test-value.config.ts').write_text(text)


def plan(template, files, cli, output):
    config = {'schemaVersion': 1, 'selection': {'include': files}, 'operators': ['comparison', 'equality', 'logical']}
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


def run_fault(candidate, dependencies, cli, output, reporter, limit, node, runtime_identity, verify_identity):
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
    configure(template, candidate['testFiles'], reporter)
    options = dict(dependencies=dependencies, timeout=90, node=str(node), expected_runtime=runtime_identity)
    fixed = execute_case(template, output / 'fixed-before', COMMAND, **options)
    if not fixed['complete'] or fixed['status'] != 'survived':
        return {**candidate, 'verified': False, 'exclusion': 'fixed adapted baseline incomplete or failing', 'fixedEvidence': 'fixed-before/execution.json'}
    faulty = output / 'faulty-template'
    shutil.copytree(template, faulty)
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
        return {**candidate, 'verified': False, 'exclusion': 'no historical source difference'}
    regression = execute_case(faulty, output / 'faulty', COMMAND, baseline=fixed['tests'], **options)
    corrected = execute_case(template, output / 'fixed-after', COMMAND, baseline=fixed['tests'], **options)
    killed = [identity for identity, outcome in regression['outcomes'].items() if outcome == 'killed']
    unaffected = [identity for identity, outcome in regression['outcomes'].items() if outcome == 'notKilled']
    verified = regression['complete'] and bool(killed) and bool(unaffected) and corrected['complete'] and corrected['status'] == 'survived'
    fault = {**candidate, 'verified': verified, 'replay': 'adapted source-reversion replay', 'revertedFiles': reverted,
             'killedBy': killed, 'unaffected': unaffected,
             'fixedTemplateSha256': hash_tree(template)[0], 'faultyTemplateSha256': hash_tree(faulty)[0],
             'dependencySha256': dependencies.sha256,
             'node': runtime_identity,
             'fixedBefore': 'fixed-before/execution.json', 'faulty': 'faulty/execution.json', 'fixedAfter': 'fixed-after/execution.json'}
    # Presence in the parent file distinguishes existing inventory from retrospective
    # fix-revision inventory; exact individual test additions require manual diff audit.
    fault['testFileOrigins'] = {}
    for file in candidate['testFiles']:
        exists = subprocess.run(['git', '-C', str(repository), 'cat-file', '-e', parent + ':' + file], capture_output=True).returncode == 0
        fault['testFileOrigins'][file] = 'parent file existed; individual additions not classified' if exists else 'fix-added regression file'
    parent_tests = output / 'parent-tests-template'
    shutil.copytree(template, parent_tests)
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
    mutation_plan = plan(template, reverted, cli, output)
    sites = mutation_plan['sites'][:limit] if limit is not None else mutation_plan['sites']
    matrix = {'schemaVersion': 1, 'baselineComplete': True, 'subject': {'name': candidate['subject'], 'revision': fix, 'faultId': candidate['id']},
              'tests': [{k: test[k] for k in ['id', 'name', 'file', 'durationMs'] if k in test} for test in fixed['tests']], 'mutants': [],
              'provenance': {'replay': fault['replay'], 'sourceSha256': fault['fixedTemplateSha256'], 'dependencySha256': dependencies.sha256,
                             'archguardSha256': hashlib.sha256(cli.read_bytes()).hexdigest(), 'planSha256': hash_tree(output / 'planning')[0],
                             'plannedMutants': len(mutation_plan['sites']), 'predeclaredPrefixLimit': limit}}
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
    parser.add_argument('--t3-dependencies', required=True, type=Path)
    parser.add_argument('--scope-dependencies', required=True, type=Path)
    parser.add_argument('--mutant-limit', type=int, default=None)
    parser.add_argument('--prepare-only', action='store_true')
    parser.add_argument('--t3-node', required=True, type=Path)
    parser.add_argument('--scope-node', required=True, type=Path)
    args = parser.parse_args()
    install_signal_handlers()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    reporter = Path(__file__).resolve().parent / 'vitest-reporter.ts'
    dependencies = {}
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
    manifest = read_json(args.candidates)
    candidates = manifest['candidates'] if isinstance(manifest, dict) else manifest
    runtimes = {'t3code':args.t3_node.resolve(), 'scope':args.scope_node.resolve()}
    registration = {'candidates': candidates, 'mutantPrefixLimit': args.mutant_limit,
               'testPool': 'fixed-revision tests; retrospective test changes remain a confounder',
               'replay': 'adapted fixed source + first-parent changed-source reversion + borrowed current dependencies',
               'seedCount': 100, 'testFractions': [0.25, 0.5, 0.75, 1], 'unit': 'historical fault, clustered by relatedGroup',
               'operators': ['comparison','equality','logical'], 'runnerSha256': runner_fingerprint(),
               'experimentSha256': runner_fingerprint(), 'sdkSha256': hash_tree(REPOSITORY/'sdk')[0],
               'node': {name:node_fingerprint(str(path)) for name,path in runtimes.items()},
               'archguardSha256': hashlib.sha256(args.archguard.resolve().read_bytes()).hexdigest(),
               'dependencySha256': {name:store.sha256 for name,store in dependencies.items()}}
    register_experiment(output, registration)
    results = []
    for candidate in candidates:
        disk_check(output)
        verify_experiment_identity(registration,args.archguard.resolve(),runtimes)
        directory = output / candidate['id']
        if (directory / 'fault.json').exists():
            result = read_json(directory / 'fault.json')
            for file, expected in result.get('recordDigests', {}).items():
                if hashlib.sha256((directory/file).read_bytes()).hexdigest()!=expected:
                    raise ValueError('retained historical evidence changed')
        else:
            try:
                result = run_fault(candidate, dependencies[candidate['subject']], args.archguard.resolve(), directory, reporter, args.mutant_limit,
                                   runtimes[candidate['subject']], registration['node'][candidate['subject']],
                                   lambda:verify_experiment_identity(registration,args.archguard.resolve(),runtimes))
            except (ValueError, OSError, subprocess.SubprocessError) as error:
                result = {**candidate, 'verified': False, 'exclusion': str(error)}
            verify_experiment_identity(registration,args.archguard.resolve(),runtimes)
            result['recordDigests'] = {file:hashlib.sha256((directory/file).read_bytes()).hexdigest() for file in
                 ['matrix.json','analysis.json','evaluation.json','fixed-before/execution.json','faulty/execution.json','fixed-after/execution.json'] if (directory/file).exists()}
            write_json(directory / 'fault.json', result)
        results.append(result)
        write_json(output / 'attempts.json', results)
        evaluations = [read_json(output / value['id'] / 'evaluation.json') for value in results
                       if value.get('verified') and (output / value['id'] / 'evaluation.json').exists()]
        write_json(output / 'aggregate.json', summarize_faults(evaluations))
        print(candidate['id'], 'verified' if result.get('verified') else 'excluded', result.get('exclusion', ''), flush=True)


if __name__ == '__main__':
    main()
