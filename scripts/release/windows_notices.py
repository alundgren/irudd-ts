#!/usr/bin/env python3
"""Retain the selected release CRT's actual license material and provenance."""
import argparse
import hashlib
import html
import json
import os
from pathlib import Path
import re
import urllib.request
import zipfile
import io

VS_TERMS = 'https://visualstudio.microsoft.com/wp-content/uploads/2021/11/Visual-Studio-2022-Enterprise-Professional-License-EN.docx'
MAX_NOTICE_BYTES = 4 * 1024 * 1024
LIBRARIES = ('libvcruntime.lib', 'libcmt.lib', 'libucrt.lib')
# This package declares the terms for the original 26100 SDK and its updates.
# It identifies the license source, not a NuGet package installed on the builder.
SDK_LICENSE_PACKAGE = {
    '10.0.26100.0': {
        'id': 'Microsoft.Windows.SDK.CPP', 'version': '10.0.26100.1',
        'packageUrl': 'https://www.nuget.org/packages/Microsoft.Windows.SDK.CPP/10.0.26100.1',
        'declaredLicenseUrl': 'https://aka.ms/WinSDKLicenseURL',
        'sourceUrl': 'https://download.microsoft.com/download/0/F/F/0FF2B061-47DD-4F55-89B6-FD1D8C44F14D/sdk_license.rtf',
    },
}


