#!/usr/bin/env python3
"""Audit the separately locked compiler provider or historical npm environment."""
import argparse
import hashlib
import json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--write', action='store_true')
parser.add_argument('--history', action='store_true', help='audit the T3 Effect reproduction')
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
installation = 'research/t3code/semantic' if args.history else 'providers/typescript7'
lock_path = root / installation / 'package-lock.json'
lock = json.loads(lock_path.read_text())
allowed = {'Apache-2.0', 'MIT', 'BSD-2-Clause', 'BSD-3-Clause', 'ISC', 'MIT AND BSD-3-Clause'}
records = []
for path, package in sorted(lock['packages'].items()):
    if not path:
        continue
    license = package.get('license')
    if license not in allowed:
        raise SystemExit(f'Unreviewed npm dependency license {path}: {license}')
    records.append({'package': path.removeprefix('node_modules/'), 'version': package['version'],
                    'license': license, 'integrity': package['integrity']})
notices = {}
notice_package = 'effect' if args.history else 'typescript'
notice_names = ['LICENSE'] if args.history else ['LICENSE', 'NOTICE.txt']
for name in notice_names:
    path = root / installation / 'node_modules' / notice_package / name
    if not path.exists():
        raise SystemExit(f'Install {installation} with npm ci --prefix {installation} --ignore-scripts first')
    notices[name] = hashlib.sha256(path.read_bytes()).hexdigest()
review = ('Effect MIT; historical declarations only. Retain the upstream license when redistributing.'
          if args.history else 'TypeScript Apache-2.0; bundled DefinitelyTyped MIT, Unicode permission notice, '
          'W3C specifications/CC-BY, WebGL MIT; native BSD/MIT/Apache dependencies and Go patent grants. '
          'Retain LICENSE and NOTICE.txt when redistributing upstream files. LGPL paragraph is a general '
          'notice, not a listed bundled LGPL component.')
report = {'source': f'{installation}/package-lock.json; locked npm manifests and bundled notices; no binary is vendored',
          'noticeReview': review, 'upstreamNotices': notices, 'dependencies': records}
output = json.dumps(report, indent=2) + '\n'
path = root / ('research/t3code/semantic/licenses.json' if args.history else 'docs/licenses/typescript7.json')
if args.write:
    path.write_text(output)
elif not path.exists() or path.read_text() != output:
    flag = ' --history' if args.history else ''
    raise SystemExit(f'Npm license inventory is stale. Run python3 scripts/semantic_licenses.py{flag} --write and review changes.')
print(f'{installation}: {len(records)} locked npm licenses and upstream notice hashes checked')
