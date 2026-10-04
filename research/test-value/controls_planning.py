#!/usr/bin/env python3
"""Historical planning-budget controls using the actual bounded CLI."""
import argparse
import hashlib
from pathlib import Path

from controls_runner import fixture, require
from history import plan, register_experiment
from runner import install_signal_handlers, private_environment, read_json, run_command, write_json


def controls(output, cli):
    output.mkdir(parents=True, exist_ok=False)
    template = fixture(output / 'template', {'subject.ts': 'export const padding=[' + ','.join(['0'] * 5000) +
                                           '];export function check(value:number){return value<2;}\n'})
    source_sha256 = hashlib.sha256((template / 'subject.ts').read_bytes()).hexdigest()
    before = output / 'default-budget'
    before.mkdir()
    try:
        plan(template, ['subject.ts'], cli, before)
    except ValueError:
        pass
    else:
        raise AssertionError('Default planning must reject the same source above 8192 raw units')
    failed = read_json(before / 'planning/stdout.txt')
    require(not failed['complete'] and 'sourceComplexityLimit' in str(failed), 'Retain the real bounded-source planning failure')
    require('limits' not in read_json(before / 'plan-config.json'), 'Unspecified budget must retain the product default')
    after = output / 'declared-budget'
    after.mkdir()
    corrected = plan(template, ['subject.ts'], cli, after, 16384)
    require(corrected['complete'] and corrected['sites'], 'Declared budget must plan actual mutation edits on unchanged source')
    require(hashlib.sha256((template / 'subject.ts').read_bytes()).hexdigest() == source_sha256, 'Planning must preserve source bytes')
    require(read_json(after / 'plan-config.json')['limits'] == {'maxRawUnits': 16384}, 'Actual CLI configuration must record the declared budget')

    over = output / 'above-ceiling'
    over.mkdir()
    config = read_json(after / 'plan-config.json')
    config['limits']['maxRawUnits'] = 16385
    write_json(over / 'plan-config.json', config)
    process = run_command([str(cli), 'mutator', 'plan', '--root', str(template), '--config', str(over / 'plan-config.json'), '--json'],
                          template, private_environment(over / 'private'), over / 'planning')
    write_json(over / 'planning/execution.json', process)
    require(process['status'] == 'finished' and process['exitCode'] == 2 and process['cleanupComplete'], 'Product must reject configuration above its hard ceiling')
    require('max_raw_units' in (over / 'planning/stderr.txt').read_text(), 'Actual product diagnostic must identify the exceeded limit')
    try:
        plan(template, ['subject.ts'], cli, over, 16385)
    except ValueError:
        pass
    else:
        raise AssertionError('Historical helper must reject above-ceiling budgets before invocation')

    identity = output / 'registration'
    register_experiment(identity, {'planningMaxRawUnits': 8192, 'mutantPrefixLimit': 24})
    try:
        register_experiment(identity, {'planningMaxRawUnits': 16384, 'mutantPrefixLimit': 24})
    except ValueError:
        pass
    else:
        raise AssertionError('A different planning budget must require a new registration')
    require(read_json(identity / 'preregistration.json')['planningMaxRawUnits'] == 8192, 'Rejected identity change must preserve the original registration')
    write_json(output / 'controls.json', {'passed': True, 'groups': 3, 'sourceSha256': source_sha256,
               'archguardSha256': hashlib.sha256(cli.read_bytes()).hexdigest(), 'plannedMutants': len(corrected['sites']),
               'timingUse': 'validation only; not a registered measurement'})
    print('Passed 3 planning-budget control groups', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--archguard', type=Path, required=True)
    args = parser.parse_args()
    install_signal_handlers()
    controls(args.output.resolve(), args.archguard.resolve())


if __name__ == '__main__':
    main()