def sha256(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def notice_text(data, suffix):
    if suffix == '.docx':
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            entry = archive.getinfo('word/document.xml')
            if entry.file_size > MAX_NOTICE_BYTES:
                raise RuntimeError('Visual Studio license document exceeds its limit')
            data = archive.read(entry)
    encoding = 'utf-16' if data.startswith((b'\xff\xfe', b'\xfe\xff')) else 'utf-8-sig'
    text = data.decode(encoding, errors='replace')
    text = html.unescape(re.sub(r'<[^>]+>|\\[a-z]+[-0-9]* ?', ' ', text))
    return re.sub(r'\s+', ' ', text).upper()


def download(url):
    with urllib.request.urlopen(url, timeout=30) as response:
        if response.url != url:
            raise RuntimeError('Runtime license URL redirected unexpectedly')
        data = response.read(MAX_NOTICE_BYTES + 1)
    if len(data) > MAX_NOTICE_BYTES:
        raise RuntimeError('Runtime license document exceeds its limit')
    return data


def collect(binary, destination, selected, environment):
    if destination.exists():
        raise RuntimeError('Runtime notice destination must be fresh')
    if selected['productId'] not in ('Microsoft.VisualStudio.Product.Enterprise', 'Microsoft.VisualStudio.Product.Professional') or not selected['installationVersion'].startswith('17.'):
        raise RuntimeError('Unsupported Visual Studio license edition/version')
    visual_studio = Path(environment['VSINSTALLDIR']).resolve()
    if visual_studio != Path(selected['installationPath']).resolve():
        raise RuntimeError('Selected Visual Studio differs from the build environment')
    toolset_version = environment['VCToolsVersion'].rstrip('\\/')
    sdk_version = environment['WindowsSDKVersion'].rstrip('\\/')
    toolset = Path(environment['VCToolsInstallDir']).resolve()
    sdk = Path(environment['WindowsSdkDir']).resolve()
    if toolset != visual_studio / 'VC/Tools/MSVC' / toolset_version or not re.fullmatch(r'14\.\d+\.\d+', toolset_version) or not re.fullmatch(r'10\.0\.\d+\.\d+', sdk_version):
        raise RuntimeError('Toolset or SDK identity differs from the selected directories')
    linker = toolset / 'bin/Hostx64/x64/link.exe'
    if Path(selected['releaseLinkerPath']).resolve() != linker.resolve() or not linker.is_file():
        raise RuntimeError('Cargo release linker differs from the selected toolset')
    expected = {name: toolset / 'lib/x64' / name for name in LIBRARIES[:2]}
    expected['libucrt.lib'] = sdk / 'Lib' / sdk_version / 'ucrt/x64/libucrt.lib'
    search = [Path(value) for value in environment['LIB'].split(';') if value]
    libraries = []
    for name, path in expected.items():
        actual = next((directory / name for directory in search if (directory / name).is_file()), None)
        if actual is None or actual.resolve() != path.resolve():
            raise RuntimeError(f'Release LIB lookup does not select {path}')
        libraries.append({'name': name, 'sourcePath': str(path), 'sha256': sha256(path)})
    candidates = []
    for base in [sdk / 'Licenses', sdk / 'LicenseTerms']:
        if base.exists():
            for index, path in enumerate(base.rglob('*')):
                if index >= 10000:
                    raise RuntimeError(f'SDK notice discovery exceeds its limit: {base}')
                if path.is_file() and re.match(r'^(licen[cs]e|copyright|thirdpartynotices|credits)', path.name, re.I):
                    versions = [part for part in path.parts if re.fullmatch(r'10\.0\.\d+\.\d+', part)]
                    if not versions or sdk_version in versions:
                        candidates.append(path)
    candidates.extend(path for path in sdk.glob('*') if path.is_file() and re.match(r'^licen[cs]e', path.name, re.I))
    sdk_notices = []
    for path in sorted(set(candidates)):
        if path.suffix.lower() not in ('.txt', '.rtf', '.html') or path.stat().st_size > MAX_NOTICE_BYTES:
            continue
        data = path.read_bytes()
        sdk_notices.append((path, data, notice_text(data, path.suffix.lower())))
    license_package = None
    sdk_download = None
    if not any('SOFTWARE LICENSE TERMS' in text and re.search(r'WINDOWS.*SOFTWARE DEVELOPMENT KIT', text) for _, _, text in sdk_notices):
        license_package = SDK_LICENSE_PACKAGE.get(sdk_version)
        if license_package is None:
            raise RuntimeError(f'Missing actual installed Windows SDK terms for {sdk_version}; searched {sdk / "Licenses"}, {sdk / "LicenseTerms"}, and root license files; candidates: {[str(path) for path in candidates]}')
        sdk_download = download(license_package['sourceUrl'])
        text = notice_text(sdk_download, '.rtf')
        if 'SOFTWARE LICENSE TERMS' not in text or not re.search(r'WINDOWS.*SOFTWARE DEVELOPMENT KIT', text):
            raise RuntimeError('Missing applicable SDK package license terms')
    data = download(VS_TERMS)
    if not all(value in notice_text(data, '.docx') for value in ('VISUAL STUDIO', '2022', 'DISTRIBUTABLE CODE')):
        raise RuntimeError('Missing applicable Visual Studio 2022 license terms')
    destination.mkdir(parents=True)
    records = []
    def retain(relative, data, kind, source):
        path = destination / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        records.append({'file': relative, 'kind': kind, 'source': source, 'sha256': hashlib.sha256(data).hexdigest()})
    retain('visual-studio-license.docx', data, 'visual-studio', VS_TERMS)
    for path, data, _ in sdk_notices:
        retain('windows-sdk/' + path.relative_to(sdk).as_posix(), data, 'windows-sdk', str(path))
    if sdk_download is not None:
        retain('windows-sdk/sdk-license.rtf', sdk_download, 'windows-sdk', license_package['sourceUrl'])
    record = {'schemaVersion': 1, 'target': 'x86_64-pc-windows-msvc', 'runtime': 'static-release',
              'binarySha256': sha256(binary), 'visualStudio': {'productId': selected['productId'], 'version': selected['installationVersion'], 'installationPath': str(visual_studio)},
              'toolsetVersion': toolset_version, 'windowsSdkVersion': sdk_version, 'windowsSdkInstallationPath': str(sdk),
              'linker': {'sourcePath': str(linker), 'sha256': sha256(linker)},
              'sdkLicensePackage': license_package, 'libraries': libraries, 'notices': records}
    (destination / 'provenance.json').write_text(json.dumps(record, indent=2) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--destination', type=Path, required=True)
    parser.add_argument('--visual-studio', type=Path, required=True)
    args = parser.parse_args()
    collect(args.binary, args.destination, json.loads(args.visual_studio.read_text(encoding='utf-8-sig')), os.environ)
