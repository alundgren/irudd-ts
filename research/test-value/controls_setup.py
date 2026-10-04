#!/usr/bin/env python3
"""Focused owned-source and historical workspace setup controls."""
import argparse
import errno
import json
import os
from pathlib import Path
import shutil
import subprocess
from unittest.mock import patch

from controls_runner import fixture, require
from history import COMMAND, configure, scope_workspace_adapter, workspace_import_controls
from runner import DependencyStore, copy_owned_source, execute_case, install_signal_handlers, node_fingerprint, read_json, write_json
from rust_slice import archive


def source_controls(output):
    directory = output / 'source-controls'
    directory.mkdir()
    template = fixture(directory / 'template', {'value.txt': 'original', 'folder/inside.txt': 'inside'})
    (template / 'CLAUDE.md').symlink_to('AGENTS.md\n')
    (template / 'relative').symlink_to('value.txt')
    (template / 'absolute').symlink_to(template / 'value.txt')
    (template / 'directory').symlink_to('folder')
    try:
        shutil.copytree(template, directory / 'before-copy')
    except shutil.Error as error:
        require('CLAUDE.md' in str(error), 'The real before-copy failure must be the dangling archived link')
        write_json(directory / 'before.json', {'expectedFailure': str(error)})
    else:
        raise AssertionError('Before-copy dangling link unexpectedly succeeded')
    records = []
    for order in [(0, 1), (1, 0)]:
        copies = [directory / f'copy-{order[0]}-{index}' for index in range(2)]
        for destination in copies:
            copy_owned_source(template, destination)
            require(os.readlink(destination / 'CLAUDE.md') == 'AGENTS.md\n', 'Dangling relative target bytes must remain exact')
            require(os.readlink(destination / 'relative') == 'value.txt', 'Relative links must remain links')
            require((destination / 'directory/inside.txt').read_text() == 'inside', 'Owned directory links must work')
            require(os.readlink(destination / 'absolute') == str(destination / 'value.txt'), 'Internal absolute link must point into the new copy')
        (copies[order[0]] / 'relative').write_text('changed')
        require((copies[order[1]] / 'absolute').read_text() == 'original', 'A second execution must read its own unchanged target')
        require((template / 'value.txt').read_text() == 'original', 'Mutable link targets must never change the template')
        records.append({'control': f'fresh-linked-target-order-{order[0]}-{order[1]}', 'passed': True})
    outside = fixture(directory / 'outside', {'value.txt': 'outside'})
    negatives = {
        'relative-escape': {'link': '../outside/value.txt'},
        'absolute-escape': {'link': str(outside / 'value.txt')},
        'chained-escape': {'link': 'other/value.txt', 'other': '../outside'},
        'directory-escape': {'diralias': '../outside'},
        'ancestor-escape-return': {'outside': '../outside', 'link': 'outside/../ancestor-escape-return/value.txt'},
        'link-cycle': {'one': 'two', 'two': 'one'},
        'directory-ancestor-cycle': {'folder/loop': '..'},
        'directory-sibling-cycle': {'left/link': '../right', 'right/link': '../left'},
        'absolute-traversal-escape': {'link': str(directory / 'absolute-traversal-escape') + '/../outside/value.txt'},
    }
    for name, links in negatives.items():
        source = fixture(directory / name, {'value.txt': 'original', 'folder/inside.txt': 'inside',
                                             'left/value.txt': 'left', 'right/value.txt': 'right'})
        for relative, target in links.items():
            path = source / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.symlink_to(target)
        try:
            copy_owned_source(source, directory / (name + '-copy'))
        except ValueError as error:
            records.append({'control': name, 'passed': True, 'error': str(error)})
        else:
            raise AssertionError(f'Unsafe source copy accepted: {name}')
        require((outside / 'value.txt').read_text() == 'outside', 'Negative copy must preserve external bytes')
    large = fixture(directory / 'large-source', {'large.bin': 'x' * (3 * 1024 * 1024)})
    partial = directory / 'bounded-copy'

    def stop_after_chunk(_directory, minimum=0.12):
        copied = partial / 'large.bin'
        if copied.exists() and copied.stat().st_size >= 1024 * 1024:
            raise RuntimeError('Injected disk boundary after one copied chunk')
        return {}

    with patch('runner.clone_regular_file', side_effect=OSError(errno.ENOTSUP, 'Authored unsupported clone control')), patch('runner.disk_check', side_effect=stop_after_chunk):
        try:
            copy_owned_source(large, partial)
        except RuntimeError:
            require((partial / 'large.bin').stat().st_size == 1024 * 1024, 'Disk boundary must stop copying at the next bounded chunk')
        else:
            raise AssertionError('Source copy ignored the disk boundary')
    records.append({'control': 'bounded-copy-disk-boundary', 'passed': True})
    write_json(directory / 'controls.json', records)
    return records


