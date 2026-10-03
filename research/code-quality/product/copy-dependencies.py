#!/usr/bin/env python3
"""Export a pinned pnpm package closure for the explicit research profiles."""
import argparse
import json
import os
from pathlib import Path
import shutil
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('checkout', type=Path)
parser.add_argument('destination', type=Path)
args = parser.parse_args()
checkout = args.checkout.resolve(strict=True)
store = (checkout / 'node_modules/.pnpm').resolve(strict=True)
destination = args.destination.absolute()
if destination.exists() or destination.is_symlink():
    parser.error('destination must not exist')
destination = destination.resolve(strict=False)
if destination == checkout or checkout in destination.parents:
    parser.error('destination must be outside the checkout')
destination.parent.mkdir(parents=True, exist_ok=True)
roots = ['vite-plus', 'effect', '@effect/vitest', 'react', 'react-test-renderer',
         'jose', 'yaml', '@noble/hashes', 'vitest']
search = [checkout / 'node_modules', checkout / 'apps/web/node_modules',
          checkout / 'packages/shared/node_modules', store / 'node_modules']
staging = Path(tempfile.mkdtemp(prefix='archguard-dependency-export-', dir=destination.parent))
pending = []
copied = set()

def retained_target(target):
    relative = target.resolve(strict=True).relative_to(store)
    if len(relative.parts) < 2 or relative.parts[0] == 'node_modules':
        raise ValueError('dependency does not resolve to a physical pnpm package: ' + str(target))
    pending.append(relative.parts[0])
    return staging / '.pnpm' / relative

try:
    for name in roots:
        candidate = next((base / name for base in search if (base / name).exists()), None)
        if candidate is None:
            raise FileNotFoundError('missing installed package: ' + name)
        link = staging / name
        target = retained_target(candidate)
        link.parent.mkdir(parents=True, exist_ok=True)
        link.symlink_to(os.path.relpath(target, link.parent), target_is_directory=True)
    while pending:
        name = pending.pop()
        if name in copied:
            continue
        copied.add(name)
        source = store / name
        output = staging / '.pnpm' / name
        shutil.copytree(source, output, symlinks=True)
        for parent, directories, files in os.walk(source, followlinks=False):
            for child in directories + files:
                path = Path(parent) / child
                if path.is_symlink():
                    target = retained_target(path)
                    link = output / path.relative_to(source)
                    link.unlink()
                    link.symlink_to(os.path.relpath(target, link.parent))
                elif not path.is_dir() and not path.is_file():
                    raise ValueError('non-regular dependency entry: ' + str(path))
    files = byte_count = 0
    for parent, directories, names in os.walk(staging, followlinks=False):
        for name in directories + names:
            path = Path(parent) / name
            if path.is_symlink():
                path.resolve(strict=True).relative_to(staging)
            elif path.is_file():
                files += 1
                byte_count += path.stat().st_size
    staging.rename(destination)
    print(json.dumps({'destination': str(destination), 'roots': roots,
                      'packageDirectories': len(copied), 'files': files, 'bytes': byte_count}))
except BaseException:
    shutil.rmtree(staging)
    raise
