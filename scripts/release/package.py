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
import zipfile

PLATFORMS = {
    'linux-x86_64': ('x86_64-unknown-linux-gnu', 'Ubuntu 22.04 / glibc 2.35'),
    'linux-arm64': ('aarch64-unknown-linux-gnu', 'Ubuntu 22.04 / glibc 2.35'),
    'macos-x86_64': ('x86_64-apple-darwin', 'macOS 15'),
    'macos-arm64': ('aarch64-apple-darwin', 'macOS 15'),
    'windows-x86_64': ('x86_64-pc-windows-msvc', 'Windows 10 1809 / Windows Server 2019'),
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


def windows_runtime_notices(source, binary, destination):
    if source is None:
        raise RuntimeError('Windows release requires collected static runtime notices')
    record = json.loads((source / 'provenance.json').read_text())
    if (record.get('schemaVersion') != 1 or record.get('target') != PLATFORMS['windows-x86_64'][0]
            or record.get('runtime') != 'static-release'
            or record.get('binarySha256') != hashlib.sha256(binary.read_bytes()).hexdigest()):
        raise RuntimeError('Windows runtime provenance does not match the release binary')
    if (record.get('visualStudio', {}).get('productId') not in ('Microsoft.VisualStudio.Product.Enterprise', 'Microsoft.VisualStudio.Product.Professional')
            or not re.fullmatch(r'17\.\d+\.\d+\.\d+', record.get('visualStudio', {}).get('version', ''))
            or not re.fullmatch(r'14\.\d+\.\d+', record.get('toolsetVersion', ''))
            or not re.fullmatch(r'10\.0\.\d+\.\d+', record.get('windowsSdkVersion', ''))):
        raise RuntimeError('Windows runtime toolchain identity is unsupported')
    libraries = record.get('libraries', [])
    if (len(libraries) != 3 or {entry.get('name') for entry in libraries} != {'libvcruntime.lib', 'libcmt.lib', 'libucrt.lib'}
            or any(not re.fullmatch(r'[0-9a-f]{64}', entry.get('sha256', '')) or not entry.get('sourcePath') for entry in libraries)):
        raise RuntimeError('Windows release runtime library provenance is incomplete')
    if not record.get('linker', {}).get('sourcePath') or not re.fullmatch(r'[0-9a-f]{64}', record.get('linker', {}).get('sha256', '')):
        raise RuntimeError('Windows release linker provenance is incomplete')
    notices = record.get('notices', [])
    if not notices or {entry.get('kind') for entry in notices} != {'visual-studio', 'windows-sdk'}:
        raise RuntimeError('Windows release is missing applicable runtime terms')
    expected = {'provenance.json'}
    for notice in notices:
        relative = notice.get('file', '')
        if (not relative or '\\' in relative or ':' in relative or relative.startswith('/')
                or any(part in ('', '.', '..') for part in relative.split('/'))
                or Path(relative).suffix.lower() not in ('.docx', '.html', '.rtf', '.txt')
                or relative in expected or not notice.get('source')):
            raise RuntimeError('Windows runtime notice path or provenance is invalid')
        path = source / relative
        if path.is_symlink() or not path.is_file() or path.stat().st_size > 4 * 1024 * 1024 or hashlib.sha256(path.read_bytes()).hexdigest() != notice.get('sha256'):
            raise RuntimeError('Windows runtime notice is missing or its checksum changed')
        expected.add(relative)
    actual = {path.relative_to(source).as_posix() for path in source.rglob('*') if path.is_file()}
    if actual != expected or any(path.is_symlink() for path in source.rglob('*')):
        raise RuntimeError('Windows runtime notices contain unexpected files')
    shutil.copytree(source, destination)


def package(binary, platform, output, runtime_notices=None):
    if platform.startswith('windows-') and runtime_notices is None:
        raise RuntimeError('Windows release requires collected static runtime notices')
    if not platform.startswith('windows-') and runtime_notices is not None:
        raise RuntimeError('Microsoft runtime notices require a Windows release')
    data = metadata(platform)
    product = next(p for p in data['packages'] if p['id'] == data['resolve']['root'])
    version = product['version']
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    output.mkdir(parents=True, exist_ok=True)
    extension = 'zip' if platform.startswith('windows-') else 'tar.gz'
    executable = 'archguard.exe' if platform.startswith('windows-') else 'archguard'
    name = f'archguard-{version}-{platform}.{extension}'
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory) / f'archguard-{version}'
        (root / 'bin').mkdir(parents=True)
        shutil.copyfile(binary, root / 'bin' / executable)
        (root / 'bin' / executable).chmod(0o755)
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
        if platform.startswith('windows-'):
            windows_runtime_notices(runtime_notices, binary, root / 'licenses/microsoft-static-runtime')
        toolchain = Path(subprocess.check_output(['rustc', '+1.96.0', '--print', 'sysroot'], text=True).strip())
        standard_library = root / 'licenses/rust-standard-library-1.96.0'
        standard_library.mkdir()
        shutil.copyfile(toolchain / 'share/doc/rust/COPYRIGHT-library.html', standard_library / 'COPYRIGHT-library.html')
        shutil.copytree(toolchain / 'share/doc/rust/licenses', standard_library / 'licenses')
        # Stable archive metadata lets interrupted uploads compare bytes on reruns.
        if extension == 'zip':
            with zipfile.ZipFile(output / name, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
                for path in sorted(root.rglob('*')):
                    if path.is_file():
                        entry = zipfile.ZipInfo(f'{root.name}/{path.relative_to(root).as_posix()}', (1980, 1, 1, 0, 0, 0))
                        entry.create_system = 3
                        entry.external_attr = (0o100644 << 16)
                        entry.compress_type = zipfile.ZIP_DEFLATED
                        archive.writestr(entry, path.read_bytes())
        else:
            with (output / name).open('wb') as stream, gzip.GzipFile(fileobj=stream, mode='wb', filename='', mtime=0) as compressed:
                with tarfile.open(fileobj=compressed, mode='w') as archive:
                    for path in sorted(root.rglob('*')):
                        info = archive.gettarinfo(str(path), arcname=f'{root.name}/{path.relative_to(root).as_posix()}')
                        info.uid = info.gid = info.mtime = 0
                        info.uname = info.gname = ''
                        info.mode = 0o755 if path.is_dir() or path == root / 'bin' / executable else 0o644
                        archive.addfile(info, io.BytesIO(path.read_bytes()) if path.is_file() else None)
    digest = hashlib.sha256((output / name).read_bytes()).hexdigest()
    (output / f'{name}.sha256').write_text(f'{digest}  {name}\n')
    print(output / name)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--platform', required=True, choices=PLATFORMS)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--windows-runtime-notices', type=Path)
    args = parser.parse_args()
    package(args.binary, args.platform, args.output, args.windows_runtime_notices)
