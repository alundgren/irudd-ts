#!/usr/bin/env python3
"""Run declared current-revision TypeScript slices using the owned history stores."""
import argparse
import json
from pathlib import Path
import subprocess
from analyze import analyze
from history import COMMAND, configure, execute_case, plan
from runner import DependencyStore, disk_check, hash_tree, install_signal_handlers, read_json, write_json
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


def run_subject(subject, store, cli, output):
    directory = output / subject['id']
    directory.mkdir(parents=True)
    revision = subprocess.check_output(['git','-C',subject['repository'],'rev-parse','HEAD'],text=True).strip()
    template = directory / 'template'
    archive(Path(subject['repository']),revision,template)
    configure(template,subject['tests'],Path(__file__).resolve().parent/'vitest-reporter.ts')
    baseline = execute_case(template,directory/'baseline',COMMAND,dependencies=store,timeout=90)
    matrix = {'schemaVersion':1,'subject':{**subject,'revision':revision,'scope':'selected tests and comparison/equality/logical operators'},
              'baselineComplete':baseline['complete'] and baseline['status']=='survived',
              'tests':[{k:t[k] for k in ['id','name','file','durationMs'] if k in t} for t in baseline['tests']], 'mutants':[],
              'provenance':{'sourceSha256':hash_tree(template)[0],'dependencySha256':store.sha256}}
    if not matrix['baselineComplete']:
        write_json(directory/'matrix.json',matrix)
        return
    mutation_plan = plan(template,subject['sources'],cli,directory)
    matrix['provenance'].update(plannedMutants=len(mutation_plan['sites']),predeclaredPrefixLimit=subject['limit'])
    repeats = []
    for repeat in range(5):
        result = execute_case(template,directory/f'baseline-repeat-{repeat}',COMMAND,dependencies=store,baseline=baseline['tests'],timeout=90)
        repeats.append({'complete':result['complete'],'status':result['status'], 'tests':result['tests'], 'evidence':f'baseline-repeat-{repeat}/execution.json'})
    write_json(directory/'baseline-repeats.json',repeats)
    matrix['baselineComplete'] = matrix['baselineComplete'] and all(r['complete'] and r['status']=='survived' for r in repeats)
    if matrix['baselineComplete']:
        for index,mutation in enumerate(mutation_plan['sites'][:subject['limit']]):
            result=execute_case(template,directory/f'mutant-{index:04d}',COMMAND,dependencies=store,baseline=baseline['tests'],mutation=mutation,timeout=90)
            matrix['mutants'].append({'id':mutation['id'],'status':result['status'],'outcomes':result['outcomes'],'mutation':mutation,
                'infrastructureErrors':result['infrastructureErrors'],'evidence':f'mutant-{index:04d}/execution.json'})
            write_json(directory/'matrix.json',matrix)
            print(subject['id'],index+1,len(mutation_plan['sites'][:subject['limit']]),result['status'],flush=True)
    if store.current_digest()!=store.sha256:
        matrix['baselineComplete']=False
        matrix['provenance']['dependencyStoreChanged']=True
    write_json(directory/'matrix.json',matrix)
    write_json(directory/'analysis.json',analyze(matrix))


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--stores',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--archguard',type=Path,required=True)
    p.add_argument('--t3-repository',type=Path,required=True);p.add_argument('--scope-repository',type=Path,required=True)
    args=p.parse_args();install_signal_handlers()
    for subject in SUBJECTS:
        subject['repository']=str((args.t3_repository if subject['subject']=='t3code' else args.scope_repository).resolve())
    args.output.mkdir(parents=True)
    write_json(args.output/'preregistration.json',SUBJECTS)
    stores={name:load_store(args.stores,name) for name in ['t3code','scope']}
    for subject in SUBJECTS:
        disk_check(args.output)
        run_subject(subject,stores[subject['subject']],args.archguard.resolve(),args.output.resolve())

if __name__=='__main__':main()
