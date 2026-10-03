#!/usr/bin/env python3
"""Check the pinned provider's npm inventory and retained upstream notice hashes."""
import argparse,hashlib,json,pathlib
parser=argparse.ArgumentParser();parser.add_argument('--write',action='store_true');args=parser.parse_args()
root=pathlib.Path(__file__).resolve().parents[1];provider=root/'providers/typescript7';lock=json.loads((provider/'package-lock.json').read_text())
allowed={'Apache-2.0','MIT','BSD-2-Clause','BSD-3-Clause','ISC','MIT AND BSD-3-Clause'}
records=[]
for path,package in sorted(lock['packages'].items()):
    if not path:continue
    license=package.get('license')
    if license not in allowed:raise SystemExit(f'Unreviewed npm dependency license {path}: {license}')
    records.append({'package':path.removeprefix('node_modules/'),'version':package['version'],'license':license,'integrity':package['integrity']})
notices={}
for name in ['LICENSE','NOTICE.txt']:
    path=provider/'node_modules/typescript'/name
    if not path.exists():raise SystemExit('Install pinned provider dependencies with npm ci --ignore-scripts first')
    notices[name]=hashlib.sha256(path.read_bytes()).hexdigest()
report={'source':'providers/typescript7/package-lock.json; platform npm optional package manifests and bundled notices; no binary is vendored','noticeReview':'TypeScript Apache-2.0; bundled DefinitelyTyped MIT, Unicode permission notice, W3C specifications/CC-BY, WebGL MIT; native BSD/MIT/Apache dependencies and Go patent grants. Retain LICENSE and NOTICE.txt when redistributing upstream files. LGPL paragraph is a general notice, not a listed bundled LGPL component.','typescriptNotices':notices,'dependencies':records}
output=json.dumps(report,indent=2)+'\n';path=root/'docs/semantic-dependency-licenses.json'
if args.write:path.write_text(output)
elif not path.exists() or path.read_text()!=output:raise SystemExit('Semantic dependency license inventory is stale')
print(f'{len(records)} pinned semantic npm licenses and upstream notice hashes checked')
