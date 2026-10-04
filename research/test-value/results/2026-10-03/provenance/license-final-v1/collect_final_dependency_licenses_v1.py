#!/usr/bin/env python3
"""Read-only license inventory for explicitly mapped pnpm stores.

The collector imports no dependency package. Store traversal requires both a
final-map digest and the explicit --final-scan-confirmed gate.
"""
import argparse
import gzip
import hashlib
import json
import os
import pathlib
import re
import shutil
import sys
from collections import defaultdict

BASE = pathlib.Path('/tmp/archguard-test-value-20261003')
ORIGINAL_PLAN = BASE / 'historical-install-plan.json'
SUPPLEMENT_PLAN = BASE / 'supplement-install-input-plan.json'
ORIGINAL_PLAN_SHA256 = 'b6c13206cd9bf6440177a42a048147f790510f33c98ea9fb5c17cf8fb39df17d'
SUPPLEMENT_PLAN_SHA256 = '37215083110265f0606642fdf86bf72394ebd321d140c6d8257c2e3315c4ff2e'
LICENSE_RESOLUTION = BASE / 'license-resolution-v1.json'
PREVIOUS_INVENTORY = BASE / 'historical-dependency-license-inventory-full-22-v2.json'
PREVIOUS_INVENTORY_SHA256 = 'b0fb4da4acd08faed984c049ad24daa5b892e4f4ec380b4fe24e7bbb8fe2d4ae'
EXPECTED_PROFILES = 46
EXPECTED_BUN_EXCLUSIONS = 2
NAME_RE = re.compile(r'^(?:LICENSE|LICENCE|NOTICE|COPYING|PATENTS?|THIRD_PARTY_NOTICES)(?:$|[._ -])', re.I)
SKIP_DIRS = {'.git', 'node_modules', '.pnpm', '.yarn', '.cache'}
HEX64 = re.compile(r'^[0-9a-f]{64}$')

class AuditError(Exception):
    pass

def fail(message):
    raise AuditError(message)

def sha_bytes(data):
    return hashlib.sha256(data).hexdigest()

def sha_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()

def read_json(path):
    try:
        raw = pathlib.Path(path).read_bytes()
        return json.loads(raw), raw
    except (OSError, json.JSONDecodeError) as e:
        fail(f'cannot read JSON {path}: {type(e).__name__}')

def owned_path(raw, *, regular=False, directory=False, exists=True):
    if not isinstance(raw, str) or not raw:
        fail('expected nonempty owned path')
    path = pathlib.Path(raw)
    if not path.is_absolute():
        path = BASE / path
    try:
        resolved = path.resolve(strict=exists)
    except OSError as e:
        fail(f'cannot resolve path {raw}: {type(e).__name__}')
    if not resolved.is_relative_to(BASE.resolve(strict=True)):
        fail(f'path is outside approved research directory: {raw}')
    if exists and path.is_symlink():
        fail(f'path itself may not be a symlink: {raw}')
    if regular and not resolved.is_file():
        fail(f'expected regular file: {raw}')
    if directory and not resolved.is_dir():
        fail(f'expected directory: {raw}')
    return resolved

def input_path(value):
    if not isinstance(value, str) or not value or '\x00' in value:
        fail('invalid installer input path')
    rel = pathlib.PurePosixPath(value)
    if rel.is_absolute() or '..' in rel.parts:
        fail(f'unsafe installer input path: {value}')
    return rel

def manager(group):
    value = group.get('packageManager')
    if isinstance(value, dict):
        name, version = value.get('name'), value.get('exactVersion')
    elif isinstance(value, str) and '@' in value:
        name, version = value.rsplit('@', 1)
    else:
        name = version = None
    if not name and isinstance(version, str):
        name = 'bun' if version.startswith('1.') else 'pnpm'
    return name, version

def spdx_syntax(value):
    if not isinstance(value, str) or not value.strip():
        return 'unrecognized'
    text = ' '.join(value.strip().split())
    if text.upper() in {'UNLICENSED', 'NOASSERTION'}:
        return 'special-value'
    if text.upper().startswith('SEE LICENSE IN '):
        return 'see-license-reference'
    tokens = re.findall(r'[A-Za-z0-9.+-]+|[()]', text)
    if re.sub(r'[A-Za-z0-9.+-]+|[()]|\s+', '', text):
        return 'unrecognized'
    depth, expect_id = 0, True
    for token in tokens:
        if expect_id:
            if token == '(':
                depth += 1
            elif token in {'AND', 'OR', 'WITH', ')'}:
                return 'unrecognized'
            else:
                expect_id = False
        elif token in {'AND', 'OR', 'WITH'}:
            expect_id = True
        elif token == ')':
            depth -= 1
            if depth < 0:
                return 'unrecognized'
            expect_id = False
        else:
            return 'unrecognized'
    return 'unrecognized' if expect_id or depth != 0 else 'token-syntax-only'

def package_name_from_locator(locator):
    if locator == 'node_modules':
        return None
    separator = locator.find('@', 1) if locator.startswith('@') else locator.find('@')
    if separator <= 0:
        return None
    return locator[:separator].replace('+', '/')

def see_license_reference_status(package):
    declared = package.get('declaredLicense')
    prefix = 'SEE LICENSE IN '
    if not isinstance(declared, str) or not declared.upper().startswith(prefix):
        return None, False
    target = declared[len(prefix):].strip().strip('"\'')
    rel = pathlib.PurePosixPath(target)
    if not target or rel.is_absolute() or '..' in rel.parts or '\\' in target or '\x00' in target:
        return target, False
    normalized = pathlib.PurePosixPath(*(part for part in rel.parts if part not in {'', '.'})).as_posix()
    resolved = any(item.get('path') == normalized and item.get('status') == 'hashed'
                   for item in package.get('licenseNoticeFiles', []))
    return target, resolved

