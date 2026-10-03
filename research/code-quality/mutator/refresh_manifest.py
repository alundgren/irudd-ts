#!/usr/bin/env python3
"""Hash deliberately retained evidence without Python execution caches."""
import argparse
import hashlib
import subprocess
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--check-tracked', action='store_true', help='Reject evidence files absent from the staged Git index')
    args = parser.parse_args()
    evidence = Path(__file__).resolve().parent / 'evidence'
    files = [path for path in sorted(evidence.rglob('*')) if path.is_file()
        and '__pycache__' not in path.parts and path.suffix not in {'.pyc', '.pyo'}
        and path.name != 'manifest.sha256']
    if args.check_tracked:
        repository = Path(subprocess.check_output(['git', 'rev-parse', '--show-toplevel'], cwd=evidence, text=True).strip())
        paths = [path.relative_to(repository).as_posix() for path in files]
        subprocess.run(['git', 'ls-files', '--error-unmatch', '--', *paths], cwd=repository,
            check=True, stdout=subprocess.DEVNULL)
    manifest = ''.join(f'{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(evidence)}\n' for path in files)
    (evidence / 'manifest.sha256').write_text(manifest)
    print(f'Retained {len(files)} evidence files; Python caches excluded')


if __name__ == '__main__': main()
