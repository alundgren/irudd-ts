#!/usr/bin/env python3
"""Guard manual release preparation and publish one merged release PR."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile

PLATFORMS = ['linux-x86_64', 'linux-arm64', 'macos-x86_64', 'macos-arm64']
VERSION = re.compile(r'^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$')


def run(*args):
    return subprocess.check_output(list(args), text=True).strip()


def api(path, *args):
    return json.loads(run('gh', 'api', path, *args))


def version(value):
    if not isinstance(value, str) or not VERSION.fullmatch(value):
        raise RuntimeError(f'Invalid stable version: {value!r}')
    return tuple(map(int, value.split('.')))


def public_release(repo, value):
    for page in json.loads(run('gh', 'api', f'repos/{repo}/releases', '--paginate', '--slurp')):
        for release in page:
            if release['tag_name'] == f'v{value}':
                if release['draft'] or release['prerelease']:
                    raise RuntimeError(f'Previous version v{value} has not been published')
                commit = tag_commit(repo, release['tag_name'])
                if commit is None or release['target_commitish'] != commit:
                    raise RuntimeError(f'Previous public release v{value} has a missing or conflicting tag')
                content = api(f'repos/{repo}/contents/.release-please-manifest.json?ref={commit}')
                identity = json.loads(base64.b64decode(content['content']))
                if identity.get('.') != value:
                    raise RuntimeError(f'Previous public release v{value} has a conflicting manifest version')
                return release
    raise RuntimeError(f'Previous version v{value} has not been published')


def prepare(repo):
    current = json.loads(Path('.release-please-manifest.json').read_text())
    if current:
        public_release(repo, current['.'])


def git_file(commit, path):
    return run('git', 'show', f'{commit}:{path}')


def package_version(content):
    section = re.search(r'^\[package\]\s*\n(.*?)(?=^\[|\Z)', content, re.M | re.S)
    if not section:
        raise RuntimeError('Missing Cargo package section')
    result = re.search(r'^version\s*=\s*"([^"]+)"\s*$', section[1], re.M)
    if not result:
        raise RuntimeError('Missing explicit Cargo package version')
    return result[1]


def release_info(repo, number):
    repository = api(f'repos/{repo}')
    pr = api(f'repos/{repo}/pulls/{number}')
    labels = {label['name'] for label in pr['labels']}
    if not pr['merged'] or pr['base']['ref'] != repository['default_branch'] or pr['base']['repo']['full_name'] != repo:
        raise RuntimeError('Expected a merged release PR targeting the default branch')
    if not pr['head']['ref'].startswith('release-please--') or not labels.intersection({'autorelease: pending', 'autorelease: tagged'}):
        raise RuntimeError('Expected a release-please PR with pending or tagged release label')
    commit = pr['merge_commit_sha']
    if not re.fullmatch(r'[a-f0-9]{40}', commit):
        raise RuntimeError('Missing exact merged commit')
    run('git', 'fetch', 'origin', f"+refs/heads/{repository['default_branch']}:refs/remotes/origin/{repository['default_branch']}", commit)
    subprocess.run(['git', 'merge-base', '--is-ancestor', commit, f"origin/{repository['default_branch']}"], check=True)
    value = json.loads(git_file(commit, '.release-please-manifest.json'))['.']
    version(value)
    cargo = package_version(git_file(commit, 'Cargo.toml'))
    lock = git_file(commit, 'Cargo.lock')
    match = re.search(r'^name = "archguard"\nversion = "([^"]+)"$', lock, re.M)
    if cargo != value or not match or match[1] != value:
        raise RuntimeError('Cargo.toml, Cargo.lock, and release manifest versions disagree')
    previous = json.loads(git_file(f'{commit}^', '.release-please-manifest.json'))
    if previous:
        old = previous['.']
        if version(value) <= version(old):
            raise RuntimeError('Release version must increase')
        public_release(repo, old)
    elif value != '0.1.0':
        raise RuntimeError('First release must be 0.1.0')
    changelog = git_file(commit, 'CHANGELOG.md')
    heading = re.search(r'^##\s+(?:\[)?' + re.escape(value) + r'(?:\])?(?=\s|\().*$', changelog, re.M)
    if not heading:
        raise RuntimeError('Release changelog section is missing')
    notes = changelog[heading.end():].split('\n## ', 1)[0].strip()
    if not notes:
        raise RuntimeError('Release changelog section is empty')
    changed = set(run('git', 'diff', '--name-only', f'{commit}^', commit).splitlines())
    if not {'.release-please-manifest.json', 'CHANGELOG.md'}.issubset(changed):
        raise RuntimeError('Merged commit must update both release manifest and changelog')
    return {'version': value, 'commit': commit, 'notes': notes, 'pr': number}


def asset_names(value):
    return [f'archguard-{value}-{platform}.tar.gz{suffix}' for platform in PLATFORMS for suffix in ['', '.sha256']]


def validate_assets(directory, info):
    expected = asset_names(info['version'])
    actual = sorted(path.name for path in directory.iterdir() if path.is_file())
    if sorted(expected) != actual:
        raise RuntimeError('Expected exactly four archives and their four checksum files')
    for platform in PLATFORMS:
        name = f"archguard-{info['version']}-{platform}.tar.gz"
        archive = directory / name
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        if (directory / f'{name}.sha256').read_text() != f'{digest}  {name}\n':
            raise RuntimeError(f'Archive checksum mismatch: {name}')
        with tarfile.open(archive, 'r:gz') as package:
            stream = package.extractfile(f"archguard-{info['version']}/release.json")
            if stream is None:
                raise RuntimeError(f'Missing release identity: {name}')
            identity = json.load(stream)
            if any(identity.get(key) != expected for key, expected in
                   [('version', info['version']), ('commit', info['commit']), ('platform', platform)]):
                raise RuntimeError(f'Archive release identity mismatch: {name}')
    return expected


def tag_commit(repo, tag):
    refs = api(f'repos/{repo}/git/matching-refs/tags/{tag}')
    ref = next((ref for ref in refs if ref['ref'] == f'refs/tags/{tag}'), None)
    if ref is None:
        return None
    obj = ref['object']
    while obj['type'] == 'tag':
        obj = api(f"repos/{repo}/git/tags/{obj['sha']}")['object']
    if obj['type'] != 'commit':
        raise RuntimeError('Release tag does not identify a commit')
    return obj['sha']


def release_for_tag(repo, tag):
    pages = json.loads(run('gh', 'api', f'repos/{repo}/releases', '--paginate', '--slurp'))
    return next((r for page in pages for r in page if r['tag_name'] == tag), None)


def verify_public_assets(repo, release, info):
    expected = asset_names(info['version'])
    names = [asset['name'] for asset in release['assets']]
    if sorted(names) != sorted(expected):
        raise RuntimeError('Published release assets are incomplete or conflicting')
    with tempfile.TemporaryDirectory() as temp:
        directory = Path(temp)
        for asset in release['assets']:
            with (directory / asset['name']).open('wb') as stream:
                subprocess.run(['gh', 'api', f"repos/{repo}/releases/assets/{asset['id']}",
                                '-H', 'Accept: application/octet-stream'], stdout=stream, check=True)
        validate_assets(directory, info)


def verify_remote(repo, release, directory, expected, complete):
    actual = [asset['name'] for asset in release['assets']]
    if len(actual) != len(set(actual)) or not set(actual).issubset(expected) or (complete and sorted(actual) != sorted(expected)):
        raise RuntimeError('Conflicting or incomplete release assets')
    with tempfile.TemporaryDirectory() as temp:
        for asset in release['assets']:
            remote = Path(temp) / asset['name']
            with remote.open('wb') as stream:
                subprocess.run(['gh', 'api', f"repos/{repo}/releases/assets/{asset['id']}",
                                '-H', 'Accept: application/octet-stream'], stdout=stream, check=True)
            if remote.read_bytes() != (directory / asset['name']).read_bytes():
                raise RuntimeError(f"Conflicting remote asset: {asset['name']}")
    return set(actual)


def publish(repo, info, directory):
    tag = f"v{info['version']}"
    existing_commit = tag_commit(repo, tag)
    if existing_commit is not None and existing_commit != info['commit']:
        raise RuntimeError('Conflicting existing release tag')
    release = release_for_tag(repo, tag)
    if release and release['prerelease']:
        raise RuntimeError('Conflicting prerelease')
    if release and not release['draft']:
        if existing_commit != info['commit']:
            raise RuntimeError('Published release tag is missing or conflicts')
        if release['body'].strip() != info['notes'].strip():
            raise RuntimeError('Published release notes conflict with the changelog')
        verify_public_assets(repo, release, info)
    else:
        if directory is None:
            raise RuntimeError('A draft release requires all four built archives and checksum files')
        expected = validate_assets(directory, info)
        if release:
            if release['target_commitish'] != info['commit'] or release['body'].strip() != info['notes'].strip():
                raise RuntimeError('Conflicting draft release commit or notes')
        else:
            with tempfile.NamedTemporaryFile(mode='w') as notes:
                notes.write(info['notes']); notes.flush()
                run('gh', 'release', 'create', tag, '--repo', repo, '--draft', '--target', info['commit'],
                    '--title', tag, '--notes-file', notes.name)
            release = release_for_tag(repo, tag)
            if not release or not release['draft']:
                raise RuntimeError('Draft creation could not be verified')
        present = verify_remote(repo, release, directory, expected, complete=False)
        for name in expected:
            if name not in present:
                run('gh', 'release', 'upload', tag, str(directory / name), '--repo', repo)
        release = release_for_tag(repo, tag)
        verify_remote(repo, release, directory, expected, complete=True)
        if tag_commit(repo, tag) not in [None, info['commit']]:
            raise RuntimeError('Conflicting tag before publication')
        run('gh', 'release', 'edit', tag, '--repo', repo, '--draft=false', '--latest')
        release = release_for_tag(repo, tag)
        if not release or release['draft'] or tag_commit(repo, tag) != info['commit']:
            raise RuntimeError('Publication could not be verified')
        verify_remote(repo, release, directory, expected, complete=True)
    # release-please creates the tagged label only when its own release publisher runs.
    pages = api(f'repos/{repo}/labels?per_page=100', '--paginate', '--slurp')
    if not any(label['name'] == 'autorelease: tagged' for page in pages for label in page):
        run('gh', 'label', 'create', 'autorelease: tagged', '--repo', repo, '--color', 'ededed',
            '--description', 'Release published')
    # release-please refuses preparation while a merged PR remains pending.
    run('gh', 'pr', 'edit', str(info['pr']), '--repo', repo, '--add-label', 'autorelease: tagged')
    labels = {label['name'] for label in api(f"repos/{repo}/issues/{info['pr']}")['labels']}
    if 'autorelease: pending' in labels:
        run('gh', 'pr', 'edit', str(info['pr']), '--repo', repo, '--remove-label', 'autorelease: pending')
    labels = {label['name'] for label in api(f"repos/{repo}/issues/{info['pr']}")['labels']}
    if 'autorelease: pending' in labels or 'autorelease: tagged' not in labels:
        raise RuntimeError('Publication succeeded but release labels need reconciliation; rerun this release PR')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['prepare', 'validate', 'publish'])
    parser.add_argument('--repo', default=os.environ.get('GITHUB_REPOSITORY'))
    parser.add_argument('--pr', type=int)
    parser.add_argument('--assets', type=Path)
    args = parser.parse_args()
    if not args.repo or not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', args.repo):
        parser.error('An explicit owner/repository is required')
    if args.operation == 'prepare':
        prepare(args.repo)
    else:
        if args.pr is None or args.pr < 1:
            parser.error('--pr must be a positive release PR number')
        info = release_info(args.repo, args.pr)
        if args.operation == 'validate':
            existing = release_for_tag(args.repo, f"v{info['version']}")
            info['published'] = bool(existing and not existing['draft'])
            print(json.dumps(info))
            if os.environ.get('GITHUB_OUTPUT'):
                with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
                    output.write(f"version={info['version']}\ncommit={info['commit']}\npublished={str(info['published']).lower()}\n")
        else:
            publish(args.repo, info, args.assets)