def load_plans(map_data):
    plans = {}
    refs = map_data.get('plans')
    if not isinstance(refs, dict) or set(refs) != {'original', 'supplement'}:
        fail('map plans must contain original and supplement')
    fixed = {
        'original': (ORIGINAL_PLAN, ORIGINAL_PLAN_SHA256),
        'supplement': (SUPPLEMENT_PLAN, SUPPLEMENT_PLAN_SHA256),
    }
    for name, (path, expected_sha) in fixed.items():
        ref = refs[name]
        if not isinstance(ref, dict) or ref.get('path') != str(path) or ref.get('sha256') != expected_sha:
            fail(f'unexpected frozen {name} plan binding')
        path = owned_path(str(path), regular=True)
        if sha_file(path) != expected_sha:
            fail(f'frozen {name} plan hash mismatch')
        data, _ = read_json(path)
        if not isinstance(data.get('groups'), list):
            fail(f'{name} plan has no groups')
        candidate_key = 'groupId' if name == 'original' else 'installInputGroupId'
        by_group = defaultdict(list)
        for candidate in data.get('candidates', []):
            gid = candidate.get(candidate_key)
            if gid:
                by_group[gid].append(candidate)
        groups = {}
        for group in data['groups']:
            gid = group.get('id')
            if not isinstance(gid, str) or gid in groups:
                fail(f'{name} plan has a missing or duplicate group id')
            files = group.get('inputFiles')
            if not isinstance(files, list):
                fail(f'{name}:{gid} lacks inputFiles')
            hashes = {}
            for item in files:
                rel = input_path(item.get('path'))
                digest = item.get('sha256')
                if not isinstance(digest, str) or not HEX64.fullmatch(digest) or str(rel) in hashes:
                    fail(f'{name}:{gid} has invalid or repeated input {rel}')
                hashes[str(rel)] = digest
            if isinstance(group.get('inputFileHashes'), dict) and group['inputFileHashes'] != hashes:
                fail(f'{name}:{gid} inputFileHashes differs from inputFiles')
            groups[gid] = {
                'plan': name, 'data': group, 'inputs': hashes,
                'candidates': by_group.get(gid, []), 'planSha256': expected_sha,
            }
        if len(groups) != 25:
            fail(f'{name} frozen plan contains {len(groups)} groups, expected 25')
        plans[name] = groups
    return plans

def node_version(ref):
    return (ref['data'].get('node') or {}).get('plannedRuntimeVersion')

FINAL_MAP = BASE / 'dependency-installations-lock-matched-78.json'
FINAL_MAP_SHA256 = '81ed6f1a0048b5819212c8817e1d0a724a3365e66128e8d9d3985521b72416a0'
FINAL_MANIFEST = BASE / 'history-lock-matched-78-final.json'
FINAL_MANIFEST_SHA256 = '2a01d9d364071b7bc1b315f89a117d698a03693f74f6c8c647ce92461cb5b0a0'
SOURCE_MAP_REFS = [
    ('/private/tmp/archguard-test-value-20261003/historical-installs-filtered-v3/dependency-installations.json',
     'eb3c32f31377a748aa3a1ae96624a260aabc122fdbfca78f4a59477ae49a027c'),
    ('/private/tmp/archguard-test-value-20261003/historical-installs-supplement-filtered-v3/dependency-installations.json',
     '35bd7859459444056c869fd4b3a1ac100197087ab0c5018d8e8e27f3139dd00e'),
]
REGISTRATION_PATH = BASE / 'historical-bundled-runner-corrected-receipts/registration.json'
REGISTRATION_SHA256 = 'e33c8dae4c16679221d63007c561cbcb5553fd731e56e33e1e0bfb87b22a2fc8'
CORRECTED = {
    't3code-install-3f6293e594ec': {
        'path': BASE / 'historical-bundled-runner-corrected-receipts/t3code-install-3f6293e594ec/receipt.json',
        'sha256': 'da1dd81b7511e02567e140332117a3320c8f8e0dd093c7e3681b2695a7c10e02',
        'oldSha256': 'eb05a64493a191608303f753dc5f2e00f8d68cd0d5632ce7b23076b266080643',
    },
    't3code-supp-install-027d2efae574': {
        'path': BASE / 'historical-bundled-runner-corrected-receipts/t3code-supp-install-027d2efae574/receipt.json',
        'sha256': '534b652ed7d430cdbe0bbffada3131748838363e773eb3f0b07591c169bceddf',
        'oldSha256': '46dd09e0e1b9a91f3d05f88cd6b716617774046d640022d1e935bcd7c261a3a8',
    },
}
EXPECTED_PROFILE = 'lock-matched-filtered-declared-runner-v1'
OUT_GZIP = BASE / 'historical-dependency-license-inventory-lock-matched-78-v1.json.gz'
OUT_RECEIPT = BASE / 'historical-dependency-license-inventory-lock-matched-78-v1.receipt.json'

def path_key(path):
    return str(pathlib.Path(path).resolve(strict=True))

def verify_frozen_file(ref, expected_path, expected_sha, label):
    if not isinstance(ref, dict) or not isinstance(ref.get('path'), str) or ref.get('sha256') != expected_sha:
        fail(f'{label} reference differs from frozen value')
    actual_path = owned_path(ref['path'], regular=True)
    canonical_expected = pathlib.Path(expected_path).resolve(strict=True)
    if actual_path != canonical_expected:
        fail(f'{label} path differs from frozen value')
    if sha_file(actual_path) != expected_sha:
        fail(f'{label} SHA-256 differs from frozen value')
    return actual_path

