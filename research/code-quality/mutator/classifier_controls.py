#!/usr/bin/env python3
"""Real Vitest mixed-error and assertion-only classification controls."""
import argparse
import dataclasses
import json
from pathlib import Path
from evaluate import setup, RecordingRunner, vitest_status, write_json


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--t3', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists(): raise ValueError('use a new directory')
    setup(args.output, 'corrected', args.t3)
    root = args.output.resolve()
    assertion = root/'src/assertion.test.ts'
    broken = root/'src/broken.test.ts'
    runtime = root/'src/runtime.test.ts'
    command = './node_modules/.bin/vitest run --reporter=json --reporter=./execution-reporter.js'
    runner = RecordingRunner()
    results = {}
    def capture(label):
        result = runner.run(command, root, 20)
        results[label] = dataclasses.asdict(result) | {'status':vitest_status(result)}
    capture('passing')
    assertion.write_text("import {expect,it} from 'vitest'; it('assertion',()=>expect(1).toBe(2));\n")
    capture('assertion-only')
    broken.write_text("import './missing-module';\n")
    capture('assertion-and-import')
    broken.unlink()
    runtime.write_text("import {it} from 'vitest'; it('runtime',()=>{ throw new Error('AssertionError: adapter initialization failed'); });\n")
    capture('assertion-and-runtime')
    runtime.write_text("import {it} from 'vitest'; it('unhandled',()=>{ Promise.reject(new Error('unhandled control')); });\n")
    capture('assertion-and-unhandled')
    runtime.write_text("import {describe,it,afterAll} from 'vitest'; describe('hook',()=>{ it('pass',()=>{}); afterAll(()=>{ throw new Error('teardown control'); }); });\n")
    capture('assertion-and-teardown')
    write_json(root/'classifier-controls.json',results)
    assert results['passing']['status']=='survived'
    assert results['assertion-only']['status']=='killed'
    for key in ['assertion-and-import','assertion-and-runtime','assertion-and-unhandled','assertion-and-teardown']:
        assert results[key]['status']=='execution-error', key
    print('Passing, assertion-only and all mixed-error controls verified')


if __name__ == '__main__': main()
