#!/usr/bin/env python3
"""Compare Stryker test-only invalidation with an imported-helper negative control."""
import argparse
import json
from pathlib import Path
from evaluate import run, setup, source_hashes, write_json


def invoke(root, target, force=False):
    config = json.loads((root / 'stryker.config.json').read_text())
    config.update({'mutate': [target], 'incremental': True})
    (root / 'stryker.config.json').write_text(json.dumps(config))
    result = run(['./node_modules/.bin/stryker', 'run'] + (['--force'] if force else []), root)
    report = root / 'reports/mutation.json'
    if report.exists(): result['report'] = json.loads(report.read_text())
    result['sourceHashes'] = source_hashes(root)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--t3', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists(): raise ValueError('use new output directory')
    args.output.mkdir(parents=True)
    results = {}
    for kind, target in [('tests', 'src/account.ts:15-17'), ('helper', 'src/account.ts:11-13')]:
        root = args.output / kind
        setup(root, 'corrected', args.t3)
        if kind == 'helper':
            (root/'src/account.test.ts').write_text("import {expect,it} from 'vitest'; import {addCredits} from './account'; it('credits',()=>expect(addCredits(12,3)).toBe(15));\n")
        seed = invoke(root, target)
        if seed['code']: raise RuntimeError('Stryker seed failed')
        if kind == 'tests':
            (root/'src/account.test.ts').write_text("import {expect,it} from 'vitest'; import {initialEligibility} from './account'; it('type',()=>expect(typeof initialEligibility()).toBe('boolean'));\n")
        else:
            (root/'src/helper.ts').write_text('export function roundCredits(value: number): number { return 15; }\n')
        differential = invoke(root, target)
        forced = invoke(root, target, force=True)
        results[kind] = {'seed': seed, 'differential': differential, 'forced': forced}
        write_json(args.output / 'stryker-cache.json', results)
    print(args.output)


if __name__ == '__main__': main()