def load_final_map():
    map_path = owned_path(str(FINAL_MAP), regular=True)
    if sha_file(map_path) != FINAL_MAP_SHA256:
        fail('final 78-route map hash mismatch')
    previous = owned_path(str(PREVIOUS_INVENTORY), regular=True)
    if sha_file(previous) != PREVIOUS_INVENTORY_SHA256:
        fail('preserved full-22 inventory hash mismatch')
    map_data, _ = read_json(map_path)
    if map_data.get('schemaVersion') != 1 or map_data.get('profile') != EXPECTED_PROFILE:
        fail('final map schema/profile mismatch')
    if map_data.get('t3WorkspaceFilter') != '!t3code-relay':
        fail('final map T3 workspace filter differs from reviewed setting')
    if map_data.get('baselineValidationReusedAsMeasurement') is not False:
        fail('final map incorrectly reuses baseline validation as measurement')
    if not isinstance(map_data.get('installations'), list) or len(map_data['installations']) != EXPECTED_PROFILES:
        fail('final map must contain 46 installations')
    if not isinstance(map_data.get('candidates'), dict) or len(map_data['candidates']) != 78:
        fail('final map must contain 78 candidate routes')
    verify_frozen_file(map_data.get('candidatesManifest'), FINAL_MANIFEST, FINAL_MANIFEST_SHA256, 'candidate manifest')
    refs = map_data.get('sourceMaps')
    if not isinstance(refs, list) or len(refs) != len(SOURCE_MAP_REFS):
        fail('final map source-map count mismatch')
    source_data = []
    for i, (path, digest) in enumerate(SOURCE_MAP_REFS):
        resolved = verify_frozen_file(refs[i], path, digest, f'source map {i + 1}')
        source_map, _ = read_json(resolved)
        source_data.append(source_map)
    verify_frozen_file(map_data.get('correctedReceiptsRegistration'), REGISTRATION_PATH,
                       REGISTRATION_SHA256, 'corrected receipt registration')
    registration, _ = read_json(REGISTRATION_PATH)
    if registration.get('validationOnly') is not True:
        fail('corrected runner registration is not marked validation-only')
    if not isinstance(map_data.get('mapHelperSha256'), str) or not HEX64.fullmatch(map_data['mapHelperSha256']):
        fail('final map helper digest is invalid')
    return map_data, source_data, registration

def load_final_plans():
    synthetic = {'plans': {
        'original': {'path': str(ORIGINAL_PLAN), 'sha256': ORIGINAL_PLAN_SHA256},
        'supplement': {'path': str(SUPPLEMENT_PLAN), 'sha256': SUPPLEMENT_PLAN_SHA256},
    }}
    return load_plans(synthetic)

def validate_corrected_receipts(map_data, registration):
    source_refs = {path_key(ref['path']): ref['sha256'] for ref in map_data['sourceMaps']}
    reg_maps = registration.get('originalPreparationMaps', {})
    if not isinstance(reg_maps, dict):
        fail('corrected receipt registration has no preparation-map bindings')
    for raw_path, digest in reg_maps.items():
        if path_key(raw_path) not in source_refs or source_refs[path_key(raw_path)] != digest:
            fail('corrected receipt registration does not bind the two frozen preparation maps')
    node = registration.get('node', {})
    if (node.get('version') != 'v24.21.0' or
            node.get('sha256') != 'e4b5a3af0e05c75de2eae013904145f40fe7fc2a6e6f17510128bf45cca4e79b'):
        fail('corrected receipt registration Node identity mismatch')
    old_receipts = registration.get('originalReceipts', {})
    installs = {x.get('id'): x for x in map_data.get('installations', [])}
    source_installs = {}
    for ref in map_data['sourceMaps']:
        source_map, _ = read_json(owned_path(ref['path'], regular=True))
        for row in source_map.get('installations', []):
            source_installs.setdefault(row['id'], row)
    for gid, expected in CORRECTED.items():
        row = installs.get(gid)
        if row is None or pathlib.Path(row['receiptPath']).resolve(strict=True) != expected['path'].resolve(strict=True):
            fail(f'{gid} map row does not use its corrected receipt')
        if row.get('receiptSha256') != expected['sha256'] or sha_file(expected['path']) != expected['sha256']:
            fail(f'{gid} corrected receipt digest mismatch')
        receipt, _ = read_json(expected['path'])
        correction = receipt.get('correctionOf', {})
        old_path, old_sha = correction.get('path'), correction.get('sha256')
        original = source_installs.get(gid)
        if (not original or path_key(old_path) != path_key(original.get('receiptPath', '')) or
                original.get('receiptSha256') != expected['oldSha256'] or old_sha != expected['oldSha256'] or
                old_receipts.get(old_path) != old_sha or correction.get('complete') is not False):
            fail(f'{gid} corrected receipt does not bind the original failed receipt')
        if receipt.get('complete') is not True or (receipt.get('runnerValidation', {}).get('process') or {}).get('exitCode') != 0:
            fail(f'{gid} corrected runner validation evidence is incomplete')
        rv = receipt['runnerValidation']
        node_map = rv.get('nodeModulesBeforeAndAfter')
        if (rv.get('runnerSha256') != registration.get('runnerSha256') or
                rv.get('helperSha256') != registration.get('helperSha256') or
                (rv.get('node') or {}).get('sha256') != node.get('sha256') or
                (rv.get('node') or {}).get('version') != node.get('version') or
                rv.get('nodeModulesHashAlgorithm') != 'runner-hash-tree-v1' or
                not isinstance(node_map, dict) or not node_map or
                any(not isinstance(v, str) or not HEX64.fullmatch(v) for v in node_map.values()) or
                not str(rv.get('stdout', '')).startswith('vp test/')):
            fail(f'{gid} corrected runner identity, output, or before/after hashes mismatch')
    return {
        'registrationPath': str(REGISTRATION_PATH), 'registrationSha256': REGISTRATION_SHA256,
        'correctedReceipts': [{'id': gid, 'path': str(row['path']), 'sha256': row['sha256'],
            'supersedesReceiptSha256': row['oldSha256'], 'validationOnly': True}
            for gid, row in sorted(CORRECTED.items())],
        'node': node, 'runnerSha256': registration.get('runnerSha256'),
        'helperSha256': registration.get('helperSha256'),
    }

def candidate_plan_index(plans):
    found = defaultdict(list)
    for name, groups in plans.items():
        for gid, ref in groups.items():
            for candidate in ref['candidates']:
                found[candidate.get('id')].append((name, gid, candidate))
    return found

