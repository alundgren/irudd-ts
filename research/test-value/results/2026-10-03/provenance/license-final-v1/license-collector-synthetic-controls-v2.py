#!/usr/bin/env python3
"""Retained synthetic controls for the read-only package license collector."""
import hashlib
import importlib.util
import json
import os
import pathlib
import shutil
import tempfile
from collections import defaultdict

BASE = pathlib.Path('/tmp/archguard-test-value-20261003')
HELPER = BASE / 'collect_final_dependency_licenses_v1.py'
EXPECTED_HELPER_SHA256 = 'c1cbd9ba1ce29c46fce9b053958740265620b7cf32f16e1d78cc7fe8d6e84a56'
OUTPUT = BASE / 'license-collector-synthetic-controls-v2.json'

def sha(data):
    return hashlib.sha256(data).hexdigest()

def rejected(call):
    try:
        call()
    except Exception as exc:
        return type(exc).__name__ == 'AuditError'
    return False

def main():
    if sha(HELPER.read_bytes()) != EXPECTED_HELPER_SHA256:
        raise SystemExit('helper digest changed; review and rebind this control before running')
    spec = importlib.util.spec_from_file_location('license_collector_synthetic', HELPER)
    collector = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(collector)
    fixture = pathlib.Path(tempfile.mkdtemp(prefix='license-collector-synthetic-', dir=BASE))
    external = BASE / (fixture.name + '-outside-marker')
    external.write_text('Synthetic target. Not a license or third-party notice.\n')
    original_read = pathlib.Path.read_bytes
    external_real = external.resolve(strict=True)
    def guarded_read(path):
        try:
            if path.resolve(strict=False) == external_real:
                raise AssertionError('collector attempted to read outside symlink target')
        except OSError:
            pass
        return original_read(path)
    pathlib.Path.read_bytes = guarded_read
    try:
        source_manifest = fixture / 'package.json'
        source_manifest.write_text('{"name":"synthetic-workspace","version":"0.0.0"}\n')
        store = fixture / 'node_modules' / '.pnpm'
        mit = store / 'synthetic-license-check@1.0.0' / 'node_modules' / 'synthetic-license-check'
        see = store / 'synthetic-see-license@2.0.0' / 'node_modules' / 'synthetic-see-license'
        mit.mkdir(parents=True)
        see.mkdir(parents=True)
        (mit / 'package.json').write_text('{"name":"synthetic-license-check","version":"1.0.0","license":"MIT"}\n')
        (mit / 'LICENSE.txt').write_text('Synthetic license marker for scanner control only.\n')
        (mit / 'NOTICE').symlink_to(external)
        (mit / 'COPYING').symlink_to(mit / 'does-not-exist')
        (see / 'package.json').write_text('{"name":"synthetic-see-license","version":"2.0.0","license":"SEE LICENSE IN COPYING"}\n')
        (see / 'NOTICE').write_text('A notice does not satisfy a reference to COPYING.\n')
        (fixture / 'LICENSE').symlink_to(external)
        expected = {'package.json': sha(source_manifest.read_bytes())}
        row = {'id':'synthetic-install-check','subject':'scope','revision':'synthetic',
               'root':str(fixture),'receiptPath':'synthetic-only','receiptSha256':'0' * 64}
        group = {'data':{'id':'synthetic-install-check'},'inputs':expected}
        packages, excluded, failures = {}, defaultdict(list), []
        result, local = collector.scan_installation(row, group, packages, excluded, failures)
        by_name = {p['name']:p for p in packages.values()}
        mit_doc_status = {x['path']:x['status'] for x in by_name['synthetic-license-check']['licenseNoticeFiles']}
        assert result['thirdPartyPackageInstancesInventoried'] == 2
        assert mit_doc_status['LICENSE.txt'] == 'hashed'
        assert mit_doc_status['NOTICE'] == 'symlink-target-outside-install-root'
        assert mit_doc_status['COPYING'] == 'unreadable'
        assert collector.see_license_reference_status(by_name['synthetic-see-license']) == ('COPYING', False)
        root_license = next(x for x in result['rootUpstreamNoticeFiles'] if x['path'] == 'LICENSE')
        assert root_license['status'] == 'symlink-target-outside-source'
        assert not failures and not local
        existing = fixture / 'already-present.json'
        existing.write_text('retain this file')
        link = fixture / 'existing-link.json'
        link.symlink_to(external)
        assert rejected(lambda: collector.safe_output_path(existing))
        assert rejected(lambda: collector.safe_output_path(link))
        rows = {
            'helperSha256': EXPECTED_HELPER_SHA256,
            'syntheticStorePackagesInventoried': result['thirdPartyPackageInstancesInventoried'],
            'resolvedNoticeFileHashed': True,
            'externalPackageSymlinkNotRead': True,
            'danglingPackageSymlinkMarkedUnreadable': True,
            'externalProjectNoticeSymlinkNotRead': True,
            'SEE_LICENSE_IN_requires_named_file': True,
            'existingOutputFileRejected': True,
            'existingOutputSymlinkRejected': True,
            'packageCodeExecuted': False,
            'historicalStoresTraversed': False,
        }
    finally:
        pathlib.Path.read_bytes = original_read
        shutil.rmtree(fixture)
        external.unlink(missing_ok=True)
    encoded = (json.dumps({'schemaVersion':1,'controls':rows},indent=2) + '\n').encode()
    fd = os.open(OUTPUT, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(encoded)
        stream.flush()
        os.fsync(stream.fileno())
    print(json.dumps({'resultPath':str(OUTPUT),'resultSha256':sha(encoded),'controls':rows},sort_keys=True))

if __name__ == '__main__':
    main()