def load_dependencies(directory, installed, subject='scope'):
    metadata = read_json(directory / (subject + '-dependencies.json'))
    store = DependencyStore(installed, directory / (subject + '-dependencies'))
    store.modules, store.workspace_links = metadata['modules'], metadata['workspaceLinks']
    store.owned_directories, store.sha256 = set(metadata['ownedDirectories']), metadata['sha256']
    require(store.current_digest() == store.sha256, 'Explicit owned dependency store must match its retained hash')
    return store


def workspace_controls(output, dependencies, node):
    directory = output / 'workspace-controls'
    directory.mkdir()
    template = fixture(directory / 'template', {
        'package.json': '{"type":"module"}',
        'packages/sqlite/package.json': '{"name":"@irudd-scope/sqlite","exports":{".":"./src/maintenance.ts"}}',
        'packages/sqlite/src/maintenance.ts': 'export const value=1;\n',
        'apps/desktop/package.json': '{"dependencies":{"@irudd-scope/sqlite":"workspace:*"}}',
        'case.test.ts': 'import{test,expect}from"vite-plus/test";import{value}from"@irudd-scope/sqlite";test("owned workspace",()=>expect(value).toBe(1));',
    })
    for relative in set(dependencies.workspace_links.values()):
        path = template / relative
        if not path.exists():
            path.mkdir(parents=True)
            (path / 'control.txt').write_text('Owned unused workspace directory\n')
    reporter = Path(__file__).with_name('vitest-reporter.ts')
    adapters = configure(template, ['case.test.ts'], reporter)
    controls = workspace_import_controls(adapters)
    fixed = execute_case(template, directory / 'fixed', COMMAND, dependencies=dependencies, node=node, import_controls=controls)
    require(fixed['complete'] and fixed['status'] == 'survived', 'Declared historical workspace must execute its own export')
    require(len(fixed['importControls']) == 1 and '/fixed/source/' in fixed['importControls'][0]['realPath'],
            'Actual test resolver evidence must identify the fresh source')
    changed = directory / 'changed-template'
    copy_owned_source(template, changed)
    (changed / 'packages/sqlite/src/maintenance.ts').write_text('export const value=2;\n')
    changed_result = execute_case(changed, directory / 'changed', COMMAND, dependencies=dependencies, node=node,
                                  baseline=fixed['tests'], import_controls=controls)
    require(changed_result['complete'] and changed_result['status'] == 'killed', 'Changed fresh workspace source must be observed by the actual test')
    require((template / 'packages/sqlite/src/maintenance.ts').read_text() == 'export const value=1;\n', 'Template workspace bytes must remain unchanged')
    restored = execute_case(template, directory / 'restored', COMMAND, dependencies=dependencies, node=node,
                            baseline=fixed['tests'], import_controls=controls)
    require(restored['complete'] and restored['status'] == 'survived', 'Restored own workspace must pass')
    subpath = directory / 'subpath-template'
    copy_owned_source(template, subpath)
    (subpath / 'case.test.ts').write_text('import{test,expect}from"vite-plus/test";import{value}from"@irudd-scope/sqlite/undeclared";test("owned workspace",()=>expect(value).toBe(1));')
    rejected = execute_case(subpath, directory / 'undeclared-subpath', COMMAND, dependencies=dependencies, node=node,
                            baseline=fixed['tests'], import_controls=controls)
    require(not rejected['complete'] and all(v == 'unknown' for v in rejected['outcomes'].values()), 'Exact alias must never authorize a subpath')
    mismatch = directory / 'mismatched-resolution-template'
    copy_owned_source(template, mismatch)
    (mismatch / 'case.test.ts').write_text((template / 'case.test.ts').read_text() +
        'import{afterAll}from"vite-plus/test";import fs from"node:fs";afterAll(()=>{const p="./research-workspace-resolution.json";'
        'const evidence=JSON.parse(fs.readFileSync(p,"utf8"));evidence.realPath=evidence.sourceRoot+"/wrong.ts";fs.writeFileSync(p,JSON.stringify(evidence));});')
    wrong = execute_case(mismatch, directory / 'mismatched-resolution', COMMAND, dependencies=dependencies, node=node,
                         baseline=fixed['tests'], import_controls=controls)
    require(wrong['protocol']['exitCode'] == 0 and not wrong['complete'] and all(v == 'unknown' for v in wrong['outcomes'].values()),
            'Passing tests with altered actual-resolution evidence must reject the entire column')
    deleted = directory / 'deleted-export-template'
    copy_owned_source(template, deleted)
    (deleted / 'case.test.ts').write_text((template / 'case.test.ts').read_text() +
        'import{afterAll}from"vite-plus/test";import fs from"node:fs";afterAll(()=>{fs.unlinkSync("./packages/sqlite/src/maintenance.ts");});')
    deletion = execute_case(deleted, directory / 'deleted-export', COMMAND, dependencies=dependencies, node=node,
                            baseline=fixed['tests'], import_controls=controls)
    require(deletion['protocol']['exitCode'] == 0 and not deletion['complete'] and
            all(v == 'unknown' for v in deletion['outcomes'].values()) and
            any('Workspace resolver evidence failed' in message for message in deletion['infrastructureErrors']) and
            not any(record.get('observed') is False for record in deletion['importControls']),
            'Passing tests that delete an imported export in teardown must retain resolver validation failure and unknown cells')
    unused = directory / 'unused-alias-template'
    copy_owned_source(template, unused)
    (unused / 'case.test.ts').write_text('import{test,expect}from"vite-plus/test";test("unrelated",()=>expect(1).toBe(1));')
    unused_result = execute_case(unused, directory / 'unused-alias', COMMAND, dependencies=dependencies, node=node, import_controls=controls)
    require(unused_result['complete'] and unused_result['importControls'] == [{'specifier': '@irudd-scope/sqlite', 'observed': False}],
            'An unused declared alias must remain explicit and cannot claim actual resolution')
    for name, mutate in [
        ('unsupported-export', lambda p: (p / 'packages/sqlite/package.json').write_text('{"name":"@irudd-scope/sqlite","exports":{".":"./other.ts"}}')),
        ('undeclared-consumer', lambda p: (p / 'apps/desktop/package.json').write_text('{"dependencies":{}}')),
        ('escaping-export', lambda p: ((p / 'packages/sqlite/src/maintenance.ts').unlink(), (p / 'packages/sqlite/src/maintenance.ts').symlink_to(template / 'packages/sqlite/src/maintenance.ts'))),
    ]:
        candidate = directory / name
        copy_owned_source(template, candidate)
        mutate(candidate)
        try:
            scope_workspace_adapter(candidate)
        except ValueError:
            pass
        else:
            raise AssertionError(f'Unsupported alias setup accepted: {name}')
    record = {'control': 'exact-declared-workspace-with-actual-resolver-and-restoration', 'passed': True,
              'negativeControls': ['undeclared-subpath', 'unsupported-export', 'undeclared-consumer', 'escaping-export', 'mismatched-resolution', 'deleted-export'],
              'adapter': adapters, 'unusedAliasEvidence': 'unused-alias/execution.json',
              'evidence': ['fixed/execution.json', 'changed/execution.json', 'restored/execution.json', 'undeclared-subpath/execution.json', 'mismatched-resolution/execution.json', 'deleted-export/execution.json']}
    write_json(directory / 'controls.json', record)
    return record