def validate_final_inputs(map_data, source_data, registration, plans):
    manifest, _ = read_json(FINAL_MANIFEST)
    final_candidates = manifest.get('candidates')
    if not isinstance(final_candidates, list) or len(final_candidates) != 78:
        fail('frozen candidate manifest must contain 78 candidate rows')
    candidate_rows = {c.get('id'): c for c in final_candidates if isinstance(c, dict)}
    if len(candidate_rows) != 78 or set(candidate_rows) != set(map_data['candidates']):
        fail('candidate identities differ between frozen manifest and final map')
    plan_candidates = candidate_plan_index(plans)
    if set(plan_candidates) != set(candidate_rows):
        fail('frozen plan candidate identities differ from final manifest')
    route_exclusions = []
    execution_exclusions = []
    candidate_groups = {}
    install_ids = {x['id'] for x in map_data['installations']}
    for cid, row in candidate_rows.items():
        matches = plan_candidates[cid]
        if len(matches) != 1:
            fail(f'{cid} has {len(matches)} frozen plan candidate rows')
        plan_name, gid, candidate = matches[0]
        profile_id = map_data['candidates'][cid]
        if not isinstance(profile_id, str) or gid != profile_id:
            fail(f'{cid} route profile differs from its frozen installation-input group')
        for field in ('subject', 'repository', 'fix', 'parent'):
            if row.get(field) != candidate.get(field):
                fail(f'{cid} {field} differs between final manifest and frozen plan')
        group = plans[plan_name][gid]['data']
        candidate_groups[cid] = {'plan': plan_name, 'groupId': gid, 'profileId': profile_id}
        if row.get('preflightEligible') is False:
            pm_name, pm_version = manager(group)
            reason = row.get('preflightExclusionReason') or row.get('preflightExclusion')
            if pm_name != 'bun' or pm_version != '1.3.9' or not reason or profile_id in install_ids:
                fail(f'{cid} has an unsupported or undocumented preflight exclusion')
            route_exclusions.append({'candidateId': cid, 'profileId': profile_id,
                'packageManager': f'{pm_name}@{pm_version}', 'reason': reason})
        elif profile_id not in install_ids:
            fail(f'{cid} eligible route lacks a completed installation')
        if row.get('preflightEligible') is not False and row.get('preflightExclusion'):
            reason = row['preflightExclusion']
            excluded_test = reason.split(' requires ', 1)[0]
            if excluded_test not in row.get('testFiles', []):
                fail(f'{cid} file-level preflight exclusion does not identify a listed test path')
            execution_exclusions.append({'candidateId': cid, 'subject': row['subject'],
                'testPath': excluded_test, 'reason': reason})
    if len(route_exclusions) != EXPECTED_BUN_EXCLUSIONS:
        fail(f'expected {EXPECTED_BUN_EXCLUSIONS} exact Bun preflight exclusions')

    union_candidates, union_installs = {}, {}
    for source_map in source_data:
        candidates = source_map.get('candidates')
        installs = source_map.get('installations')
        if not isinstance(candidates, dict) or not isinstance(installs, list):
            fail('a frozen preparation map is missing its candidate or installation map')
        for cid, profile_id in candidates.items():
            if cid in union_candidates:
                fail(f'{cid} appears in both frozen preparation maps')
            union_candidates[cid] = profile_id
        for row in installs:
            iid = row.get('id')
            if not isinstance(iid, str):
                fail('a frozen preparation map contains an invalid installation id')
            old = union_installs.get(iid)
            if old and any(old.get(k) != row.get(k) for k in ('subject', 'root', 'receiptPath', 'receiptSha256')):
                fail(f'{iid} reused installation metadata differs between preparation maps')
            union_installs[iid] = row
    if union_candidates != map_data['candidates']:
        fail('final candidate routes do not equal the two frozen preparation maps')
    installations = {x.get('id'): x for x in map_data['installations']}
    if len(installations) != EXPECTED_PROFILES or set(installations) != set(union_installs):
        fail('final physical installation rows differ from preparation-map union')
    excluded_ids = {x['candidateId'] for x in route_exclusions}
    installed_routes = {map_data['candidates'][cid] for cid in candidate_rows if cid not in excluded_ids}
    if installed_routes != set(installations):
        fail('eligible candidate routes do not cover exactly the 46 installed profiles')

    correction_proof = validate_corrected_receipts(map_data, registration)
    rows = []
    for iid, row in sorted(installations.items()):
        source_row = union_installs[iid]
        correction = CORRECTED.get(iid)
        if row.get('subject') != source_row.get('subject') or path_key(row['root']) != path_key(source_row['root']):
            fail(f'{iid} final installation root/subject differs from its preparation map')
        if correction is None:
            if path_key(row['receiptPath']) != path_key(source_row['receiptPath']) or row.get('receiptSha256') != source_row.get('receiptSha256'):
                fail(f'{iid} final receipt differs from successful preparation-map receipt')
        else:
            old_receipt_path = owned_path(source_row['receiptPath'], regular=True)
            if sha_file(old_receipt_path) != correction['oldSha256']:
                fail(f'{iid} original failed receipt digest mismatch')
            old_receipt, _ = read_json(old_receipt_path)
            if old_receipt.get('complete') is not False:
                fail(f'{iid} original receipt was not marked incomplete')
        receipt_path = owned_path(row['receiptPath'], regular=True)
        receipt_sha = sha_file(receipt_path)
        if receipt_sha != row.get('receiptSha256'):
            fail(f'{iid} receipt digest differs from final map')
        receipt, _ = read_json(receipt_path)
        if receipt.get('id') != iid or receipt.get('subject') != row.get('subject'):
            fail(f'{iid} receipt identity mismatch')
        if path_key(receipt.get('installationRoot', '')) != path_key(row['root']):
            fail(f'{iid} receipt installation root differs from final map')
        matching = [ref for groups in plans.values() for gid, ref in groups.items()
                    if gid == iid and ref['data'].get('revision') == receipt.get('revision')]
        if len(matching) != 1:
            fail(f'{iid} receipt revision does not identify exactly one frozen install group')
        ref = matching[0]
        group = ref['data']
        if receipt.get('repository') != group.get('repository') or receipt.get('sourceTree') != group.get('rootTreeId'):
            fail(f'{iid} receipt repository/source tree differs from frozen group')
        expected_inputs = ref['inputs']
        if receipt.get('inputFiles') != expected_inputs:
            fail(f'{iid} receipt input hashes differ from frozen group')
        if receipt.get('complete') is not True or receipt.get('inputFilesUnchanged') is not True or receipt.get('changedArchivedInputs') != []:
            fail(f'{iid} receipt does not attest complete unchanged installer inputs')
        if (receipt.get('process') or {}).get('exitCode') != 0:
            fail(f'{iid} install process did not finish successfully')
        safety = receipt.get('safeguards') or {}
        if safety != {'frozenLockfile': True, 'lifecycleScripts': False, 'pnpmHooks': False,
                      'automaticPackageManagerSwitch': False, 'automaticRuntimeInstall': False}:
            fail(f'{iid} install safeguards differ from reviewed values')
        pm_name, pm_version = manager(group)
        if pm_name != 'pnpm' or receipt.get('pmVersion') != pm_version:
            fail(f'{iid} package-manager version differs from frozen plan')
        expected_node = node_version(ref)
        if (receipt.get('nodeVersion') or '').lstrip('v') != expected_node:
            fail(f'{iid} Node version differs from frozen plan')
        if row['subject'] == 't3code' and receipt.get('workspaceFilter') != '--filter=!t3code-relay':
            fail(f'{iid} T3 receipt is missing the reviewed relay exclusion')
        if row['subject'] == 'scope' and receipt.get('workspaceFilter'):
            fail(f'{iid} Scope receipt has an unexpected workspace filter')
        rows.append({'id': iid, 'subject': row['subject'], 'root': row['root'],
            'receiptPath': row['receiptPath'], 'receiptSha256': receipt_sha,
            'plan': ref['plan'], 'repository': group['repository'], 'revision': group['revision'],
            'rootTreeId': group['rootTreeId'], 'inputFileCount': len(expected_inputs),
            'inputFilesUnchanged': True, 'frozenLockfile': True, 'lifecycleScripts': False,
            'pnpmHooks': False, 'automaticPackageManagerSwitch': False,
            'automaticRuntimeInstall': False, 'packageManager': f'{pm_name}@{pm_version}',
            'nodeVersion': expected_node, 'workspaceFilter': receipt.get('workspaceFilter'),
            'candidateIds': sorted(cid for cid, route in map_data['candidates'].items() if route == iid)})
    return {'installationRows': rows, 'candidateRouteCount': len(candidate_rows),
        'installedRouteCount': sum(len(r['candidateIds']) for r in rows),
        'bunPreflightExclusions': sorted(route_exclusions, key=lambda x: x['candidateId']),
        'testExecutionExclusions': sorted(execution_exclusions, key=lambda x: (x['candidateId'], x['testPath'])),
        'candidateGroupBindings': candidate_groups, 'correctedReceiptProof': correction_proof,
        'sourceMapBindings': [{'path': p, 'sha256': h} for p, h in SOURCE_MAP_REFS],
        'finalMapSha256': FINAL_MAP_SHA256, 'candidateManifestSha256': FINAL_MANIFEST_SHA256,
        'originalPlanSha256': ORIGINAL_PLAN_SHA256, 'supplementPlanSha256': SUPPLEMENT_PLAN_SHA256}

