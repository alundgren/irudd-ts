#!/usr/bin/env python3
"""Run declared current-revision TypeScript slices using the owned history stores."""
import argparse
import json
import hashlib
from pathlib import Path
import subprocess
from analyze import analyze
from history import COMMAND, configure, execute_case, plan
from runner import REPOSITORY, DependencyStore, disk_check, hash_tree, install_signal_handlers, node_fingerprint, read_json, runner_fingerprint, write_json
from rust_slice import archive

SUBJECTS = [
 {'id':'t3-preview','subject':'t3code','repository':None,
  'sources':['packages/shared/src/filePreview.ts','packages/shared/src/video.ts','packages/shared/src/image.ts'],
  'tests':['packages/shared/src/filePreview.test.ts'],'limit':200},
 {'id':'t3-execution','subject':'t3code','repository':None,
  'sources':['packages/client-runtime/src/state/threadExecution.ts','packages/client-runtime/src/state/subagentDisplay.ts'],
  'tests':['packages/client-runtime/src/state/threadExecution.test.ts','packages/client-runtime/src/state/subagentDisplay.test.ts'],'limit':200},
 {'id':'scope-events','subject':'scope','repository':None,
  'sources':['apps/desktop/src/workspace/events.ts'], 'tests':['tests/tab-events.test.ts'],'limit':200},
]


def load_store(root, name):
    info = read_json(root / (name + '-dependencies.json'))
    store = DependencyStore(info['installedRoot'], root / (name + '-dependencies'))
    store.modules, store.workspace_links, store.owned_directories = info['modules'], info['workspaceLinks'], set(info['ownedDirectories'])
    store.sha256 = info['sha256']
    if store.current_digest() != store.sha256:
        raise ValueError('owned dependency store changed')
    return store


def run_subject(subject, store, cli, output, node, runtime, verify_identity):
    directory = output / subject['id']
    directory.mkdir(parents=True)
    revision = subject['revision']
    template = directory / 'template'
    archive(Path(subject['repository']),revision,template)
    configure(template,subject['tests'],Path(__file__).resolve().parent/'vitest-reporter.ts')
    options = dict(dependencies=store, timeout=90, node=str(node), expected_runtime=runtime)
    baseline = execute_case(template,directory/'baseline',COMMAND,**options)
    matrix = {'schemaVersion':1,'subject':{**subject,'revision':revision,'scope':'selected tests and comparison/equality/logical operators'},
              'baselineComplete':baseline['complete'] and baseline['status']=='survived',
              'tests':[{k:t[k] for k in ['id','name','file','durationMs'] if k in t} for t in baseline['tests']], 'mutants':[],
              'provenance':{'sourceSha256':hash_tree(template)[0],'dependencySha256':store.sha256,'node':runtime,
                            'archguardSha256':hashlib.sha256(cli.read_bytes()).hexdigest(), 'runnerSha256':runner_fingerprint()}}
    if not matrix['baselineComplete']:
        write_json(directory/'matrix.json',matrix)
        return
    mutation_plan = plan(template,subject['sources'],cli,directory)
    matrix['provenance'].update(plannedMutants=len(mutation_plan['sites']),predeclaredPrefixLimit=subject['limit'])
    repeats = []
    for repeat in range(5):
        result = execute_case(template,directory/f'baseline-repeat-{repeat}',COMMAND,baseline=baseline['tests'],**options)
        repeats.append({'complete':result['complete'],'status':result['status'], 'tests':result['tests'], 'evidence':f'baseline-repeat-{repeat}/execution.json'})
    write_json(directory/'baseline-repeats.json',repeats)
    matrix['baselineComplete'] = matrix['baselineComplete'] and all(r['complete'] and r['status']=='survived' for r in repeats)
    if matrix['baselineComplete']:
        for index,mutation in enumerate(mutation_plan['sites'][:subject['limit']]):
            result=execute_case(template,directory/f'mutant-{index:04d}',COMMAND,baseline=baseline['tests'],mutation=mutation,**options)
            matrix['mutants'].append({'id':mutation['id'],'status':result['status'],'outcomes':result['outcomes'],'mutation':mutation,
                'infrastructureErrors':result['infrastructureErrors'],'evidence':f'mutant-{index:04d}/execution.json'})
            write_json(directory/'matrix.json',matrix)
            print(subject['id'],index+1,len(mutation_plan['sites'][:subject['limit']]),result['status'],flush=True)
    if store.current_digest()!=store.sha256:
        matrix['baselineComplete']=False
        matrix['provenance']['dependencyStoreChanged']=True
    verify_identity()
    write_json(directory/'matrix.json',matrix)
    write_json(directory/'analysis.json',analyze(matrix))


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--stores',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--archguard',type=Path,required=True)
    p.add_argument('--t3-repository',type=Path,required=True);p.add_argument('--scope-repository',type=Path,required=True)
    p.add_argument('--t3-node',type=Path,required=True);p.add_argument('--scope-node',type=Path,required=True)
    args=p.parse_args();install_signal_handlers()
    for subject in SUBJECTS:
        subject['repository']=str((args.t3_repository if subject['subject']=='t3code' else args.scope_repository).resolve())
        subject['revision']=subprocess.check_output(['git','-C',subject['repository'],'rev-parse','HEAD'],text=True).strip()
    args.output.mkdir(parents=True)
    stores={name:load_store(args.stores,name) for name in ['t3code','scope']}
    runtimes={'t3code':args.t3_node.resolve(),'scope':args.scope_node.resolve()}
    registration={'subjects':SUBJECTS,'node':{name:node_fingerprint(str(path)) for name,path in runtimes.items()},
                  'runnerSha256':runner_fingerprint(),'sdkSha256':hash_tree(REPOSITORY/'sdk')[0],
                  'archguardSha256':hashlib.sha256(args.archguard.resolve().read_bytes()).hexdigest(),
                  'dependencySha256':{name:store.sha256 for name,store in stores.items()}}
    write_json(args.output/'preregistration.json',registration)
    def verify_identity():
        observed={'node':{name:node_fingerprint(str(path)) for name,path in runtimes.items()},
                  'runnerSha256':runner_fingerprint(),'sdkSha256':hash_tree(REPOSITORY/'sdk')[0],
                  'archguardSha256':hashlib.sha256(args.archguard.resolve().read_bytes()).hexdigest()}
        if any(observed[key]!=registration[key] for key in observed):
            raise RuntimeError('current experiment implementation or runtime changed; stop and retain evidence')
    for subject in SUBJECTS:
        disk_check(args.output)
        verify_identity()
        run_subject(subject,stores[subject['subject']],args.archguard.resolve(),args.output.resolve(),
                    runtimes[subject['subject']],registration['node'][subject['subject']],verify_identity)
        verify_identity()

if __name__=='__main__':main()
