#!/usr/bin/env python3
"""Verify reduced semantic fixture excerpts against exact upstream sources."""
import hashlib,json,pathlib,subprocess,sys
root=pathlib.Path(__file__).resolve().parents[1]
evidence=json.loads((root/'docs/semantic-history.json').read_text())
for source in evidence['sources']:
    raw=subprocess.check_output(['git','-C',sys.argv[1],'show',source['commit']+':'+source['path']])
    assert hashlib.sha256(raw).hexdigest()==source['sha256'],source
    for excerpt in source['excerpts']:
        assert excerpt in raw.decode(),source
        for fixture in source['fixtures']:
            assert excerpt in (root/fixture).read_text(),fixture
for fixture,digest in evidence['fixtures'].items():
    assert hashlib.sha256((root/fixture).read_bytes()).hexdigest()==digest,fixture
parent=subprocess.check_output(['git','-C',sys.argv[1],'rev-parse',evidence['fixed']+'^'],text=True).strip()
assert parent==evidence['before']
print('Verified merged PR11304 sources, parent and reduced fixture excerpts')