def license_paths_safe(package_root, store_root):
    out = []
    package_real = package_root.resolve(strict=True)
    store_real = store_root.resolve(strict=True)
    for dirpath, dirs, files in os.walk(package_real, followlinks=False):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS and not (pathlib.Path(dirpath) / d).is_symlink()]
        for name in files:
            if not NAME_RE.match(name):
                continue
            path = pathlib.Path(dirpath) / name
            row = {'path': path.relative_to(package_real).as_posix(), 'isSymlink': path.is_symlink()}
            try:
                resolved = path.resolve(strict=True)
                if not resolved.is_relative_to(store_real):
                    row['status'] = 'symlink-target-outside-install-root'
                else:
                    data = resolved.read_bytes()
                    row.update(status='hashed', sha256=sha_bytes(data), bytes=len(data))
            except OSError as e:
                row.update(status='unreadable', errorType=type(e).__name__)
            out.append(row)
    return sorted(out, key=lambda x: x['path'])

def root_notices_safe(source):
    found = []
    source_real = source.resolve(strict=True)
    for dirpath, dirs, files in os.walk(source_real, followlinks=False):
        rel_dir = pathlib.Path(dirpath).relative_to(source_real)
        if rel_dir.parts and any(part in {'.repos', 'node_modules', '.git', '.pnpm'} for part in rel_dir.parts):
            dirs[:] = []
            continue
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS and d != '.repos' and not (pathlib.Path(dirpath) / d).is_symlink()]
        for name in files:
            if not NAME_RE.match(name):
                continue
            path = pathlib.Path(dirpath) / name
            item = {'path': path.relative_to(source_real).as_posix(), 'isSymlink': path.is_symlink()}
            try:
                resolved = path.resolve(strict=True)
                if not resolved.is_relative_to(source_real):
                    item['status'] = 'symlink-target-outside-source'
                else:
                    data = resolved.read_bytes()
                    item.update(status='hashed', sha256=sha_bytes(data), bytes=len(data))
            except OSError as e:
                item.update(status='unreadable', errorType=type(e).__name__)
            found.append(item)
    return sorted(found, key=lambda x: x['path'])

def source_local_packages(source, group, expected_inputs):
    local = {}
    source_real = source.resolve(strict=True)
    for rel, digest in expected_inputs.items():
        if (rel != 'package.json' and not rel.endswith('/package.json')) or '.repos/' in rel:
            continue
        path = source / rel
        try:
            resolved = path.resolve(strict=True)
            if not resolved.is_relative_to(source_real) or not resolved.is_file():
                fail(f'{group["id"]}:{rel} resolves outside the installed source')
            raw = resolved.read_bytes()
            if sha_bytes(raw) != digest:
                fail(f'{group["id"]}:{rel} no longer matches frozen package input hash')
            manifest = json.loads(raw)
        except (OSError, json.JSONDecodeError) as e:
            fail(f'{group["id"]}:{rel} cannot be read safely: {type(e).__name__}')
        if isinstance(manifest, dict) and isinstance(manifest.get('name'), str) and isinstance(manifest.get('version'), str):
            local[(manifest['name'], manifest['version'])] = rel
    return local

