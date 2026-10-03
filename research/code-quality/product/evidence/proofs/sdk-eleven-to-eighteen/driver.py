#!/usr/bin/env python3
"""Run serial Node public controls against exact byte edits in an owned copy."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tempfile
import threading
import uuid

ROOT = Path('/home/dev/.t3/worktrees/irudd-ts/t3code-dcd18ca5')
WORK = Path('/tmp/archguard-quality-product-cli-worktree')
BASE = ROOT / 'research/local/code-quality-product/new-sdk-public-control-proofs'
REPORT = ROOT / 'research/local/code-quality-product/self-protocol-strong-r10.json'
INSPECTION = Path('/tmp/archguard-quality-product-sdk-survivor-inspection.json')
DEADLINE_SECONDS = 15
OUTPUT_LIMIT = 65_536

def sha(data):
    return hashlib.sha256(data).hexdigest()

def git(*args):
    return subprocess.check_output(['git', *args], cwd=WORK)

def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')

def main():
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    evidence = BASE / ('run-' + stamp + '-' + uuid.uuid4().hex[:8])
    evidence.mkdir()
    shutil.copyfile(Path(__file__), evidence / 'driver.py')
    report_bytes = REPORT.read_bytes()
    report = json.loads(report_bytes)
    inspection_bytes = INSPECTION.read_bytes()
    inspection = json.loads(inspection_bytes)
    source = (WORK / 'sdk/mutator.ts').read_bytes()
    selected = next(f for f in report['selection']['selected'] if f['path'] == 'sdk/mutator.ts')
    assert sha(source) == selected['sha256'] == inspection['source']['sha256']
    assert len(source) == selected['bytes']
    old_tests = git('show', '64e42d7:tests/mutator_sdk.test.ts')
    new_tests = git('show', '5da4172:tests/mutator_sdk.test.ts')
    old_head = git('rev-parse', '64e42d7').decode().strip()
    new_head = git('rev-parse', '5da4172').decode().strip()
    assert len(re.findall(rb'^test\(', old_tests, re.M)) == 11
    assert len(re.findall(rb'^test\(', new_tests, re.M)) == 18
    node_launcher = shutil.which('node')
    # The installed launcher dispatches by filename; resolving it yields vp.
    node = Path(subprocess.check_output([node_launcher, '-p', 'process.execPath'],
                                       cwd=WORK, text=True, timeout=DEADLINE_SECONDS).strip()).resolve()
    node_version = subprocess.check_output([str(node), '--version'], text=True,
                                          timeout=DEADLINE_SECONDS).strip()
    assert node_version.startswith('v24.')
    inputs = evidence / 'inputs'
    inputs.mkdir()
    (inputs / 'original-sdk-mutator.ts').write_bytes(source)
    (inputs / 'original-eleven-tests.ts').write_bytes(old_tests)
    (inputs / 'corrected-eighteen-tests.ts').write_bytes(new_tests)
    (inputs / 'r10-report.json').write_bytes(report_bytes)
    (inputs / 'survivor-inspection.json').write_bytes(inspection_bytes)
    useful = [r for r in inspection['survivors'] if r['classification'] == 'usefulMissingTest']
    assert len(useful) == 11
    report_results = {r['mutationId']: r for r in report['results']}
    for site in useful:
        result = report_results[site['mutationId']]
        assert result['outcome'] == 'survived'
        assert site['location'] == result['location']
        assert site['expected'] == result['expected']
        assert site['replacement'] == result['replacement']
    owned = Path(tempfile.mkdtemp(prefix='archguard-sdk-public-proof-'))
    (owned / 'sdk').mkdir()
    (owned / 'tests').mkdir()
    (owned / 'tmp').mkdir()
    sdk_file = owned / 'sdk/mutator.ts'
    test_file = owned / 'tests/mutator_sdk.test.ts'
    environment = dict(os.environ)
    environment.pop('ARCHGUARD_MUTATION_REQUEST', None)
    environment.pop('ARCHGUARD_MUTATION_RESULT', None)
    environment.pop('NODE_OPTIONS', None)
    environment['TMPDIR'] = str(owned / 'tmp')
    environment['NO_COLOR'] = '1'
    executions = []

    def run(label, source_bytes, test_bytes, expected_exit):
        sdk_file.write_bytes(source_bytes)
        test_file.write_bytes(test_bytes)
        output = evidence / label
        output.mkdir()
        command = [str(node), '--test', '--test-reporter=tap', 'tests/mutator_sdk.test.ts']
        process = subprocess.Popen(command, cwd=owned, env=environment,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   start_new_session=True)
        buffers = {'stdout': bytearray(), 'stderr': bytearray()}
        totals = {'stdout': 0, 'stderr': 0}
        def drain(stream, name):
            while True:
                chunk = stream.read(8192)
                if not chunk:
                    break
                totals[name] += len(chunk)
                available = OUTPUT_LIMIT - len(buffers[name])
                if available > 0:
                    buffers[name].extend(chunk[:available])
            stream.close()
        threads = [threading.Thread(target=drain, args=(process.stdout, 'stdout')),
                   threading.Thread(target=drain, args=(process.stderr, 'stderr'))]
        for thread in threads:
            thread.start()
        timed_out = False
        try:
            exit_code = process.wait(timeout=DEADLINE_SECONDS)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(process.pid, signal.SIGKILL)
            exit_code = process.wait()
        # Close any inherited output handles held by an unexpected descendant.
        for thread in threads:
            thread.join(timeout=1)
        if any(thread.is_alive() for thread in threads):
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            for thread in threads:
                thread.join(timeout=1)
        assert not any(thread.is_alive() for thread in threads)
        record = dict(label=label, command=command, childDeadlineSeconds=DEADLINE_SECONDS,
                      sourceSha256=sha(source_bytes), testsSha256=sha(test_bytes),
                      expectedExitCode=expected_exit, exitCode=exit_code, timedOut=timed_out,
                      matchesExpected=exit_code == expected_exit and not timed_out,
                      output={})
        for name in ('stdout', 'stderr'):
            data = bytes(buffers[name])
            (output / (name + '.txt')).write_bytes(data)
            record['output'][name] = dict(path=str(output / (name + '.txt')),
                                          retainedBytes=len(data), totalBytes=totals[name],
                                          truncated=totals[name] > len(data), sha256=sha(data))
        tap = bytes(buffers['stdout']).decode('utf-8', errors='replace')
        record['failedTestLines'] = re.findall(r'^not ok .+$', tap, re.M)
        record['failureDetails'] = re.findall(r'^\s+(?:failureType|error|code|name):.*$', tap, re.M)
        record['tapSummary'] = re.findall(r'^# (?:tests|pass|fail|cancelled|skipped|todo) .+$', tap, re.M)
        write_json(output / 'result.json', record)
        executions.append(record)
        print(label + ': exit=' + str(exit_code) + ', expected=' + str(expected_exit), flush=True)
        return record

    proofs = []
    baselines = []
    failure = None
    try:
        baselines.append(run('baseline-old-eleven', source, old_tests, 0))
        baselines.append(run('baseline-new-eighteen', source, new_tests, 0))
        if not all(r['matchesExpected'] for r in baselines):
            raise RuntimeError('A baseline failed; preserve evidence and stop mutation commands.')
        for site in useful:
            start = site['location']['start']
            end = site['location']['end']
            assert source[start:end] == site['expected'].encode('utf-8')
            # Byte positions come from the original full-SHA source. No cumulative edits.
            mutant = source[:start] + site['replacement'].encode('utf-8') + source[end:]
            mid = site['mutationId']
            old = run(mid + '-old-eleven', mutant, old_tests, 0)
            new = run(mid + '-new-eighteen', mutant, new_tests, 1)
            proofs.append(dict(mutationId=mid, location=site['location'], operator=site['operator'],
                               expected=site['expected'], replacement=site['replacement'],
                               originalSourceSha256=sha(source), mutatedSourceSha256=sha(mutant),
                               old=old, new=new,
                               distinguishedByPublicControls=old['matchesExpected'] and new['matchesExpected'],
                               note='Direct Node test evidence only. A nonzero direct result does not relabel the conservative Vitest mutation outcome as an assertion kill.'))
            write_json(evidence / 'proofs-progress.json', proofs)
        sdk_file.write_bytes(source)
        baselines.append(run('restored-source-new-eighteen', source, new_tests, 0))
    except Exception as error:
        failure = repr(error)
        raise
    finally:
        shutil.rmtree(owned)
        source_unchanged = (WORK / 'sdk/mutator.ts').read_bytes() == source
        summary = dict(schemaVersion=1,sourceInspection='SDK public-control before/after correctness proof',
                       reportPath=str(REPORT),reportSha256=sha(report_bytes),
                       originalSdkSha256=sha(source),originalSdkBytes=len(source),
                       beforeTests=dict(head=old_head,sha256=sha(old_tests),count=11),
                       afterTests=dict(head=new_head,sha256=sha(new_tests),count=18),
                       node=dict(launcher=node_launcher,executable=str(node),version=node_version,sha256=sha(node.read_bytes())),
                       execution=dict(serial=True,childDeadlineSeconds=DEADLINE_SECONDS,
                                      outputLimitBytesPerStream=OUTPUT_LIMIT,
                                      ownedCopy=str(owned),ownedCopyRemoved=not owned.exists(),
                                      fixtureTemporariesConfinedToOwnedCopy=True,
                                      originalSdkUnchanged=source_unchanged),
                       baselines=baselines,proofs=proofs,error=failure,
                       allElevenDistinguished=len(proofs)==11 and all(p['distinguishedByPublicControls'] for p in proofs),
                       boundary='This is direct Node correctness evidence, not a new complete mutator replay or a timing comparison. Original R10 incompleteness, runtime errors and timeout remain unchanged.')
        write_json(evidence / 'summary.json', summary)
        print('Evidence: ' + str(evidence), flush=True)
        print('Owned copy removed: ' + str(not owned.exists()), flush=True)
    assert source_unchanged
    assert summary['allElevenDistinguished']
    assert all(r['matchesExpected'] for r in baselines)

if __name__ == '__main__':
    main()
