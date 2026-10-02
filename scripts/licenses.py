#!/usr/bin/env python3
"""Audit the locked Cargo dependency manifests and keep a reproducible inventory."""
import argparse, json, pathlib, re, subprocess
parser = argparse.ArgumentParser()
parser.add_argument('--write', action='store_true')
args = parser.parse_args()
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1'], text=True))
allowed = {'MIT', 'Apache-2.0', 'BSD-2-Clause', 'BSD-3-Clause', 'ISC', 'Unicode-3.0', 'Zlib', 'BSL-1.0', 'CC0-1.0', 'Unlicense'}
def select(expression):
    tokens = re.findall(r'\(|\)|[A-Za-z0-9][A-Za-z0-9.+-]*', expression.replace('/', ' OR '))
    index = 0
    def atom():
        nonlocal index
        if index >= len(tokens):
            raise ValueError('missing license')
        token = tokens[index]; index += 1
        if token == '(':
            result = disjunction()
            if index >= len(tokens) or tokens[index] != ')':
                raise ValueError('unclosed group')
            index += 1
            return result
        if token in {'AND', 'OR', 'WITH', ')'}:
            raise ValueError('invalid license token')
        result = token if token in allowed else None
        if index < len(tokens) and tokens[index] == 'WITH':
            index += 1
            if index >= len(tokens):
                raise ValueError('missing exception')
            exception = tokens[index]; index += 1
            result = f'{token} WITH {exception}' if token == 'Apache-2.0' and exception == 'LLVM-exception' else None
        return result
    def conjunction():
        nonlocal index
        result = atom()
        while index < len(tokens) and tokens[index] == 'AND':
            index += 1; other = atom()
            result = f'{result} AND {other}' if result and other else None
        return result
    def disjunction():
        nonlocal index
        result = conjunction()
        while index < len(tokens) and tokens[index] == 'OR':
            index += 1; other = conjunction()
            result = result or other
        return result
    result = disjunction()
    if index != len(tokens):
        raise ValueError('unexpected license token')
    return result
records = []
for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
    expression = package.get('license') or ''
    try:
        selected = select(expression)
    except ValueError:
        selected = None
    if not selected:
        raise SystemExit(f"License needs review: {package['name']} {package['version']} {expression!r}")
    records.append({'name': package['name'], 'version': package['version'], 'license': expression, 'selectedLicense': selected, 'repository': package.get('repository')})
output = json.dumps({'source': 'locked Cargo package manifests; inspect upstream notices before binary redistribution', 'dependencies': records}, indent=2) + '\n'
path = pathlib.Path('docs/dependency-licenses.json')
if args.write:
    path.write_text(output)
elif not path.exists() or path.read_text() != output:
    raise SystemExit('Dependency license inventory is stale. Run python3 scripts/licenses.py --write and review changes.')
print(f'{len(records)} locked package licenses checked')