def scan_installation(row, group_ref, package_records, excluded_workspace, locate_failures):
    source = owned_path(row['root'], directory=True)
    store = source / 'node_modules' / '.pnpm'
    if store.is_symlink():
        fail(f'{row["id"]} pnpm virtual store path is a symlink')
    try:
        store_real = store.resolve(strict=True)
    except OSError as e:
        fail(f'{row["id"]} pnpm virtual store is missing: {type(e).__name__}')
    if not store_real.is_relative_to(source.resolve(strict=True)) or not store_real.is_dir():
        fail(f'{row["id"]} pnpm virtual store resolves outside source')
    group = group_ref['data']
    local_packages = source_local_packages(source, group, group_ref['inputs'])
    install_instances, local_project_instances = [], []
    virtual_entries = [x for x in store_real.iterdir()
                       if x.name != 'node_modules' and (x.is_dir() or x.is_symlink())]
    for locator_dir in virtual_entries:
        if locator_dir.is_symlink():
            locate_failures.append({'groupId': row['id'], 'virtualStoreEntry': locator_dir.name,
                'reason': 'virtual-store entry is a symlink'})
            continue
        if not locator_dir.is_dir():
            continue
        package_name = package_name_from_locator(locator_dir.name)
        if not package_name:
            locate_failures.append({'groupId': row['id'], 'virtualStoreEntry': locator_dir.name,
                'reason': 'not a package locator'})
            continue
        package_link = locator_dir / 'node_modules' / package_name
        package_json = package_link / 'package.json'
        try:
            resolved_package = package_link.resolve(strict=True)
            resolved_json = package_json.resolve(strict=True)
        except OSError:
            locate_failures.append({'groupId': row['id'], 'virtualStoreEntry': locator_dir.name,
                'packageNameGuess': package_name, 'reason': 'package path or package.json cannot resolve'})
            continue
        if '.repos' in resolved_package.parts:
            excluded_workspace[row['id']].append({'virtualStoreEntry': locator_dir.name,
                'packageName': package_name, 'reason': 'local research workspace package under .repos; excluded and never copied'})
            continue
        if not resolved_package.is_relative_to(source.resolve(strict=True)):
            locate_failures.append({'groupId': row['id'], 'virtualStoreEntry': locator_dir.name,
                'packageNameGuess': package_name, 'reason': 'package resolves outside archived install source'})
            continue
        if not resolved_package.is_relative_to(store_real) or not resolved_json.is_relative_to(store_real):
            excluded_workspace[row['id']].append({'virtualStoreEntry': locator_dir.name,
                'packageName': package_name, 'reason': 'local workspace package outside virtual store; excluded from third-party inventory'})
            continue
        try:
            package_bytes = resolved_json.read_bytes()
            manifest = json.loads(package_bytes)
        except (OSError, json.JSONDecodeError) as e:
            locate_failures.append({'groupId': row['id'], 'virtualStoreEntry': locator_dir.name,
                'packageNameGuess': package_name, 'reason': 'package.json unreadable or invalid', 'errorType': type(e).__name__})
            continue
        if not isinstance(manifest, dict) or not isinstance(manifest.get('name'), str) or not isinstance(manifest.get('version'), str):
            locate_failures.append({'groupId': row['id'], 'virtualStoreEntry': locator_dir.name,
                'packageNameGuess': package_name, 'reason': 'package manifest lacks name/version'})
            continue
        name, version, declared = manifest['name'], manifest['version'], manifest.get('license')
        if (name, version) in local_packages:
            local_project_instances.append({'virtualStoreEntry': locator_dir.name, 'packageName': name,
                'version': version, 'sourcePackagePath': local_packages[(name, version)],
                'reason': 'first-party workspace/file dependency; excluded from third-party package inventory'})
            continue
        docs = license_paths_safe(resolved_package, store_real)
        package_json_sha = sha_bytes(package_bytes)
        fingerprint_data = {'name': name, 'version': version, 'packageJsonSha256': package_json_sha,
                            'declaredLicense': declared, 'licenseFiles': docs}
        fingerprint = sha_bytes(json.dumps(fingerprint_data, sort_keys=True, separators=(',', ':')).encode())
        record = package_records.get(fingerprint)
        if record is None:
            record = {'id': f'{name}@{version}#{fingerprint[:16]}', 'fingerprintSha256': fingerprint,
                'name': name, 'version': version, 'declaredLicense': declared,
                'declaredLicenseStatus': spdx_syntax(declared), 'packageJsonSha256': package_json_sha,
                'licenseNoticeFiles': docs, 'observedInGroups': [], 'virtualStoreEntries': [],
                'patchedVirtualStoreEntryObserved': False}
            package_records[fingerprint] = record
        if row['id'] not in record['observedInGroups']:
            record['observedInGroups'].append(row['id'])
        entry = {'groupId': row['id'], 'virtualStoreEntry': locator_dir.name,
                 'packagePath': package_link.relative_to(source).as_posix()}
        if entry not in record['virtualStoreEntries']:
            record['virtualStoreEntries'].append(entry)
        if 'patch_hash=' in locator_dir.name:
            record['patchedVirtualStoreEntryObserved'] = True
        install_instances.append(fingerprint)
    group_result = {'id': row['id'], 'subject': row['subject'], 'revision': row['revision'],
        'installationRoot': row['root'], 'receiptPath': row['receiptPath'], 'receiptSha256': row['receiptSha256'],
        'virtualStoreEntryCount': len(virtual_entries),
        'thirdPartyPackageInstancesInventoried': len(install_instances),
        'distinctPackageEvidenceRecords': len(set(install_instances)),
        'researchOrWorkspacePackagesExcluded': len(excluded_workspace[row['id']]),
        'firstPartyWorkspaceOrFilePackagesExcluded': len(local_project_instances),
        'rootUpstreamNoticeFiles': root_notices_safe(source)}
    return group_result, local_project_instances

def safe_output_path(path):
    out = pathlib.Path(path)
    parent = owned_path(str(out.parent), directory=True)
    if not parent.is_relative_to(BASE.resolve(strict=True)):
        fail('output parent is outside approved research directory')
    if out.exists() or out.is_symlink():
        fail(f'refusing to overwrite existing output: {out}')
    return out

