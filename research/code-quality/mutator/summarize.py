#!/usr/bin/env python3
"""Retain per-site review input, inventories, and bounded manual classifications."""
import argparse
import json
from collections import Counter
from pathlib import Path


def original_text(source, location):
    lines = source.splitlines(keepends=True)
    def offset(point):
        row = point['line'] - 1
        column = point['column'] - 1
        if row < 0 or row >= len(lines) or column < 0:
            raise ValueError('invalid report location')
        encoded = lines[row].encode('utf-16-le')
        if column * 2 > len(encoded):
            raise ValueError('column exceeds line')
        prefix = encoded[:column * 2].decode('utf-16-le')
        return sum(len(line) for line in lines[:row]) + len(prefix)
    first = offset(location['start'])
    last = offset(location['end'])
    if first > last:
        raise ValueError('reversed report location')
    return source[first:last]


def classification(target, line, original, replacement, operator):
    if target.endswith('account.ts'):
        if line == 24 and operator in {'arithmetic', 'ArithmeticOperator'}:
            return 'equivalent', 'Finite integer credit amounts treat signed zero as the same balance; adding or subtracting zero leaves that balance unchanged.'
        if line == 28:
            return 'irrelevant implementation detail', 'Archived plan is intentionally outside the exercised order workflow. NoCoverage is preserved, not relabeled as a test kill.'
        return 'genuinely missing test/spec', 'The weak suite omits an exact boundary, mixed payment flags, positive credit addition, default value, or exact fee. Corrected behavior assertions cover those cases.'
    return 'genuinely missing test/spec', 'Missing test or specification if minute precision is intended. Existing tests assert seconds but omit milliseconds; the source reset alone does not establish a required behavior. Confirm the observable precision contract before requesting an assertion.'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('evidence', type=Path)
    args = parser.parse_args()
    output = args.evidence
    summaries = []
    actionable = []
    for label in ['weak', 'corrected', 'real', 'real-corrected']:
        upstream = json.loads((output / f'{label}-upstream.json').read_text())
        sites = upstream['sites']
        inventories = {'upstream': sites}
        for site in sites:
            if site['upstreamStatus'] == 'survived':
                category, reason = classification(site['file'], site['line'], site['original'], site['mutant'], site['category'])
                actionable.append({'run': label, 'tool': 'upstream', 'file': site['file'], 'line': site['line'],
                    'startByte': site['start'], 'endByte': site['end'], 'original': site['original'],
                    'replacement': site['mutant'], 'operator': site['category'], 'status': site['upstreamStatus'],
                    'classification': category, 'reason': reason,
                    'reproduce': f"python -m mutator --root RUN_DIRECTORY --no-coverage --mutate-all --lines {site['line']} --max-workers 1 --test-command './node_modules/.bin/vitest run --reporter=json' {site['file']}"})
        stryker = json.loads((output / f'{label}-stryker.json').read_text())
        mutants = []
        for filename, entry in stryker.get('report', {}).get('files', {}).items():
            for mutant in entry['mutants']:
                start = mutant['location']['start']; end = mutant['location']['end']
                record = {'file': filename, 'original': original_text(entry['source'], mutant['location']), **mutant}
                mutants.append(record)
                if mutant['status'] == 'Survived':
                    category, reason = classification(filename, start['line'], record['original'], mutant['replacement'], mutant['mutatorName'])
                    actionable.append({'run': label, 'tool': 'stryker', **record, 'classification': category, 'reason': reason,
                        'reproduce': f"./node_modules/.bin/stryker run --mutate '{filename}:{start['line']}-{end['line']}' --force"})
        inventories['stryker'] = mutants
        (output / f'{label}-inventory.json').write_text(json.dumps(inventories, indent=2)+'\n')
        summaries.append({'run': label, 'upstreamSeconds': upstream['seconds'],
            'upstreamStatuses': dict(Counter(s['upstreamStatus'] for s in sites)),
            'observedStatuses': dict(Counter(s.get('observedStatus',s['upstreamStatus']) for s in sites)),
            'strykerSeconds': stryker['seconds'], 'strykerExit': stryker['code'],
            'strykerStatuses': dict(Counter(s['status'] for s in mutants)),
            'upstreamOperators': dict(Counter(s['category'] for s in sites)),
            'strykerOperators': dict(Counter(s['mutatorName'] for s in mutants))})
    (output / 'review-input.json').write_text(json.dumps(actionable,indent=2)+'\n')
    (output / 'summary.json').write_text(json.dumps(summaries,indent=2)+'\n')
    print(json.dumps(summaries,indent=2))


if __name__ == '__main__':
    main()
