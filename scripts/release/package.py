#!/usr/bin/env python3
"""Create a native distribution and retain the locked dependencies' actual notices."""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile

PLATFORMS = {
    'linux-x86_64': ('x86_64-unknown-linux-gnu', 'Ubuntu 22.04 / glibc 2.35'),
    'linux-arm64': ('aarch64-unknown-linux-gnu', 'Ubuntu 22.04 / glibc 2.35'),
    'macos-x86_64': ('x86_64-apple-darwin', 'macOS 15'),
    'macos-arm64': ('aarch64-apple-darwin', 'macOS 15'),
}
ROOT = Path(__file__).resolve().parents[2]


def metadata(platform):
    return json.loads(subprocess.check_output(
        ['cargo', '+1.96.0', 'metadata', '--locked', '--format-version', '1', '--filter-platform', PLATFORMS[platform][0]],
        cwd=ROOT, text=True))


def dependencies(data):
    nodes = {node['id']: node for node in data['resolve']['nodes']}
    pending, selected = [data['resolve']['root']], set()
    while pending:
        identifier = pending.pop()
        if identifier in selected:
            continue
        selected.add(identifier)
        pending.extend(dep['pkg'] for dep in nodes[identifier]['deps']
                       if any(kind['kind'] != 'dev' for kind in dep['dep_kinds']))
    return sorted((p for p in data['packages'] if p['id'] in selected), key=lambda p: (p['name'], p['version']))


def notices(packages, destination):
    records = []
    for package in packages:
        source = Path(package['manifest_path']).parent
        target = destination / f"{package['name']}-{package['version']}"
        # Registry packages retain notices below the root too, such as vendored Unicode data.
        candidates = [ROOT / 'LICENSE'] if source == ROOT else sorted(
            path for path in source.rglob('*') if path.is_file() and
            re.match(r'^(licen[cs]e|notices?|copying|copyright)([._-]|$)', path.name, re.I))
        if package.get('license_file'):
            candidates.append(source / package['license_file'])
        if not candidates:
            saved = json.loads((ROOT / 'docs/licenses/upstream/index.json').read_text())
            record = next((entry for entry in saved if entry['name'] == package['name'] and entry['version'] == package['version']), None)
            if record is None:
                raise RuntimeError(f"Missing actual license notices for {package['name']} {package['version']}")
            vcs = json.loads((source / '.cargo_vcs_info.json').read_text())['git']['sha1']
            if record['commit'] != vcs or record['repository'] != package.get('repository'):
                raise RuntimeError(f"Upstream notice provenance changed: {package['name']}")
            for notice in record['notices']:
                path = ROOT / 'docs/licenses/upstream' / notice['file']
                if hashlib.sha256(path.read_bytes()).hexdigest() != notice['sha256']:
                    raise RuntimeError(f"Upstream notice checksum changed: {notice['file']}")
                target.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, target / notice['upstreamPath'])
            records.append({**record, 'license': package.get('license')})
            continue
        files = []
        for path in sorted(set(candidates)):
            relative = path.relative_to(source)
            output = target / relative
            output.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, output)
            files.append(str(relative))
        records.append({'name': package['name'], 'version': package['version'],
                        'license': package.get('license'), 'repository': package.get('repository'), 'notices': files})
    (destination / 'inventory.json').write_text(json.dumps(records, indent=2) + '\n')


def package(binary, platform, output):
    data = metadata(platform)
    product = next(p for p in data['packages'] if p['id'] == data['resolve']['root'])
    version = product['version']
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    output.mkdir(parents=True, exist_ok=True)
    name = f'archguard-{version}-{platform}.tar.gz'
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory) / f'archguard-{version}'
        (root / 'bin').mkdir(parents=True)
        shutil.copyfile(binary, root / 'bin/archguard')
        (root / 'bin/archguard').chmod(0o755)
        for path in ['sdk', 'skills', 'docs', 'examples']:
            shutil.copytree(ROOT / path, root / path, ignore=shutil.ignore_patterns('node_modules', 'local'))
        provider = root / 'providers/typescript7'
        provider.mkdir(parents=True)
        for path in ['provider.mjs', 'package.json', 'package-lock.json', 'README.md']:
            shutil.copyfile(ROOT / 'providers/typescript7' / path, provider / path)
        for path in ['README.md', 'LICENSE', 'archguard.json']:
            shutil.copyfile(ROOT / path, root / path)
        (root / 'VERSION').write_text(version + '\n')
        (root / 'release.json').write_text(json.dumps({'version': version, 'commit': commit, 'platform': platform,
                                                      'minimumOS': PLATFORMS[platform][1]}, sort_keys=True) + '\n')
        notices(dependencies(data), root / 'licenses')
        toolchain = Path(subprocess.check_output(['rustc', '+1.96.0', '--print', 'sysroot'], text=True).strip())
        standard_library = root / 'licenses/rust-standard-library-1.96.0'
        standard_library.mkdir()
        shutil.copyfile(toolchain / 'share/doc/rust/COPYRIGHT-library.html', standard_library / 'COPYRIGHT-library.html')
        shutil.copytree(toolchain / 'share/doc/rust/licenses', standard_library / 'licenses')
        # Stable archive metadata lets interrupted uploads compare bytes on reruns.
        with (output / name).open('wb') as stream, gzip.GzipFile(fileobj=stream, mode='wb', filename='', mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w') as archive:
                for path in sorted(root.rglob('*')):
                    info = archive.gettarinfo(str(path), arcname=f'{root.name}/{path.relative_to(root)}')
                    info.uid = info.gid = info.mtime = 0
                    info.uname = info.gname = ''
                    info.mode = 0o755 if path.is_dir() or path == root / 'bin/archguard' else 0o644
                    archive.addfile(info, io.BytesIO(path.read_bytes()) if path.is_file() else None)
    digest = hashlib.sha256((output / name).read_bytes()).hexdigest()
    (output / f'{name}.sha256').write_text(f'{digest}  {name}\n')
    print(output / name)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--platform', required=True, choices=PLATFORMS)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    package(args.binary, args.platform, args.output)