def historical_probe(output, candidate, dependencies, node):
    directory = output / ('historical-probe-' + candidate['subject'])
    directory.mkdir()
    template = directory / 'fixed-template'
    archive(Path(candidate['repository']), candidate['fix'], template)
    # This is the original Node-only adapter before the workspace correction.
    config = {'test': {'environment': 'node', 'include': candidate['testFiles'], 'pool': 'forks', 'maxWorkers': 1,
                       'fileParallelism': False, 'retry': 0, 'bail': 0, 'testTimeout': 30000, 'hookTimeout': 30000,
                       'reporters': [str(Path(__file__).with_name('vitest-reporter.ts'))]}}
    (template / 'research-test-value.config.ts').write_text('import{defineConfig}from"vite-plus/test/config";export default defineConfig(' + json.dumps(config) + ');')
    if candidate['subject'] == 'scope':
        before = execute_case(template, directory / 'before-workspace-adapter', COMMAND, dependencies=dependencies, node=node, timeout=90)
        require(not before['complete'] and any('@irudd-scope/sqlite' in x.get('message', '') for x in before.get('protocol', {}).get('failures', [])),
                'Existing historical test must reproduce the original missing workspace import')
    else:
        try:
            shutil.copytree(template, directory / 'before-source-copy')
        except shutil.Error as error:
            require('CLAUDE.md' in str(error), 'Existing archive must reproduce its source-copy failure')
            write_json(directory / 'before-source-copy-error.json', {'expectedFailure': str(error)})
        else:
            raise AssertionError('Existing archive before-copy unexpectedly succeeded')
    adapters = configure(template, candidate['testFiles'], Path(__file__).with_name('vitest-reporter.ts'))
    options = {'dependencies': dependencies, 'node': node, 'expected_node': node_fingerprint(node), 'timeout': 90,
               'import_controls': workspace_import_controls(adapters)}
    fixed = execute_case(template, directory / 'fixed-before', COMMAND, **options)
    if candidate['subject'] == 'scope' and not fixed['complete']:
        require(any('@effect/sql-sqlite-node' in x.get('message', '') for x in fixed.get('protocol', {}).get('failures', [])),
                'Corrected historical import must retain its separate missing external dependency')
        require(any(x.get('specifier') == '@irudd-scope/sqlite' and x.get('realPath', '').startswith(str(directory / 'fixed-before/source/'))
                    for x in fixed['importControls']), 'Historical alias correction must have actual fresh-source resolver evidence')
        evidence = {'candidate': candidate, 'workspaceAdapters': adapters, 'aliasCorrectionProven': True,
                    'verifiedValidationReplay': False, 'blockedBy': 'borrowed external store lacks historical @effect/sql-sqlite-node',
                    'timingUse': 'validation only; not a registered measurement',
                    'paths': ['before-workspace-adapter/execution.json', 'fixed-before/execution.json']}
        write_json(directory / 'controls.json', evidence)
        return evidence
    require(fixed['complete'] and fixed['status'] == 'survived', 'Corrected historical baseline must pass')
    faulty = directory / 'faulty-template'
    copy_owned_source(template, faulty)
    for relative in candidate['sourceFiles']:
        (faulty / relative).write_bytes(subprocess.check_output(['git', '-C', candidate['repository'], 'show', candidate['parent'] + ':' + relative]))
    regression = execute_case(faulty, directory / 'faulty', COMMAND, baseline=fixed['tests'], **options)
    restored = execute_case(template, directory / 'fixed-after', COMMAND, baseline=fixed['tests'], **options)
    killed = [k for k, v in regression['outcomes'].items() if v == 'killed']
    unaffected = [k for k, v in regression['outcomes'].items() if v == 'notKilled']
    require(regression['complete'] and killed and unaffected, 'Historical source reversion must produce an assertion regression and unaffected controls')
    require(restored['complete'] and restored['status'] == 'survived', 'Historical corrected source must pass again')
    evidence = {'candidate': candidate, 'workspaceAdapters': adapters, 'verifiedValidationReplay': True,
                'killedBy': killed, 'unaffected': unaffected, 'timingUse': 'validation only; not a registered measurement',
                'paths': ['before-workspace-adapter/execution.json', 'fixed-before/execution.json', 'faulty/execution.json', 'fixed-after/execution.json']}
    write_json(directory / 'controls.json', evidence)
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--dependency-output', type=Path)
    parser.add_argument('--installed', type=Path, default=Path('/Users/alun/repos/irudd-scope'))
    parser.add_argument('--node', default='/Users/alun/.local/share/vite-plus/js_runtime/node/26.10.0/bin/node')
    parser.add_argument('--candidate-manifest', type=Path)
    parser.add_argument('--candidate-id', default='scope-c46613230e97')
    parser.add_argument('--t3-dependency-output', type=Path)
    parser.add_argument('--t3-installed', type=Path, default=Path('/Users/alun/.t3/worktrees/t3code/t3code-674149e1'))
    parser.add_argument('--t3-node', default='/Users/alun/.local/share/vite-plus/js_runtime/node/24.21.0/bin/node')
    parser.add_argument('--source-candidate-id', default='t3code-6befe42eb096')
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    records = source_controls(output)
    if args.dependency_output:
        dependencies = load_dependencies(args.dependency_output.resolve(), args.installed)
        records.append(workspace_controls(output, dependencies, args.node))
        if args.candidate_manifest:
            manifest = read_json(args.candidate_manifest)
            rows = manifest['candidates'] if isinstance(manifest, dict) else manifest
            candidate = next(row for row in rows if row['id'] == args.candidate_id)
            records.append(historical_probe(output, candidate, dependencies, args.node))
        require(dependencies.current_digest() == dependencies.sha256, 'Validation must leave the shared owned dependency store unchanged')
    if args.t3_dependency_output:
        dependencies = load_dependencies(args.t3_dependency_output.resolve(), args.t3_installed, 't3code')
        manifest = read_json(args.candidate_manifest)
        rows = manifest['candidates'] if isinstance(manifest, dict) else manifest
        candidate = next(row for row in rows if row['id'] == args.source_candidate_id)
        records.append(historical_probe(output, candidate, dependencies, args.t3_node))
        require(dependencies.current_digest() == dependencies.sha256, 'Validation must preserve the T3 copied dependency store')
    write_json(output / 'controls.json', {'passed': True, 'controls': records, 'timingUse': 'validation only'})
    print(f'Passed {len(records)} setup control groups', flush=True)


if __name__ == '__main__':
    install_signal_handlers()
    main()
