import collections
import hashlib
import json
from pathlib import Path
import re

ROOT = Path('/home/dev/.t3/worktrees/irudd-ts/t3code-dcd18ca5')
OUT = Path(__file__).parent
REPORT = ROOT / 'research/local/code-quality-product/t3-hostClassification-r10.json'
report = json.loads(REPORT.read_text())
source = (OUT / 'original/hostClassification.ts').read_text()
survivors = [item for item in report['results'] if item['outcome'] == 'survived']
probe = json.loads((OUT / 'probe-results.json').read_text())
probes = {item['mutationId']: item for item in probe['results']}
additional = json.loads((OUT / 'remaining-probe-results.json').read_text())
raw = {item['mutationId']: item for item in additional['witnessResults']}
consumer = {item['mutationId']: item for item in additional['consumerResults'] if item['mutationId']}
prior_path = Path('/tmp/archguard-quality-product-host-boundary-selection-review/equivalent-mutations.json')
prior = json.loads(prior_path.read_text())
prior_ids = {item['mutationId'] for item in prior['mutations']}

# Independently determine which zero literals are unread or removed by the
# existing matcher. The earlier equivalence list is used only as a cross-check.
prefix_literals = {}
for match in re.finditer(r'ipv6PrefixMatches\(ipv6, \[([^\]]+)\], (\d+)\)', source):
    bits = int(match[2])
    for index, literal in enumerate(re.finditer(r'0x[0-9a-fA-F]+|\d+', match[1])):
        start = match.start(1) + literal.start()
        prefix_literals[start] = {'index': index, 'bits': bits, 'value': int(literal[0], 0)}

equivalence = {}
for mutant in survivors:
    literal = prefix_literals.get(mutant['location']['start'])
    if literal and mutant['expected'] == '0' and mutant['replacement'] == '1':
        full, remainder = divmod(literal['bits'], 16)
        index = literal['index']
        if index > full or (index == full and remainder == 0):
            equivalence[mutant['mutationId']] = {
                'reason': f"Prefix entry {index} is outside the {literal['bits']}-bit comparison. The matcher reads entries 0 through {full - 1}" + (f" and the high {remainder} bits of entry {full}." if remainder else ". It returns before reading further entries."),
                'proof': 'source-proof-for-all-string-inputs-under-frozen-current-callers',
                'details': literal,
            }
        elif index == full and remainder:
            mask = (0xffff << (16 - remainder)) & 0xffff
            if (0 & mask) == (1 & mask):
                equivalence[mutant['mutationId']] = {
                    'reason': f"The {literal['bits']}-bit prefix reads entry {index} through mask {mask:#06x}. Both 0 and 1 become 0 under that mask.",
                    'proof': 'source-proof-for-all-string-inputs-under-frozen-current-callers',
                    'details': {**literal, 'mask': mask},
                }

remainder_id = '0f3c6bf3d1d3cf96d7fe44db9009ee3f0ea42b8ba63cd66f22131ef2ca2f4cf8'
lengths = sorted({literal['bits'] for literal in prefix_literals.values()})
remainders = sorted({value % 16 for value in lengths})
assert remainders == [0, 7, 12]
equivalence[remainder_id] = {
    'reason': 'Current prefix lengths are 16, 23, 28, 32, 48 and 96, so the possible remainders are 0, 7 and 12. Changing the zero shortcut to one never introduces a new shortcut. For remainder zero, the unchanged mask is zero and the final comparison is 0 === 0. For remainders 7 and 12, execution is unchanged.',
    'proof': 'source-proof-for-all-string-inputs-under-frozen-current-callers',
    'details': {'prefixLengths': lengths, 'remainders': remainders},
}
assert set(equivalence) == prior_ids, (set(equivalence) - prior_ids, prior_ids - set(equivalence))
suffix_id = '42b04b7fd95243da54fecc774df1410a3d54b3f87ed1963754f3ee921f75be6b'
equivalence[suffix_id] = {
    'reason': 'All current reserved suffixes have one leading dot and otherwise contain neither dots nor colons. Equality with suffix.slice(1) can therefore only match a bare name such as test, which already returns false through isPrivateNetworkHost before the suffix check. Equality with suffix.slice(0) only matches the entire dotted suffix, which already satisfies endsWith(suffix). All other inputs are unchanged.',
    'proof': 'source-proof-for-all-string-inputs-under-frozen-current-suffix-list',
    'details': {'suffixes': ['.alt', '.example', '.internal', '.invalid', '.onion', '.test']},
}

