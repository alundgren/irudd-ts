#!/usr/bin/env python3
"""Check maintained Markdown links and byte-preserved research artifacts."""
import hashlib
import json
from pathlib import Path
import re
from urllib.parse import unquote

root = Path(__file__).resolve().parents[1]
paths = [root / 'README.md', root / 'AGENTS.md']
for directory in ['docs', 'examples', 'sdk', 'providers', 'research']:
    paths.extend(path for path in (root / directory).rglob('*.md')
                 if 'node_modules' not in path.parts and 'local' not in path.parts)
errors = []
for path in sorted(paths):
    content = path.read_text()
    for target in re.findall(r'\[[^\]]*\]\(([^\s)]+)(?:\s+"[^"]*")?\)', content):
        if re.match(r'[a-zA-Z][a-zA-Z0-9+.-]*:', target) or target.startswith('#'):
            continue
        target = unquote(target.split('#', 1)[0].split('?', 1)[0].strip('<>'))
        if target and not (path.parent / target).exists():
            errors.append(f'{path.relative_to(root)}: missing link {target}')
for artifact in json.loads((root / 'research/relocations.json').read_text())['artifacts']:
    path = root / artifact['current']
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != artifact['sha256']:
        errors.append(f'Preserved artifact changed: {artifact["current"]}')
if errors:
    raise SystemExit('\n'.join(errors))
print(f'{len(paths)} maintained Markdown files and preserved artifact hashes checked')
