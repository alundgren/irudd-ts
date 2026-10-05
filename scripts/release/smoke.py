#!/usr/bin/env python3
"""Run the shipped executable against all three public exit outcomes."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile


def smoke(binary):
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        (root / 'source.ts').write_text('export const answer = 42;\n')
        config = {'schemaVersion': 1, 'include': ['*.ts'], 'rules': []}
        for expected in [0, 1, 2]:
            config['rules'] = [] if expected == 0 else [
                {'id': 'exports', 'kind': 'requiredExport', 'files': ['source.ts'], 'names': ['absent']}]
            if expected == 2:
                (root / 'source.ts').write_text('export const = ;\n')
            (root / 'archguard.json').write_text(json.dumps(config))
            result = subprocess.run([str(Path(binary).resolve()), 'check', '--root', directory,
                                     '--config', str(root / 'archguard.json')], capture_output=True, text=True)
            if result.returncode != expected:
                raise RuntimeError(f'Expected exit {expected}, got {result.returncode}: {result.stdout}{result.stderr}')


if __name__ == '__main__':
    smoke(sys.argv[1])
    print('Native executable smoke checks passed for exits 0, 1, and 2')
