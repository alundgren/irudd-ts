#!/usr/bin/env python3
"""Read-only license evidence inventory for completed historical package installs."""
import hashlib
import json
import os
import pathlib
import re
import sys
from collections import defaultdict

BASE = pathlib.Path('/tmp/archguard-test-value-20261003')
INSTALL_ROOT = BASE / 'historical-installs'
PREP_LOG = BASE / 'historical-installs-preparation.log'
PLAN = BASE / 'historical-install-plan.json'
OUT = BASE / 'historical-dependency-license-inventory-full-22-v2.json'

NAME_RE = re.compile(r'^(?:LICENSE|LICENCE|NOTICE|COPYING|PATENTS?|THIRD_PARTY_NOTICES)(?:$|[._ -])', re.I)
SKIP_DIRS = {'.git', 'node_modules', '.pnpm', '.yarn', '.cache'}

def sha(data):
    return hashlib.sha256(data).hexdigest()

def spdx_syntax(value):
    if not isinstance(value, str) or not value.strip():
        return 'unrecognized'
    s = ' '.join(value.strip().split())
    if s.upper() in {'UNLICENSED', 'NOASSERTION'}:
        return 'special-value'
    if s.upper().startswith('SEE LICENSE IN '):
        return 'see-license-reference'
    toks = re.findall(r'[A-Za-z0-9.+-]+|[()]', s)
    stripped = re.sub(r'[A-Za-z0-9.+-]+|[()]|\s+', '', s)
    if stripped:
        return 'unrecognized'
    depth = 0
    expect_id = True
    for t in toks:
        if expect_id:
            if t == '(':
                depth += 1
            elif t in {'AND', 'OR', 'WITH', ')'}:
                return 'unrecognized'
            else:
                expect_id = False
        else:
            if t in {'AND', 'OR', 'WITH'}:
                expect_id = True
            elif t == ')':
                depth -= 1
                if depth < 0:
                    return 'unrecognized'
                expect_id = False
            elif t == '(':
                return 'unrecognized'
            else:
                return 'unrecognized'
    if expect_id or depth != 0:
        return 'unrecognized'
    return 'token-syntax-only'

def license_paths(package_root, store_root):
    out = []
    pkg_real = package_root.resolve()
    store_real = store_root.resolve()
    for dirpath, dirs, files in os.walk(package_root, followlinks=False):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS and not (pathlib.Path(dirpath) / d).is_symlink()]
        for name in files:
            if not NAME_RE.match(name):
                continue
            p = pathlib.Path(dirpath) / name
            rel = p.relative_to(package_root).as_posix()
            row = {'path': rel, 'isSymlink': p.is_symlink()}
            try:
                resolved = p.resolve(strict=True)
                if not resolved.is_relative_to(store_real):
                    row.update(status='symlink-target-outside-install-root', resolvedTarget=str(resolved))
                else:
                    body = p.read_bytes()
                    row.update(status='hashed', sha256=sha(body), bytes=len(body))
            except OSError as e:
                row.update(status='unreadable', errorType=type(e).__name__)
            out.append(row)
    out.sort(key=lambda x: x['path'])
    return out

def package_name_from_locator(locator):
    if locator == 'node_modules':
        return None
    sep = locator.find('@', 1) if locator.startswith('@') else locator.find('@')
    if sep <= 0:
        return None
    # Pnpm peers follow the first name@version boundary; names themselves can contain underscores.
    return locator[:sep].replace('+', '/')

def root_notices(source):
    found = []
    for dirpath, dirs, files in os.walk(source, followlinks=False):
        rel_dir = pathlib.Path(dirpath).relative_to(source)
        if rel_dir.parts and ('.repos' in rel_dir.parts or 'node_modules' in rel_dir.parts or '.git' in rel_dir.parts):
            dirs[:] = []
            continue
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS and d != '.repos' and not (pathlib.Path(dirpath) / d).is_symlink()]
        for name in files:
            if not NAME_RE.match(name):
                continue
            p = pathlib.Path(dirpath) / name
            try:
                data = p.read_bytes()
                found.append({'path': p.relative_to(source).as_posix(), 'sha256': sha(data), 'bytes': len(data)})
            except OSError as e:
                found.append({'path': p.relative_to(source).as_posix(), 'status': 'unreadable', 'errorType': type(e).__name__})
    found.sort(key=lambda x: x['path'])
    return found

