#!/usr/bin/env python3
"""Release guards exercise failures, corrected retries, and public replay."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def module(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / f'scripts/release/{name}.py')
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


github = module('github')
packaging = module('package')


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.info = {'version': '0.1.0', 'commit': 'a' * 40, 'notes': '### Features\n\n* Checks', 'pr': 123}
        for platform in github.PLATFORMS:
            name = f'archguard-0.1.0-{platform}.tar.gz'
            data = json.dumps({**self.info, 'platform': platform}).encode()
            with tarfile.open(self.directory / name, 'w:gz') as archive:
                item = tarfile.TarInfo('archguard-0.1.0/release.json')
                item.size = len(data)
                archive.addfile(item, io.BytesIO(data))
            digest = hashlib.sha256((self.directory / name).read_bytes()).hexdigest()
            (self.directory / f'{name}.sha256').write_text(f'{digest}  {name}\n')

    def test_asset_identity_and_checksum(self):
        self.assertEqual(len(github.validate_assets(self.directory, self.info)), 8)
        wrong = {**self.info, 'commit': 'b' * 40}
        with self.assertRaisesRegex(RuntimeError, 'identity mismatch'):
            github.validate_assets(self.directory, wrong)
        checksum = self.directory / 'archguard-0.1.0-linux-arm64.tar.gz.sha256'
        original = checksum.read_text()
        checksum.write_text('0' * 64 + '  archguard-0.1.0-linux-arm64.tar.gz\n')
        with self.assertRaisesRegex(RuntimeError, 'checksum mismatch'):
            github.validate_assets(self.directory, self.info)
        checksum.write_text(original)
        github.validate_assets(self.directory, self.info)
        (self.directory / 'unexpected').write_text('x')
        with self.assertRaisesRegex(RuntimeError, 'exactly four'):
            github.validate_assets(self.directory, self.info)

    def test_unpublished_previous_blocks_preparation(self):
        with patch.object(github.Path, 'read_text', return_value='{".": "0.1.0"}'), \
             patch.object(github, 'run', return_value=json.dumps([[{'tag_name': 'v0.1.0', 'draft': True, 'prerelease': False}]])), \
             patch.object(github, 'api', return_value={'can_approve_pull_request_reviews': True}):
            with self.assertRaisesRegex(RuntimeError, 'not been published'):
                github.prepare('owner/repo')
        with patch.object(github.Path, 'read_text', return_value='{}'), \
             patch.object(github, 'api', return_value={'can_approve_pull_request_reviews': True}):
            github.prepare('owner/repo')

    def test_previous_public_tag_and_manifest_identity(self):
        release = {'tag_name': 'v0.1.0', 'draft': False, 'prerelease': False,
                   'target_commitish': self.info['commit']}
        import base64
        def identity(value):
            return {'content': base64.b64encode(json.dumps({'.': value}).encode()).decode()}
        with patch.object(github, 'run', return_value=json.dumps([[release]])), \
             patch.object(github, 'tag_commit', return_value=None):
            with self.assertRaisesRegex(RuntimeError, 'missing or conflicting tag'):
                github.public_release('owner/repo', '0.1.0')
        with patch.object(github, 'run', return_value=json.dumps([[release]])), \
             patch.object(github, 'tag_commit', return_value=self.info['commit']), \
             patch.object(github, 'api', return_value=identity('0.2.0')):
            with self.assertRaisesRegex(RuntimeError, 'conflicting manifest'):
                github.public_release('owner/repo', '0.1.0')
        with patch.object(github, 'run', return_value=json.dumps([[release]])), \
             patch.object(github, 'tag_commit', return_value=self.info['commit']), \
             patch.object(github, 'api', return_value=identity('0.1.0')):
            self.assertEqual(github.public_release('owner/repo', '0.1.0'), release)

    def test_public_assets_verify_without_fresh_build(self):
        names = github.asset_names('0.1.0')
        release = {'assets': [{'name': name, 'id': index} for index, name in enumerate(names)]}
        def download(args, stdout, check):
            asset_id = int(args[2].rsplit('/', 1)[1])
            stdout.write((self.directory / names[asset_id]).read_bytes())
        with patch.object(github.subprocess, 'run', side_effect=download):
            github.verify_public_assets('owner/repo', release, self.info)
            checksum = self.directory / names[1]
            original = checksum.read_text()
            checksum.write_text('invalid checksum')
            with self.assertRaisesRegex(RuntimeError, 'checksum mismatch'):
                github.verify_public_assets('owner/repo', release, self.info)
            checksum.write_text(original)
            github.verify_public_assets('owner/repo', release, self.info)
            with self.assertRaisesRegex(RuntimeError, 'identity mismatch'):
                github.verify_public_assets('owner/repo', release, {**self.info, 'commit': 'b' * 40})
        release['assets'].pop()
        with self.assertRaisesRegex(RuntimeError, 'incomplete'):
            github.verify_public_assets('owner/repo', release, self.info)

    def test_conflicting_tag_prevents_writes(self):
        with patch.object(github, 'tag_commit', return_value='b' * 40), patch.object(github, 'run') as run:
            with self.assertRaisesRegex(RuntimeError, 'Conflicting existing'):
                github.publish('owner/repo', self.info, self.directory)
            run.assert_not_called()

    def test_draft_retry_and_public_replay(self):
        release = {'draft': True, 'prerelease': False, 'target_commitish': self.info['commit'],
                   'body': self.info['notes'], 'assets': []}
        present = set(github.asset_names('0.1.0')[:3])
        calls = []
        labels = {'autorelease: pending'}

        def command(*args):
            calls.append(args)
            if '--add-label' in args:
                labels.add(args[-1])
            if '--remove-label' in args:
                labels.discard(args[-1])
            if args[1:3] == ('release', 'edit'):
                release['draft'] = False
            return ''

        with patch.object(github, 'tag_commit', return_value=self.info['commit']), \
             patch.object(github, 'release_for_tag', return_value=release), \
             patch.object(github, 'verify_remote', return_value=present), \
             patch.object(github, 'verify_public_assets') as verify_public, \
             patch.object(github, 'api', side_effect=lambda path, *args: [[{'name': 'autorelease: tagged'}]] if '/labels?' in path else {'labels': [{'name': name} for name in labels]}), \
             patch.object(github, 'run', side_effect=command):
            github.publish('owner/repo', self.info, self.directory)
            uploads = [call for call in calls if call[1:3] == ('release', 'upload')]
            self.assertEqual(len(uploads), 5)
            self.assertTrue(any(call[1:3] == ('release', 'edit') for call in calls))
            calls.clear()
            github.publish('owner/repo', self.info, None)
            verify_public.assert_called_once()
            self.assertFalse(any(call[1] == 'release' for call in calls))

    def test_failed_upload_leaves_draft_and_labels(self):
        release = {'draft': True, 'prerelease': False, 'target_commitish': self.info['commit'],
                   'body': self.info['notes'], 'assets': []}
        with patch.object(github, 'tag_commit', return_value=None), \
             patch.object(github, 'release_for_tag', return_value=release), \
             patch.object(github, 'verify_remote', return_value=set()), \
             patch.object(github, 'run', side_effect=subprocess.CalledProcessError(1, 'upload')) as run:
            with self.assertRaises(subprocess.CalledProcessError):
                github.publish('owner/repo', self.info, self.directory)
            self.assertTrue(release['draft'])
            self.assertEqual(run.call_args.args[1:3], ('release', 'upload'))

    def test_remote_asset_conflicts_are_preserved(self):
        release = {'assets': [{'name': 'unexpected', 'id': 1}]}
        with self.assertRaisesRegex(RuntimeError, 'Conflicting'):
            github.verify_remote('owner/repo', release, self.directory, github.asset_names('0.1.0'), False)
        release['assets'][0]['name'] = github.asset_names('0.1.0')[0]
        with patch.object(github.subprocess, 'run') as download:
            with self.assertRaisesRegex(RuntimeError, 'Conflicting remote asset'):
                github.verify_remote('owner/repo', release, self.directory, github.asset_names('0.1.0'), False)
            download.assert_called_once()

    def test_installer_checksum_version_and_unsafe_paths(self):
        os_name = 'macos' if subprocess.check_output(['uname', '-s'], text=True).strip() == 'Darwin' else 'linux'
        machine = subprocess.check_output(['uname', '-m'], text=True).strip()
        arch = 'arm64' if machine in ['aarch64', 'arm64'] else 'x86_64'
        name = f'archguard-0.1.0-{os_name}-{arch}.tar.gz'
        directory = self.directory / 'installer'
        directory.mkdir()
        def create(version='0.1.0', unsafe=False):
            with tarfile.open(directory / name, 'w:gz') as archive:
                for path, data, mode in [
                    ('VERSION', b'0.1.0\n', 0o644),
                    ('bin/archguard', f'#!/bin/sh\necho "archguard {version}"\n'.encode(), 0o755)]:
                    item = tarfile.TarInfo('archguard-0.1.0/' + path)
                    item.mode, item.size = mode, len(data)
                    archive.addfile(item, io.BytesIO(data))
                if unsafe:
                    item = tarfile.TarInfo('../escaped')
                    item.size = 1
                    archive.addfile(item, io.BytesIO(b'x'))
            digest = hashlib.sha256((directory / name).read_bytes()).hexdigest()
            (directory / f'{name}.sha256').write_text(f'{digest}  {name}\n')
        def install(destination):
            import os
            env = {**os.environ, 'ARCHGUARD_DOWNLOAD_BASE_URL': directory.as_uri()}
            return subprocess.run(['bash', str(ROOT / 'scripts/install.sh'), '0.1.0', str(destination)], env=env, capture_output=True, text=True)
        create()
        checksum = directory / f'{name}.sha256'
        original = checksum.read_text()
        checksum.write_text('0' * 64 + f'  {name}\n')
        destination = self.directory / 'installed'
        self.assertEqual(install(destination).returncode, 2)
        self.assertFalse(destination.exists())
        checksum.write_text(original)
        self.assertEqual(install(destination).returncode, 0)
        self.assertEqual(install(destination).returncode, 2)
        create(version='0.2.0')
        self.assertEqual(install(self.directory / 'wrong-version').returncode, 2)
        create(unsafe=True)
        self.assertEqual(install(self.directory / 'unsafe').returncode, 2)
        self.assertFalse((self.directory / 'escaped').exists())

    def test_reviewed_upstream_notice_checksums(self):
        directory = ROOT / 'docs/licenses/upstream'
        for record in json.loads((directory / 'index.json').read_text()):
            self.assertTrue(record['commit'])
            for notice in record['notices']:
                self.assertEqual(hashlib.sha256((directory / notice['file']).read_bytes()).hexdigest(), notice['sha256'])

    def test_merged_pr_version_controls(self):
        pr = {'merged': True, 'base': {'ref': 'main', 'repo': {'full_name': 'owner/repo'}},
              'head': {'ref': 'release-please--branches--main'}, 'labels': [{'name': 'autorelease: pending'}],
              'merge_commit_sha': 'a' * 40}
        current = {'value': '0.1.0', 'previous': {}}
        def content(commit, path):
            if path == '.release-please-manifest.json':
                return json.dumps(current['previous'] if commit.endswith('^') else {'.': current['value']})
            if path == 'Cargo.toml':
                return '[package]\nname = "archguard"\nversion = "' + current['value'] + '"\n'
            if path == 'Cargo.lock':
                return '[[package]]\nname = "archguard"\nversion = "' + current['value'] + '"\n'
            return '## ' + current['value'] + ' (2026-10-05)\n\n### Features\n\n* Checks\n'
        with patch.object(github, 'api', side_effect=lambda path, *args: pr if '/pulls/' in path else {'default_branch': 'main'}), \
             patch.object(github, 'git_file', side_effect=content), \
             patch.object(github, 'run', side_effect=lambda *args: '.release-please-manifest.json\nCHANGELOG.md' if args[1] == 'diff' else ''), \
             patch.object(github.subprocess, 'run'), \
             patch.object(github, 'public_release') as previous:
            self.assertEqual(github.release_info('owner/repo', 123)['version'], '0.1.0')
            previous.assert_not_called()
            current.update(value='0.1.1', previous={'.': '0.1.0'})
            self.assertEqual(github.release_info('owner/repo', 123)['version'], '0.1.1')
            previous.assert_called_with('owner/repo', '0.1.0')
            current['value'] = '0.2.0'
            self.assertEqual(github.release_info('owner/repo', 123)['version'], '0.2.0')
            current['value'] = '0.1.0'
            with self.assertRaisesRegex(RuntimeError, 'must increase'):
                github.release_info('owner/repo', 123)
            current.update(value='0.2.0', previous={})
            with self.assertRaisesRegex(RuntimeError, 'First release'):
                github.release_info('owner/repo', 123)
            current.update(value='0.1.0', previous={})
            pr['merged'] = False
            with self.assertRaisesRegex(RuntimeError, 'merged release PR'):
                github.release_info('owner/repo', 123)

    def test_normal_closure_and_nested_notices(self):
        source = self.directory / 'dependency'
        (source / 'src').mkdir(parents=True)
        (source / 'LICENSE-MIT').write_text('Permission')
        (source / 'src/LICENSE-UNICODE').write_text('Unicode notice')
        data = {'resolve': {'root': 'root', 'nodes': [
            {'id': 'root', 'deps': [{'pkg': 'normal', 'dep_kinds': [{'kind': None}]},
                                   {'pkg': 'dev', 'dep_kinds': [{'kind': 'dev'}]}]},
            {'id': 'normal', 'deps': []}, {'id': 'dev', 'deps': []}]},
            'packages': [{'id': key, 'name': key, 'version': '1.0.0', 'license': 'MIT AND Unicode-3.0',
                          'manifest_path': str(source / 'Cargo.toml')} for key in ['root', 'normal', 'dev']]}
        selected = packaging.dependencies(data)
        self.assertEqual([p['name'] for p in selected], ['normal', 'root'])
        packaging.notices(selected, self.directory / 'notices')
        self.assertEqual((self.directory / 'notices/normal-1.0.0/src/LICENSE-UNICODE').read_text(), 'Unicode notice')
        (source / 'LICENSE-MIT').unlink()
        (source / 'src/LICENSE-UNICODE').unlink()
        with self.assertRaisesRegex(RuntimeError, 'Missing actual license'):
            packaging.notices(selected, self.directory / 'missing')


if __name__ == '__main__':
    unittest.main()