rows = []
for mutant in survivors:
    item = {key: mutant[key] for key in ['mutationId', 'operator', 'location', 'expected', 'replacement']}
    item['sourceLine'] = source.splitlines()[mutant['location']['line'] - 1]
    evidence = probes[mutant['mutationId']]
    item['originalOutcome'] = mutant['outcome']
    item['sampleDifferenceCounts'] = {name: values['count'] for name, values in evidence['differences'].items()}
    if mutant['mutationId'] in equivalence:
        item['classification'] = 'equivalent'
        item.update(equivalence[mutant['mutationId']])
        assert all(value == 0 for value in item['sampleDifferenceCounts'].values())
    elif evidence['existingFailures']:
        first = evidence['existingFailures'][0]
        item['classification'] = 'useful_selection_gap'
        item['reason'] = f"The selected profile omits the existing T3 browser test '{first['testName']}'. Its assertion expects {first['functionName']}({first['host']!r}) to be {first['expected']}, but this mutant returns {first.get('actual', first.get('error'))}."
        item['proof'] = 'observed-direct-module-difference-against-existing-assertions-not-full-vitest-replay'
        item['details'] = {'testFile': 'apps/web/src/browser/browserTargetResolver.test.ts', 'testBlockLines': [220, 275] if first['functionName'] == 'isPrivateNetworkHost' else [276, 349], 'witness': first, 'existingAssertionDifferenceCount': len(evidence['existingFailures'])}
    elif mutant['mutationId'] in consumer:
        test = next(test for test in consumer[mutant['mutationId']]['cases'] if test['detected'])
        item['classification'] = 'useful_selection_gap'
        item['reason'] = f"The selected shared tests omit the existing resolver test '{test['testName']}'. With prepared base {test['base']}, that test expects {test['expected']}; the adapted original resolver code returns {test['actual']} under this mutant."
        item['proof'] = 'observed-existing-consumer-case-through-adapted-original-source-not-full-vitest-replay'
        item['details'] = {'testFile': 'apps/web/src/browser/browserTargetResolver.test.ts', 'testFirstLine': test['firstLine'], 'witness': test, 'adaptation': additional['consumerAdaptation']}
    elif mutant['mutationId'] in raw:
        witness = raw[mutant['mutationId']]
        item['classification'] = 'useful_spec_gap'
        item['reason'] = witness['reason'] + f" isPublicFaviconHost({witness['host']!r}) changes from {witness['original']} to {witness['mutated']}."
        item['proof'] = 'observed-exported-classifier-difference-with-source-reviewed-input-validity'
        item['details'] = {'witness': witness, 'scope': 'Direct exported host-string API. The inspected selected/shared tests and two existing browser classification blocks do not exercise this case. No claim of absence from every T3 test.'}
    elif mutant['mutationId'] == additional['implementationDetail']['mutationId']:
        witness = additional['implementationDetail']
        item['classification'] = 'implementation_detail'
        item['reason'] = 'Changing the guard from OR to AND allows Number.parseInt to read a valid private prefix from malformed text such as fc00invalid::1. The exported isPrivateNetworkHost result changes from false to true, while isPublicFaviconHost remains false. Current production callers pass URL.hostname, which rejects this malformed address before classification.'
        item['proof'] = 'observed-malformed-raw-export-difference-and-source-proof-of-unchanged-public-favicon-result'
        item['details'] = {
            'witness': witness,
            'publicFaviconProof': 'A difference requires a first token rejected by the full-hextet regex but partially accepted by parseInt, with a private IPv6 mask. The original private classifier returns false, then parseIpv6Address rejects the invalid token and public eligibility returns false. The mutant private classifier returns true, so public eligibility also returns false. Valid tokens and no-colon inputs that can reach this guard have identical results.',
            'callerSource': ['apps/web/src/browser/browserTargetResolver.ts:31', 'packages/shared/src/favicon.ts:92'],
        }
    else:
        item['classification'] = 'unresolved'
        item['reason'] = 'No supported classification was established.'
        item['proof'] = 'unresolved'
    rows.append(item)