def main():
    plan = json.loads(PLAN.read_text())
    # The frozen preparation log is the independent record of successful installs.
    completed = []
    for line in PREP_LOG.read_text(errors='replace').splitlines():
        parts = line.split()
        if len(parts) >= 4 and parts[1] == 'complete' and parts[2] == '0':
            completed.append(parts[0])
    completed = sorted(set(completed))
    if len(completed) != 22:
        raise RuntimeError(f'Expected the recorded 22 completed full installs, found {len(completed)}')
    groups = {g['id']: g for g in plan['groups']}
    package_records = {}
    group_rows = []
    excluded_workspace = defaultdict(list)
    locate_failures = []

    for gid in completed:
        root = INSTALL_ROOT / gid
        source = root / 'source'
        receipt_path = root / 'receipt.json'
        receipt = json.loads(receipt_path.read_text())
        group = groups[gid]
        if (receipt.get('id') != gid or receipt.get('subject') != group['subject'] or
                receipt.get('revision') != group['revision']):
            raise RuntimeError(f'receipt identity mismatch for {gid}')
        expected_inputs = {r['path']: r['sha256'] for r in group['inputFiles']}
        if receipt.get('inputFiles') != expected_inputs:
            raise RuntimeError(f'install receipt input map mismatch for {gid}')
        if not source.is_dir():
            raise RuntimeError(f'missing source root for {gid}')
        store = source / 'node_modules' / '.pnpm'
        if not store.is_dir():
            raise RuntimeError(f'missing pnpm virtual store for {gid}')
        install_instances = []
        local_source_packages = {}
        for item in group['inputFiles']:
            path = item['path']
            if not path.endswith('/package.json') and path != 'package.json':
                continue
            if '.repos/' in path:
                continue
            candidate_manifest = source / path
            try:
                m = json.loads(candidate_manifest.read_text())
            except Exception:
                continue
            if isinstance(m, dict) and isinstance(m.get('name'), str) and isinstance(m.get('version'), str):
                local_source_packages[(m['name'], m['version'])] = path
        local_project_instances = []
        for locator_dir in store.iterdir():
            if not locator_dir.is_dir() or locator_dir.name == 'node_modules':
                continue
            pkg_name = package_name_from_locator(locator_dir.name)
            if not pkg_name:
                locate_failures.append({'groupId': gid, 'virtualStoreEntry': locator_dir.name, 'reason': 'not a package locator'})
                continue
            pkg_dir = locator_dir / 'node_modules' / pkg_name
            package_json = pkg_dir / 'package.json'
            if not package_json.is_file():
                locate_failures.append({'groupId': gid, 'virtualStoreEntry': locator_dir.name, 'packageNameGuess': pkg_name, 'reason': 'package.json not found at decoded locator path'})
                continue
            try:
                resolved_package = pkg_dir.resolve(strict=True)
            except OSError:
                locate_failures.append({'groupId': gid, 'virtualStoreEntry': locator_dir.name, 'packageNameGuess': pkg_name, 'reason': 'package path cannot resolve'})
                continue
            if '.repos' in resolved_package.parts:
                excluded_workspace[gid].append({'virtualStoreEntry': locator_dir.name, 'packageName': pkg_name, 'reason': 'local research workspace package under .repos; excluded from third-party inventory and never copied'})
                continue
            if not resolved_package.is_relative_to(source.resolve()):
                locate_failures.append({'groupId': gid, 'virtualStoreEntry': locator_dir.name, 'packageNameGuess': pkg_name, 'reason': 'package resolves outside archived install source'})
                continue
            if not resolved_package.is_relative_to(store.resolve()):
                # Workspace packages are present through links, not third-party package contents.
                excluded_workspace[gid].append({'virtualStoreEntry': locator_dir.name, 'packageName': pkg_name, 'reason': 'local workspace package outside virtual store; excluded from third-party inventory'})
                continue
            try:
                package_json_bytes = package_json.read_bytes()
                manifest = json.loads(package_json_bytes)
            except Exception as e:
                locate_failures.append({'groupId': gid, 'virtualStoreEntry': locator_dir.name, 'packageNameGuess': pkg_name, 'reason': 'package.json unreadable or invalid', 'errorType': type(e).__name__})
                continue
            if not isinstance(manifest, dict) or not isinstance(manifest.get('name'), str) or not isinstance(manifest.get('version'), str):
                locate_failures.append({'groupId': gid, 'virtualStoreEntry': locator_dir.name, 'packageNameGuess': pkg_name, 'reason': 'package manifest lacks name/version'})
                continue
            name = manifest['name']; version = manifest['version']; declared = manifest.get('license')
            if (name, version) in local_source_packages:
                local_project_instances.append({'virtualStoreEntry': locator_dir.name, 'packageName': name, 'version': version, 'sourcePackagePath': local_source_packages[(name, version)], 'reason': 'first-party workspace/file dependency; excluded from third-party package inventory and never copied'})
                continue
            docs = license_paths(resolved_package, store)
            pj_sha = sha(package_json_bytes)
            fingerprint_payload = {'name': name, 'version': version, 'packageJsonSha256': pj_sha,
                                   'declaredLicense': declared, 'licenseFiles': docs}
            fingerprint = sha(json.dumps(fingerprint_payload, sort_keys=True, separators=(',', ':')).encode())
            key = fingerprint
            rec = package_records.get(key)
            if rec is None:
                status = spdx_syntax(declared)
                rec = {'id': name + '@' + version + '#' + fingerprint[:16], 'fingerprintSha256': fingerprint,
                       'name': name, 'version': version, 'declaredLicense': declared,
                       'declaredLicenseStatus': status, 'packageJsonSha256': pj_sha,
                       'licenseNoticeFiles': docs, 'observedInGroups': [], 'virtualStoreEntries': [],
                       'patchedVirtualStoreEntryObserved': False}
                package_records[key] = rec
            if gid not in rec['observedInGroups']:
                rec['observedInGroups'].append(gid)
            loc = {'groupId': gid, 'virtualStoreEntry': locator_dir.name, 'packagePath': pkg_dir.relative_to(source).as_posix()}
            if loc not in rec['virtualStoreEntries']:
                rec['virtualStoreEntries'].append(loc)
            if 'patch_hash=' in locator_dir.name:
                rec['patchedVirtualStoreEntryObserved'] = True
            install_instances.append(key)
        notices = root_notices(source)
        group_rows.append({'id': gid, 'subject': group['subject'], 'revision': group['revision'],
                           'installationRoot': str(source), 'receiptPath': str(receipt_path),
                           'receiptSha256': sha(receipt_path.read_bytes()), 'completedInstallEvidence': 'historical-installs-preparation.log line: complete 0',
                           'virtualStoreEntryCount': len([x for x in store.iterdir() if x.is_dir() and x.name != 'node_modules']),
                           'thirdPartyPackageInstancesInventoried': len(install_instances),
                           'distinctPackageEvidenceRecords': len(set(install_instances)),
                           'researchOrWorkspacePackagesExcluded': len(excluded_workspace[gid]),
                           'firstPartyWorkspaceOrFilePackagesExcluded': len(local_project_instances),
                           'rootUpstreamNoticeFiles': notices})
        excluded_workspace[gid].extend({'virtualStoreEntry':x['virtualStoreEntry'],'packageName':x['packageName'],'reason':x['reason'],'sourcePackagePath':x['sourcePackagePath']} for x in local_project_instances)

    packages = sorted(package_records.values(), key=lambda r: (r['name'], r['version'], r['fingerprintSha256']))
    for p in packages:
        p['observedInGroups'].sort(); p['virtualStoreEntries'].sort(key=lambda x:(x['groupId'],x['virtualStoreEntry']))
    missing_declared = [p for p in packages if p['declaredLicenseStatus'] == 'unrecognized' and p['declaredLicense'] is None]
    unrecognized = [p for p in packages if p['declaredLicenseStatus'] == 'unrecognized' and p['declaredLicense'] is not None]
    no_license_docs = [p for p in packages if not p['licenseNoticeFiles']]
    unreadable_docs = [p for p in packages if any(x.get('status') != 'hashed' for x in p['licenseNoticeFiles'])]
    see_missing = [p for p in packages if p['declaredLicenseStatus'] == 'see-license-reference' and not p['licenseNoticeFiles']]
    effect = [p for p in packages if p['name'] in {'effect', '@effect/vitest'}]
    root_notice_map = {}
    for g in group_rows:
        for n in g['rootUpstreamNoticeFiles']:
            if n.get('sha256'):
                k=(n['path'],n['sha256'])
                root_notice_map.setdefault(k,[]).append(g['id'])
    try:
        import shutil
        disk = shutil.disk_usage(BASE)
        diskdata={'freeBytes':disk.free,'totalBytes':disk.total,'freeFraction':disk.free/disk.total}
    except Exception:
        diskdata=None
    out={'schemaVersion':1,'scope':'read-only declared-license and LICENSE/NOTICE evidence for 22 successful full historical installs',
         'supersedes':'historical-dependency-license-inventory-full-22.json; v2 uses corrected pnpm locator parsing for peer suffixes and excludes first-party workspace/file packages.',
         'method':{'installSelection':'IDs marked complete with exit code 0 in the retained historical-installs-preparation.log; exact IDs, receipts, subjects, revisions, and frozen input maps cross-checked against historical-install-plan.json.',
                   'packageEnumeration':'Read each successful install source/node_modules/.pnpm virtual store; decode package locator names, then read package.json and license/notice files from the installed package contents. No package code or lifecycle scripts were executed.',
                   'researchRule':'Do not include or copy .repos research workspace package trees. Local workspace links are excluded from third-party package rows. All external packages found in the full stores are evidence only; no dependency content is copied by this audit.',
                   'licenseClassification':'Retain exact package.json license value and file hashes. The local parser classifies syntax only and does not certify SPDX identifier validity or grant redistribution rights; missing declarations, unrecognized declarations, missing license documents, and unreadable notice files are separate reports.',
                   'rootNotices':'Hash top-level project LICENSE/NOTICE/COPYING and first-party THIRD_PARTY_NOTICES files under each archived source root; omit .repos and installed node_modules.',
                   'limitations':'Presence in a full install does not establish that a candidate needs that package. External packages used solely by research workspaces are not identified by dependency reachability here; no package trees are copied, and research workspace source is excluded.'},
         'inputs':{'installPreparationLog':str(PREP_LOG),'installPreparationLogSha256':sha(PREP_LOG.read_bytes()),'historicalInstallPlan':str(PLAN),'historicalInstallPlanSha256':sha(PLAN.read_bytes()),'installRoot':str(INSTALL_ROOT),'installCount':len(group_rows)},
         'counts':{'successfulFullInstalls':len(group_rows),'distinctThirdPartyPackageLicenseRecords':len(packages),'missingLicenseDeclarations':len(missing_declared),'unrecognizedLicenseDeclarations':len(unrecognized),'noLicenseNoticeFiles':len(no_license_docs),'unreadableLicenseNoticeRecords':len(unreadable_docs),'unresolvedSeeLicenseReferences':len(see_missing),'researchWorkspaceEntriesExcluded':sum(1 for v in excluded_workspace.values() for x in v if '.repos' in x.get('reason','')),'firstPartyWorkspaceOrFilePackageInstancesExcluded':sum(1 for v in excluded_workspace.values() for x in v if 'first-party workspace/file' in x.get('reason','')),'packageLocatorResolutionFailures':len(locate_failures),'rootUpstreamNoticeFingerprints':len(root_notice_map)},
         'diskSnapshotAfterReadOnlyScan':diskdata,'fullInstallGroups':group_rows,'packages':packages,
         'missingLicenseDeclarations':[{'packageId':p['id'],'name':p['name'],'version':p['version'],'observedInGroups':p['observedInGroups'],'declaredLicense':p['declaredLicense'],'licenseNoticeFiles':p['licenseNoticeFiles']} for p in missing_declared],
         'unrecognizedLicenseDeclarations':[{'packageId':p['id'],'name':p['name'],'version':p['version'],'observedInGroups':p['observedInGroups'],'declaredLicense':p['declaredLicense'],'licenseNoticeFiles':p['licenseNoticeFiles']} for p in unrecognized],
         'packagesWithoutLicenseNoticeFiles':[{'packageId':p['id'],'name':p['name'],'version':p['version'],'observedInGroups':p['observedInGroups'],'declaredLicense':p['declaredLicense']} for p in no_license_docs],
         'unreadableLicenseNoticeFiles':[{'packageId':p['id'],'name':p['name'],'version':p['version'],'observedInGroups':p['observedInGroups'],'licenseNoticeFiles':[x for x in p['licenseNoticeFiles'] if x.get('status')!='hashed']} for p in unreadable_docs],
         'unresolvedSeeLicenseReferences':[{'packageId':p['id'],'name':p['name'],'version':p['version'],'declaredLicense':p['declaredLicense'],'observedInGroups':p['observedInGroups']} for p in see_missing],
         'effectAndEffectVitestEvidence':effect,
         'rootUpstreamNotices':[{'path':p,'sha256':h,'observedInGroups':sorted(set(ids))} for (p,h),ids in sorted(root_notice_map.items())],
         'excludedWorkspacePackagesByGroup':{k:v for k,v in sorted(excluded_workspace.items()) if v},
         'packageLocatorResolutionFailures':locate_failures,
         'filteredStoreInventory':{'status':'no completed filtered install root recorded during this scan','searchedBase':str(BASE),'potentialProbeDirectoriesExcluded':['pnpm10-filtered-positive-probe','pnpm10-filtered-positive-probe-2']}}
    OUT.write_text(json.dumps(out,indent=2)+'\n')
    print('WROTE',OUT,'bytes',OUT.stat().st_size,'sha256',sha(OUT.read_bytes()))
    print('COUNTS',json.dumps(out['counts'],sort_keys=True))
    print('EFFECT')
    for p in effect:
        print(p['name'],p['version'],repr(p['declaredLicense']),p['declaredLicenseStatus'],[(x.get('path'),x.get('sha256')) for x in p['licenseNoticeFiles']])
    print('MISSING',[(p['name'],p['version'],p['observedInGroups'][:2]) for p in missing_declared[:30]])
    print('UNRECOGNIZED',[(p['name'],p['version'],repr(p['declaredLicense']),p['observedInGroups'][:2]) for p in unrecognized[:30]])
    print('NO_DOCS',[(p['name'],p['version'],repr(p['declaredLicense'])) for p in no_license_docs[:30]])

if __name__=='__main__':main()
