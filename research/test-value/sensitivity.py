#!/usr/bin/env python3
"""Posthoc sensitivity analysis. Original assertion-only evidence stays unchanged."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import re

from analyze import analyze
from runner import hash_tree, read_json, validate_inventory, write_json
from rust_slice import parse_libtest

POLICY = {
    'id': 'posthoc-test-failures-v1',
    'timing': 'specified after observing assertion-only exclusions',
    'vitest': 'complete fixed inventory; SDK finished; failed test assertion or test-bound exception; no hook/global/import/unhandled failures',
    'rust': 'complete fixed libtest inventory; assertion or Result::unwrap failure at a frozen test-source unwrap site',
    'limits': 'failure origin is not instrumented; exceptions do not establish assertion strength; no timeout promotion',
}


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def classifier_hashes():
    return {name:sha(Path(__file__).parent/name) for name in ['sensitivity.py','analyze.py','runner.py','rust_slice.py']}


def event_hashes(directory, names):
    return {name: sha(directory/name) for name in names if (directory/name).is_file()}


def vitest_outcomes(directory, baseline, frozen_node):
    """Validate original events, then classify individually failed Vitest cases."""
    e = read_json(directory/'execution.json')
    if not e.get('vitest') or e.get('cleanupComplete') is not True or e.get('cleanupErrors') or e.get('residualProcessGroup'):
        raise ValueError('Vitest identity or confirmed cleanup unavailable')
    if e.get('exitCode') not in {0, 1} or e.get('node') != frozen_node or e.get('expectedNode') != frozen_node:
        raise ValueError('process exit or frozen runtime disagrees')
    if e.get('stdoutBytes', 0) + e.get('stderrBytes', 0) > 4*1024*1024:
        raise ValueError('output budget exceeded')
    request, protocol, inventory = [read_json(directory/name) for name in ['request.json','result.json','inventory.json']]
    if request != e.get('request') or protocol != e.get('protocol'):
        raise ValueError('execution record disagrees with original SDK events')
    # The retained request contains the original absolute path. Relocation changes
    # only where the validator reads the same hashed result bytes.
    local_request = {**request, 'resultPath': str(directory/'result.json')}
    records, original_problems = validate_inventory(inventory, protocol, local_request, e['exitCode'], frozen_node)
    if records != e.get('tests'):
        raise ValueError('execution inventory differs from sidecar')
    if {test['id'] for test in records} != {test['id'] for test in baseline}:
        raise ValueError('fixed inventory mismatch')
    if sorted(original_problems) != sorted(e.get('infrastructureErrors', [])):
        raise ValueError('unexplained execution problems remain')
    revised = copy.deepcopy(protocol)
    by_raw = {test['rawId']: test for test in records}
    categories = {'assertionTests': set(), 'otherFailureTests': set()}
    for failure in revised['failures']:
        test = by_raw.get(failure.get('testId'))
        if failure['kind'] not in {'assertion', 'runtime'} or test is None or test['state'] != 'failed':
            raise ValueError('global, hook, import, suite or unhandled failure')
        expected_file = Path(e['cwd']) / test['file']
        if failure.get('file') != str(expected_file):
            raise ValueError('failure file does not identify the owned failed test')
        key = 'assertionTests' if failure['kind'] == 'assertion' else 'otherFailureTests'
        categories[key].add(test['id'])
        # Reuse every inventory/lifecycle check. This local copy does not relabel
        # the original protocol or describe an exception as an assertion.
        failure['kind'] = 'assertion'
    _, problems = validate_inventory(inventory, revised, local_request, e['exitCode'], frozen_node)
    if problems:
        raise ValueError('; '.join(problems))
    if (e['exitCode'] == 0) != all(test['state'] == 'passed' for test in records):
        raise ValueError('exit disagrees with individual results')
    return {test['id']: 'killed' if test['state']=='failed' else 'notKilled' for test in records}, {
        key: sorted(value) for key,value in categories.items()}


def native_sites(template):
    file = 'src/mutator/result.rs'
    lines = (template/file).read_text().splitlines()
    assertions = {(file,n) for n,line in enumerate(lines,1) if re.search(r'^\s*assert(?:_eq|_ne)?!\s*\(',line)}
    start = next(n for n,line in enumerate(lines,1) if line == '#[cfg(test)]')
    unwraps = {(file,n) for n,line in enumerate(lines,1) if n > start and '.unwrap()' in line}
    return assertions, unwraps


def native_outcomes(directory, baseline, assertions, unwraps):
    e = read_json(directory/'execution.json')
    p = e['process']
    if p.get('status')!='finished' or p.get('cleanupComplete') is not True or p.get('cleanupErrors') or p.get('residualProcessGroup'):
        raise ValueError('native command or cleanup incomplete')
    stdout = (directory/'stdout.txt').read_text()
    parsed = parse_libtest(stdout,p['exitCode'],assertions)
    records = parsed['tests']
    if {test['id'] for test in records} != {test['id'] for test in baseline}:
        raise ValueError('native fixed inventory mismatch')
    categories = {'assertionTests':[], 'otherFailureTests':[]}
    for test in records:
        if test['outcome']=='killed':
            categories['assertionTests'].append(test['id'])
        elif test['outcome']=='unknown':
            block = re.search(r'^---- '+re.escape(test['name'])+r' stdout ----\n(.*?)(?=^---- |^failures:|^test result:|\Z)',stdout,re.M|re.S)
            panics = re.findall(r'^thread .* panicked at (.+):(\d+):(\d+):$',block.group(1),re.M) if block else []
            site = (panics[0][0],int(panics[0][1])) if len(panics)==1 else None
            if site not in unwraps or not re.search(r'^called `Result::unwrap\(\)` on an `Err` value:',block.group(1),re.M):
                raise ValueError('native failure is not a frozen test-source Result::unwrap')
            test['outcome']='killed'
            categories['otherFailureTests'].append(test['id'])
    summary = re.search(r'^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;',stdout,re.M)
    if not summary or len(re.findall(r'^test result:',stdout,re.M))!=1:
        raise ValueError('native summary missing or duplicated')
    counts = [int(summary.group(i)) for i in [2,3,4]]
    if counts != [sum(t['outcome']=='notKilled' for t in records),sum(t['outcome']=='killed' for t in records),0] or int(summary.group(5))!=0:
        raise ValueError('native summary disagrees with active results')
    if p['exitCode'] not in {0,101} or (p['exitCode']==0)!=(summary.group(1)=='ok'):
        raise ValueError('native exit disagrees with summary')
    return {test['id']:test['outcome'] for test in records}, categories


def bind_column(directory, mutant, matrix, baseline, native_plan=None):
    e=read_json(directory/'execution.json')
    if native_plan is not None:
        mutation=native_plan['mutants'][int(directory.name.rsplit('-',1)[1])]
        diff=(directory/'mutation.diff').read_text()
        if diff!=mutation['diff'] or hashlib.sha256(diff.encode()).hexdigest()!=mutant['id'] or mutation['name']!=mutant['name']:
            raise ValueError('native mutation does not match planned column')
        if e['process']['command']!=baseline['process']['command'] or e['process']['cwd']!=baseline['process']['cwd']:
            raise ValueError('native command or owned source differs from baseline')
    else:
        if e.get('mutation')!=mutant.get('mutation') or (e.get('mutation') or {}).get('id')!=mutant['id']:
            raise ValueError('execution mutation does not match column')
        if e['request']['mutationId']!=mutant['id'] or e['request']['phase']!='mutation':
            raise ValueError('SDK request is not for this mutation')
        if e['request']['inputDigest']!=baseline['request']['inputDigest']:
            raise ValueError('column SDK, command, environment or runner digest differs from baseline')
        for field,expected in [('templateSha256',matrix['provenance']['sourceSha256']),
                               ('dependencySha256',matrix['provenance']['dependencySha256']),
                               ('runnerSha256',matrix['provenance'].get('runnerSha256',baseline['runnerSha256']))]:
            if e.get(field)!=expected or baseline.get(field)!=expected:
                raise ValueError('column source, dependency or runner identity differs from baseline')


def derive(root, output, native=False):
    matrix = read_json(root/'matrix.json')
    if matrix.get('baselineComplete') is not True:
        raise ValueError('primary baseline is incomplete')
    derived = copy.deepcopy(matrix)
    evidence = {'policy': POLICY, 'classifierModules':classifier_hashes(),
                'primaryMatrixSha256':sha(root/'matrix.json'), 'events':{}}
    baseline_dir=root/('fixed-before' if (root/'fixed-before').exists() else 'baseline')
    baseline=read_json(baseline_dir/'execution.json')
    if baseline.get('complete') is not True or baseline.get('status')!='survived':
        raise ValueError('bound baseline execution is incomplete or failing')
    names=['execution.json','request.json','result.json','inventory.json','stdout.txt','stderr.txt']
    evidence['baselineEvents']=event_hashes(baseline_dir,names)
    if native:
        if hash_tree(root/'template')[0]!=matrix['provenance']['sourceSha256']:
            raise ValueError('native template changed from primary source identity')
        assertions, unwraps = native_sites(root/'template')
        evidence['testSourceSha256']=sha(root/'template/src/mutator/result.rs')
        evidence['unwrapSites']=sorted(unwraps)
        native_plan=read_json(root/'plan.json')
        evidence['planSha256']=sha(root/'plan.json')
        native_outcomes(baseline_dir,matrix['tests'],assertions,unwraps)
    else:
        native_plan=None
        frozen_node=matrix['provenance'].get('node')
        if frozen_node is None:
            registration=read_json(root.parent/'preregistration.json')
            frozen_node=registration['node'][matrix['subject']['name']]
            evidence['registrationSha256']=sha(root.parent/'preregistration.json')
        evidence['frozenNode']=frozen_node
        vitest_outcomes(baseline_dir,matrix['tests'],frozen_node)
    for index,mutant in enumerate(derived['mutants']):
        directory = root/(f'mutant-{index:04d}' if native else Path(mutant['evidence']).parent)
        event_names=names+(['mutation.diff','patch.log'] if native else [])
        evidence['events'][mutant['id']]=event_hashes(directory,event_names)
        bound=False
        try:
            bind_column(directory,mutant,matrix,baseline,native_plan)
            bound=True
            if native:
                outcomes,categories=native_outcomes(directory,matrix['tests'],assertions,unwraps)
            else:
                outcomes,categories=vitest_outcomes(directory,matrix['tests'],frozen_node)
            mutant.update(outcomes=outcomes,status='killed' if 'killed' in outcomes.values() else 'survived', failureCategories=categories,
                          primaryInfrastructureErrors=mutant.get('infrastructureErrors',[]),infrastructureErrors=[])
        except (OSError,ValueError,KeyError) as error:
            mutant['sensitivityExclusion']=str(error)
            if not (bound and native and mutant['status']=='invalid' and mutant.get('invalidReason')=='compilerRejected'):
                mutant.update(status='error',outcomes={t['id']:'unknown' for t in matrix['tests']})
        if event_hashes(directory,event_names)!=evidence['events'][mutant['id']]:
            raise ValueError('retained events changed during derivation')
    if sha(root/'matrix.json')!=evidence['primaryMatrixSha256']:
        raise ValueError('primary matrix changed during derivation')
    if event_hashes(baseline_dir,names)!=evidence['baselineEvents'] or classifier_hashes()!=evidence['classifierModules']:
        raise ValueError('baseline or classifier changed during derivation')
    if native and (sha(root/'template/src/mutator/result.rs')!=evidence['testSourceSha256'] or sha(root/'plan.json')!=evidence['planSha256']):
        raise ValueError('native test source or plan changed during derivation')
    if 'registrationSha256' in evidence and sha(root.parent/'preregistration.json')!=evidence['registrationSha256']:
        raise ValueError('historical registration changed during derivation')
    derived['provenance'].update(sensitivityPolicy=POLICY['id'], primaryMatrixSha256=evidence['primaryMatrixSha256'])
    output.mkdir(parents=True,exist_ok=False)
    write_json(output/'policy-and-events.json',evidence)
    write_json(output/'matrix.json',derived)
    write_json(output/'analysis.json',analyze(derived))
    return derived


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--native',action='store_true')
    a=p.parse_args();derive(a.root.resolve(),a.output.resolve(),a.native)