counts = dict(collections.Counter(item['classification'] for item in rows))
assert len(rows) == 153 and len({item['mutationId'] for item in rows}) == 153
assert all(re.fullmatch('[0-9a-f]{64}', item['mutationId']) for item in rows)
assert counts == {'useful_spec_gap': 13, 'useful_selection_gap': 92, 'equivalent': 47, 'implementation_detail': 1}
assert sum(bool(item['existingFailures']) for item in probe['results']) == 88
assert not any(item['sharedFailures'] for item in probe['results'])
classification = {
    'schemaVersion': 1, 'frozenT3Revision': '31a9da179ed0763335f05681c577474aec5d2309',
    'sourceSha256': hashlib.sha256(source.encode()).hexdigest(),
    'originalSummary': report['summary'], 'originalBaseline': report['baseline'],
    'counts': counts, 'unresolved': 0,
    'details': {'selectionGapsDetectedByTwoExistingBoundaryBlocks': 88, 'selectionGapsDetectedByOtherExistingResolverCases': 4, 'additionalPublicClassifierCases': 13, 'validIpv6AdditionalCases': 9, 'malformedIpv6AdditionalCases': 4, 'additionalCasesThatDifferAfterUrlCanonicalization': 8, 'validAdditionalCasesOnlyDifferBeforeUrlCanonicalization': 1, 'malformedAdditionalCasesRejectedByUrlParsing': 4, 'malformedPrivateExportOnly': 1, 'priorEquivalentsIndependentlyVerified': 46, 'additionalEquivalent': 1},
    'limitations': [
        'Equivalence claims use source proofs under the frozen private helper, current literal callers and suffix list. They do not claim equivalence if those change.',
        'The direct probe executes edited pure modules in memory. It does not rerun the production mutation runner or replace Vitest mutation outcomes.',
        'Zero differences on 4431 sampled host strings are corroboration only. They are not the basis for any equivalence claim.',
        'Additional cases demonstrate direct API behavior differences. Four use malformed input rejected by current URL callers, and one valid input is canonicalized before those callers classify it.',
        'No whole-repository coverage claim is made. Test absence is limited to inspected selected/shared tests and the two existing classification blocks.',
        'No production source, frozen upstream source, dependency installation, or original mutation report was changed.',
    ],
    'survivors': rows,
}
(OUT / 'survivor-classification.json').write_text(json.dumps(classification, indent=2) + '\n')

lines = ['mutationId\toperator\tline\tstart\tend\texpected\treplacement\tclassification\tproof\treason']
for item in rows:
    values = [item['mutationId'], item['operator'], item['location']['line'], item['location']['start'], item['location']['end'], item['expected'], item['replacement'], item['classification'], item['proof'], item['reason']]
    lines.append('\t'.join(str(value).replace('\t', ' ').replace('\n', ' ') for value in values))
(OUT / 'survivor-classification.tsv').write_text('\n'.join(lines) + '\n')

provenance = {
    'frozenT3Revision': classification['frozenT3Revision'],
    'frozenT3Root': '/tmp/archguard-quality-product-t3code',
    'reviewedArchguardRevision': '54e98675b276069649b4c8d8fce71c7b6c20cdcf',
    'finalCoordinatorRevisionProvidedByParent': '3f9f4e6',
    'sourceFiles': {f'original/{file.name}': hashlib.sha256(file.read_bytes()).hexdigest() for file in sorted((OUT / 'original').iterdir())},
    'license': {'path': 'LICENSE', 'sha256': hashlib.sha256((OUT / 'LICENSE').read_bytes()).hexdigest(), 'notice': 'MIT License, Copyright (c) 2026 T3 Tools Inc.'},
    'originalReport': {'path': str(REPORT), 'sha256': hashlib.sha256(REPORT.read_bytes()).hexdigest()},
    'priorEquivalenceClaims': {'path': str(prior_path), 'sha256': hashlib.sha256(prior_path.read_bytes()).hexdigest()},
    'runtime': probe['nodeVersion'],
    'reproduce': ['node --disable-warning=ExperimentalWarning probe.mjs', 'node --disable-warning=ExperimentalWarning remaining-probes.mjs', 'python3 classify.py'],
    'method': probe['method'],
    'existingAssertions': {'selectedSharedAssertions': probe['sharedCaseCount'], 'omittedTwoBrowserBlocksAssertions': probe['omittedExistingCaseCount'], 'extraction': 'Extracted the string arrays and explicit expect calls from exact frozen test copies. Kept each existing expected boolean; verified every extracted baseline assertion. Additional two environment-port cases retain original resolver function source with imported command dependencies replaced by bounded fixed test stubs.'},
    'artifacts': {file.name: hashlib.sha256(file.read_bytes()).hexdigest() for file in sorted(OUT.iterdir()) if file.is_file() and file.name != 'provenance.json'},
}
(OUT / 'provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
print(json.dumps({'classified': len(rows), 'counts': counts, 'artifact': str(OUT / 'survivor-classification.json')}))
