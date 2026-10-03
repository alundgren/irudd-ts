#!/usr/bin/env python3
"""Assert recorded failure, correction, negative-control and reliability outcomes."""
import argparse
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('evidence', type=Path)
    args = parser.parse_args()
    def load(name): return json.loads((args.evidence / name).read_text())
    weak = load('weak-upstream.json'); corrected = load('corrected-upstream.json')
    for line in [4, 8, 12, 16, 20]:
        assert next(s for s in weak['sites'] if s['line'] == line)['upstreamStatus'] == 'survived'
        site = next(s for s in corrected['sites'] if s['line'] == line)
        assert site['upstreamStatus'] == site['observedStatus'] == 'killed'
    for result in [weak, corrected]:
        assert next(s for s in result['sites'] if s['line'] == 24 and s['original'] == '+')['upstreamStatus'] == 'survived'
        assert next(s for s in result['sites'] if s['line'] == 28)['upstreamStatus'] == 'uncovered'
        assert result['sourceRestored']
    assert [s['upstreamStatus'] for s in load('real-upstream.json')['sites']] == ['killed', 'survived']
    assert [s['upstreamStatus'] for s in load('real-corrected-upstream.json')['sites']] == ['killed', 'killed']
    for kind in ['tests', 'helper', 'dependency']:
        result = load(f'cache-{kind}.json')
        assert not result['seed']['baselineFailed']
        assert result['differential']['calls'] == []
        assert any(s['upstreamStatus'] == 'killed' for s in result['differential']['sites'])
        assert any(s['upstreamStatus'] == 'survived' for s in result['forced']['sites'])
    for kind in ['config', 'baseline']:
        result = load(f'cache-{kind}.json')
        assert result['differential']['calls'] == []
        assert result['forced']['baselineFailed'] and len(result['forced']['calls']) == 1
        assert result['forced']['sites'] == []
    result = load('reliability.json')
    for name, observed in [('harness-error', 'execution-error'), ('timeout', 'timeout')]:
        site = next(s for s in result[name]['sites'] if s['line'] == 16)
        assert site['upstreamStatus'] == 'killed' and site['observedStatus'] == observed
        assert result[name]['sourceRestored']
    assert set(result['invalid-mutant']['upstreamOutcomes'].values()) == {'killed'}
    assert result['invalid-mutant']['calls'][-1]['observedStatus'] == 'execution-error'
    assert result['baselineAfterExceptions']['code'] == 0
    assert result['exception']['workersRemoved'] and result['sourceRestored']
    assert result['changedTestFiles'] == []
    assert result['changedHelperFiles'] == ['src/helper.ts']
    assert len(result['changedFunctionRun']['calls']) == 2
    cache = load('stryker-cache.json')
    def statuses(result):
        return {m['mutatorName']: m['status'] for f in result['report']['files'].values() for m in f['mutants']}
    assert statuses(cache['tests']['seed'])['BooleanLiteral'] == 'Killed'
    assert statuses(cache['tests']['differential'])['BooleanLiteral'] == 'Survived'
    assert statuses(cache['helper']['seed'])['ArithmeticOperator'] == 'Killed'
    assert statuses(cache['helper']['differential'])['ArithmeticOperator'] == 'Killed'
    assert statuses(cache['helper']['forced'])['ArithmeticOperator'] == 'Survived'
    print('Recorded failure/correction, negative controls, cache and execution outcomes verified')


if __name__ == '__main__': main()