def atomic_exclusive_write(path, data):
    path = safe_output_path(path)
    temp = path.with_name(path.name + f'.tmp-{os.getpid()}')
    fd = os.open(temp, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.link(temp, path)
        temp.unlink()
    except BaseException:
        try:
            temp.unlink()
        except OSError:
            pass
        raise
    return path

def run_preflight():
    map_data, source_data, registration = load_final_map()
    plans = load_final_plans()
    proof = validate_final_inputs(map_data, source_data, registration, plans)
    print(json.dumps({'status': 'metadata-preflight-passed; package stores not inspected',
        'profile': EXPECTED_PROFILE, 'installations': EXPECTED_PROFILES,
        'candidateRoutes': proof['candidateRouteCount'], 'installedRoutes': proof['installedRouteCount'],
        'bunPreflightExclusions': len(proof['bunPreflightExclusions']),
        'testExecutionExclusions': proof['testExecutionExclusions'],
        'correctedRunnerReceipts': len(proof['correctedReceiptProof']['correctedReceipts']),
        'finalMapSha256': FINAL_MAP_SHA256, 'originalPlanSha256': ORIGINAL_PLAN_SHA256,
        'supplementPlanSha256': SUPPLEMENT_PLAN_SHA256}, sort_keys=True))

def run_scan(expected_map_sha256):
    if expected_map_sha256 != FINAL_MAP_SHA256:
        fail('explicit scan confirmation digest differs from frozen final map')
    map_data, source_data, registration = load_final_map()
    plans = load_final_plans()
    proof = validate_final_inputs(map_data, source_data, registration, plans)
    package_records, excluded_workspace = {}, defaultdict(list)
    locate_failures, local_rows, group_rows = [], defaultdict(list), []
    for row in proof['installationRows']:
        group_ref = next((ref for groups in plans.values() for gid, ref in groups.items()
                          if gid == row['id'] and ref['data'].get('revision') == row['revision']), None)
        if group_ref is None:
            fail(f'{row["id"]} validated plan group disappeared before scan')
        result, local = scan_installation(row, group_ref, package_records, excluded_workspace, locate_failures)
        group_rows.append(result)
        local_rows[row['id']].extend(local)
    packages = sorted(package_records.values(), key=lambda x: (x['name'], x['version'], x['fingerprintSha256']))
    for p in packages:
        p['observedInGroups'].sort()
        p['virtualStoreEntries'].sort(key=lambda x: (x['groupId'], x['virtualStoreEntry']))
    missing = [p for p in packages if p['declaredLicenseStatus'] == 'unrecognized' and p['declaredLicense'] is None]
    unrecognized = [p for p in packages if p['declaredLicenseStatus'] == 'unrecognized' and p['declaredLicense'] is not None]
    without_docs = [p for p in packages if not p['licenseNoticeFiles']]
    unreadable = [p for p in packages if any(x.get('status') != 'hashed' for x in p['licenseNoticeFiles'])]
    unresolved_see = []
    for package in packages:
        if package['declaredLicenseStatus'] != 'see-license-reference':
            continue
        target, resolved = see_license_reference_status(package)
        if not resolved:
            package = dict(package)
            package['referencedLicensePath'] = target
            unresolved_see.append(package)
    effect = [p for p in packages if p['name'] in {'effect', '@effect/vitest'}]
    root_notice_map = {}
    for group in group_rows:
        for notice in group['rootUpstreamNoticeFiles']:
            if notice.get('sha256'):
                key = (notice['path'], notice['sha256'])
                root_notice_map.setdefault(key, []).append(group['id'])
    disk = shutil.disk_usage(BASE)
    resolution_ref = {'path': str(LICENSE_RESOLUTION), 'sha256': sha_file(LICENSE_RESOLUTION)}
    raw = {
        'schemaVersion': 1,
        'scope': 'read-only declared-license and LICENSE/NOTICE evidence for 46 unique lock-matched historical dependency installs supporting 76 candidate routes; two Bun routes lack installs and one Scope test file has a separate Electron/Playwright exclusion',
        'method': {
            'profile': EXPECTED_PROFILE,
            'mapPlanAndReceiptValidation': 'The frozen 78-route map, both preparation maps, original and supplement install plans, 78-candidate manifest, all 46 final receipts, two corrected bundled-runner receipts, and exact input hash maps were checked before traversing any package store.',
            'packageEnumeration': 'Read package.json and matching LICENSE, LICENCE, NOTICE, COPYING, PATENTS, and THIRD_PARTY_NOTICES files under each installed pnpm virtual store. No dependency package code was executed and no third-party file contents are included in this inventory.',
            'researchAndWorkspaceRule': 'Exclude .repos research workspaces and local workspace/file packages from third-party package rows. Do not copy installed dependencies; only record declared license strings, package JSON SHA-256, license/notice file names, byte counts, and file SHA-256.',
            'symlinkPolicy': 'Do not follow symlink directories. For package and project notice files, resolve the target and read only when it remains within the owning install source/store; record outside targets without reading them.',
            'licenseClassification': 'Retain exact package.json license values. The local parser classifies token syntax only and does not certify SPDX identifiers, license compatibility, or redistribution rights. Missing, unrecognized, absent-document, unreadable-document, and unresolved SEE LICENSE IN cases remain separate.',
            'scopeLimitations': 'A full lock-matched workspace install does not show that each candidate needs every installed package. T3 installs uniformly exclude t3code-relay. Two Bun 1.3.9 routes have no package installation. Scope candidate scope-84ec1ad4e6a8 is excluded from test execution because its selected journey test requires Electron/Playwright.',
        },
        'inputs': {
            'finalMap': str(FINAL_MAP), 'finalMapSha256': FINAL_MAP_SHA256,
            'finalCandidateManifest': str(FINAL_MANIFEST), 'finalCandidateManifestSha256': FINAL_MANIFEST_SHA256,
            'originalInstallPlan': str(ORIGINAL_PLAN), 'originalInstallPlanSha256': ORIGINAL_PLAN_SHA256,
            'supplementInstallPlan': str(SUPPLEMENT_PLAN), 'supplementInstallPlanSha256': SUPPLEMENT_PLAN_SHA256,
            'preservedFull22Inventory': str(PREVIOUS_INVENTORY), 'preservedFull22InventorySha256': PREVIOUS_INVENTORY_SHA256,
            'sourcePreparationMaps': proof['sourceMapBindings'],
            'correctedReceiptProof': proof['correctedReceiptProof'],
            'licenseResolutionReference': resolution_ref,
            'helperPath': str(pathlib.Path(__file__).resolve()), 'helperSha256': sha_file(__file__),
            'scanConfirmationMapSha256': expected_map_sha256,
        },
        'counts': {
            'successfulLockMatchedInstallations': len(group_rows),
            'candidateRoutes': proof['candidateRouteCount'], 'candidateRoutesWithInstallProfiles': proof['installedRouteCount'],
            'bunPreflightExclusions': len(proof['bunPreflightExclusions']),
            'testExecutionExclusions': len(proof['testExecutionExclusions']),
            'distinctThirdPartyPackageLicenseRecords': len(packages),
            'missingLicenseDeclarations': len(missing), 'unrecognizedLicenseDeclarations': len(unrecognized),
            'noLicenseNoticeFiles': len(without_docs), 'unreadableLicenseNoticeRecords': len(unreadable),
            'unresolvedSeeLicenseReferences': len(unresolved_see),
            'researchWorkspaceEntriesExcluded': sum(1 for values in excluded_workspace.values() for row in values if '.repos' in row.get('reason', '')),
            'firstPartyWorkspaceOrFilePackageInstancesExcluded': sum(len(v) for v in local_rows.values()),
            'packageLocatorResolutionFailures': len(locate_failures),
            'rootUpstreamNoticeFingerprints': len(root_notice_map),
        },
        'diskSnapshotAfterReadOnlyScan': {'freeBytes': disk.free, 'totalBytes': disk.total, 'freeFraction': disk.free / disk.total},
        'installGroups': group_rows,
        'candidateRoutes': {'count': proof['candidateRouteCount'], 'candidateRoutesWithInstallProfiles': proof['installedRouteCount'],
            'bunPreflightExclusions': proof['bunPreflightExclusions'], 'testExecutionExclusions': proof['testExecutionExclusions'],
            'candidateGroupBindings': proof['candidateGroupBindings']},
        'packages': packages,
        'missingLicenseDeclarations': [{'packageId': p['id'], 'name': p['name'], 'version': p['version'],
            'observedInGroups': p['observedInGroups'], 'declaredLicense': p['declaredLicense'], 'licenseNoticeFiles': p['licenseNoticeFiles']} for p in missing],
        'unrecognizedLicenseDeclarations': [{'packageId': p['id'], 'name': p['name'], 'version': p['version'],
            'observedInGroups': p['observedInGroups'], 'declaredLicense': p['declaredLicense'], 'licenseNoticeFiles': p['licenseNoticeFiles']} for p in unrecognized],
        'packagesWithoutLicenseNoticeFiles': [{'packageId': p['id'], 'name': p['name'], 'version': p['version'],
            'observedInGroups': p['observedInGroups'], 'declaredLicense': p['declaredLicense']} for p in without_docs],
        'unreadableLicenseNoticeFiles': [{'packageId': p['id'], 'name': p['name'], 'version': p['version'],
            'observedInGroups': p['observedInGroups'], 'licenseNoticeFiles': [x for x in p['licenseNoticeFiles'] if x.get('status') != 'hashed']} for p in unreadable],
        'unresolvedSeeLicenseReferences': [{'packageId': p['id'], 'name': p['name'], 'version': p['version'],
            'declaredLicense': p['declaredLicense'], 'referencedLicensePath': p.get('referencedLicensePath'),
            'observedInGroups': p['observedInGroups']} for p in unresolved_see],
        'effectAndEffectVitestEvidence': effect,
        'rootUpstreamNotices': [{'path': path, 'sha256': digest, 'observedInGroups': sorted(set(ids))}
            for (path, digest), ids in sorted(root_notice_map.items())],
        'excludedWorkspacePackagesByGroup': {k: v for k, v in sorted(excluded_workspace.items()) if v},
        'packageLocatorResolutionFailures': locate_failures,
        'retention': {'thirdPartyPackageTreesCopied': False, 'licenseNoticeContentsCopied': False,
            'onlyMetadataAndHashesRecorded': True, 'outputCompression': 'gzip with mtime=0'},
    }
    serialized = (json.dumps(raw, indent=2, ensure_ascii=False) + '\n').encode('utf-8')
    compressed = gzip.compress(serialized, compresslevel=9, mtime=0)
    gzip_path = atomic_exclusive_write(OUT_GZIP, compressed)
    compressed_sha = sha_file(gzip_path)
    uncompressed_hash = hashlib.sha256()
    uncompressed_size = 0
    with gzip.open(gzip_path, 'rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            uncompressed_hash.update(block)
            uncompressed_size += len(block)
    if compressed_sha != sha_bytes(compressed) or uncompressed_hash.hexdigest() != sha_bytes(serialized) or uncompressed_size != len(serialized):
        fail('compressed inventory failed its read-back digest check')
    receipt = {'schemaVersion': 1, 'artifactPath': str(gzip_path), 'artifactFormat': 'gzip JSON',
        'artifactBytes': len(compressed), 'artifactSha256': compressed_sha,
        'uncompressedBytes': uncompressed_size, 'uncompressedSha256': uncompressed_hash.hexdigest(),
        'helperPath': str(pathlib.Path(__file__).resolve()), 'helperSha256': sha_file(__file__),
        'finalMapPath': str(FINAL_MAP), 'finalMapSha256': FINAL_MAP_SHA256,
        'outputPathPolicy': 'exclusive create; existing targets are never overwritten'}
    receipt_bytes = (json.dumps(receipt, indent=2, ensure_ascii=False) + '\n').encode('utf-8')
    receipt_path = atomic_exclusive_write(OUT_RECEIPT, receipt_bytes)
    print(json.dumps({'status': 'inventory complete', 'artifact': str(gzip_path),
        'artifactSha256': receipt['artifactSha256'], 'uncompressedSha256': receipt['uncompressedSha256'],
        'receipt': str(receipt_path), 'counts': raw['counts']}, sort_keys=True))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--preflight-only', action='store_true', help='validate frozen metadata and receipts only; never traverse package stores')
    parser.add_argument('--final-scan-confirmed', action='store_true', help='perform the final read-only package/license inventory after reviewer and parent release')
    parser.add_argument('--expected-map-sha256', help='must equal the reviewed final map digest for a scan')
    args = parser.parse_args()
    try:
        if args.final_scan_confirmed:
            if args.preflight_only or not args.expected_map_sha256:
                fail('final scan requires only --final-scan-confirmed and the exact --expected-map-sha256')
            run_scan(args.expected_map_sha256)
        else:
            if args.expected_map_sha256:
                fail('expected-map digest is only accepted with --final-scan-confirmed')
            run_preflight()
    except AuditError as e:
        print(f'AUDIT_ERROR: {e}', file=sys.stderr)
        return 2
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
