import contextlib
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from history import configure
from runner import DependencyStore, HERE, digest, direct_vitest_command, execute_case, node_fingerprint, read_json, write_json
from sensitivity import bind_column, derive, native_outcomes, vitest_outcomes


def control_directory(name):
    retained=os.environ.get('TEST_VALUE_CONTROLS_OUTPUT')
    if not retained:
        return tempfile.TemporaryDirectory()
    directory=Path(retained)/name
    directory.mkdir(parents=True,exist_ok=False)
    return contextlib.nullcontext(str(directory))


class SensitivityControls(unittest.TestCase):
    @unittest.skipUnless(os.environ.get('TEST_VALUE_EVIDENCE') and os.environ.get('TEST_VALUE_NODE'), 'Optional copied Vitest installation not supplied')
    def test_actual_vitest_body_exception_vs_hook_and_runtime_identity(self):
        root = Path(os.environ['TEST_VALUE_EVIDENCE'])
        metadata = read_json(root/'history/t3code-dependencies.json')
        store = DependencyStore(metadata['installedRoot'],root/'history/t3code-dependencies')
        store.modules=metadata['modules'];store.workspace_links=metadata['workspaceLinks']
        store.owned_directories=set(metadata['ownedDirectories']);store.sha256=metadata['sha256']
        node = os.environ['TEST_VALUE_NODE']
        frozen = node_fingerprint(node)
        with control_directory('vitest') as directory:
            output=Path(directory)
            template=output/'template';template.mkdir()
            # This copied dependency view expects its workspace source directories.
            for relative in store.workspace_links.values():
                destination=template/relative;destination.mkdir(parents=True,exist_ok=True)
                (destination/'research-owned-marker').write_text('owned test control')
            (template/'package.json').write_text('{"type":"module"}')
            source=template/'source.ts'
            passing='export const boundary=(n:number)=>n<2?{value:true}:null;\n'
            source.write_text(passing)
            tests='import {test,expect,beforeEach} from "vite-plus/test";import {boundary} from "./source.ts";test("boundary",()=>expect(boundary(1)!.value).toBe(true));test("unaffected",()=>expect(2+2).toBe(4));\n'
            (template/'case.test.ts').write_text(tests)
            configure(template,['case.test.ts'],HERE/'vitest-reporter.ts')
            options=dict(dependencies=store,node=node,expected_node=frozen)
            fixed=execute_case(template,output/'baseline',direct_vitest_command('research-test-value.config.ts'),**options)
            self.assertTrue(fixed['complete'],fixed['infrastructureErrors'])
            start=passing.index('<')
            mid=digest(b'authored boundary mutation control')
            mutation={'id':mid,'location':{'file':'source.ts','start':start,'end':start+1},
                      'sourceSha256':digest(passing.encode()),'expected':'<','replacement':'>'}
            faulty=execute_case(template,output/'faulty',direct_vitest_command('research-test-value.config.ts'),baseline=fixed['tests'],mutation=mutation,**options)
            self.assertFalse(faulty['complete'])
            self.assertTrue(faulty['protocol']['complete'])
            outcomes,categories=vitest_outcomes(output/'faulty',fixed['tests'],frozen)
            self.assertEqual(sorted(outcomes.values()),['killed','notKilled'])
            self.assertEqual(len(categories['otherFailureTests']),1)
            self.assertEqual(categories['assertionTests'],[])
            matrix={'schemaVersion':1,'baselineComplete':True,'subject':{'name':'authored exception control'},'tests':fixed['tests'],
                    'provenance':{'node':frozen,'sourceSha256':fixed['templateSha256'],'dependencySha256':store.sha256,'runnerSha256':fixed['runnerSha256']},'mutants':[{'id':mid,'mutation':mutation,'status':faulty['status'],
                        'outcomes':faulty['outcomes'],'infrastructureErrors':faulty['infrastructureErrors'],'evidence':'faulty/execution.json'}]}
            write_json(output/'matrix.json',matrix)
            original_bytes=(output/'matrix.json').read_bytes()
            derived=derive(output,output/'derived')
            analysis=read_json(output/'derived/analysis.json')
            self.assertEqual(analysis['summary']['acceptedMutants'],1)
            self.assertEqual(analysis['summary']['subsumingRequirements'],1)
            self.assertEqual(derived['mutants'][0]['primaryInfrastructureErrors'],faulty['infrastructureErrors'])
            self.assertEqual((output/'matrix.json').read_bytes(),original_bytes)
            changed_baseline={**fixed,'request':{**fixed['request'],'inputDigest':'0'*64}}
            with self.assertRaisesRegex(ValueError,'digest differs'):
                bind_column(output/'faulty',matrix['mutants'][0],matrix,changed_baseline)
            source.write_text(passing)
            restored=execute_case(template,output/'restored',direct_vitest_command('research-test-value.config.ts'),baseline=fixed['tests'],**options)
            self.assertTrue(restored['complete'])
            self.assertEqual(set(vitest_outcomes(output/'restored',fixed['tests'],frozen)[0].values()),{'notKilled'})
            wrong={**matrix,'mutants':[{**matrix['mutants'][0],'evidence':'restored/execution.json'}]}
            write_json(output/'matrix.json',wrong)
            rejected=derive(output,output/'mismatched-derived')
            self.assertEqual(read_json(output/'mismatched-derived/analysis.json')['summary']['acceptedMutants'],0)
            self.assertIn('does not match column',rejected['mutants'][0]['sensitivityExclusion'])
            (output/'matrix.json').write_bytes(original_bytes)
            (template/'case.test.ts').write_text(tests+'beforeEach(()=>{throw new Error("owned failing hook");});\n')
            hook=execute_case(template,output/'hook',direct_vitest_command('research-test-value.config.ts'),baseline=fixed['tests'],**options)
            self.assertTrue(any(f['kind']=='hook' for f in hook['protocol']['failures']))
            with self.assertRaisesRegex(ValueError,'global, hook'):
                vitest_outcomes(output/'hook',fixed['tests'],frozen)
            with self.assertRaisesRegex(ValueError,'frozen runtime'):
                vitest_outcomes(output/'faulty',fixed['tests'],{**frozen,'sha256':'0'*64})
            original=read_json(output/'faulty/execution.json')
            for changes in [{'cleanupComplete':False},{'vitest':None},{'residualProcessGroup':True}]:
                write_json(output/'faulty/execution.json',{**original,**changes})
                with self.assertRaises(ValueError):vitest_outcomes(output/'faulty',fixed['tests'],frozen)
            write_json(output/'faulty/execution.json',original)

    def test_actual_result_unwrap_and_arbitrary_panic_remain_distinct(self):
        compiler=os.environ['RUSTC_FOR_TESTS']
        with control_directory('native') as directory:
            root=Path(directory);source=root/'case.rs'
            baseline=[{'id':'rust-lib::boundary'},{'id':'rust-lib::unaffected'}]
            for mode,body in [('unwrap','let x:Result<(),&str>=Err("owned failure"); x.unwrap();'),('panic','panic!("called `Result::unwrap()` on an `Err` value: lookalike");')]:
                source.write_text('#[test] fn boundary() {\n '+body+'\n}\n#[test] fn unaffected(){assert_eq!(1,1);}\n')
                binary=root/mode
                subprocess.run([compiler,'--test',str(source),'-o',str(binary)],capture_output=True,check=True)
                raw=subprocess.run([str(binary),'--test-threads=1'],capture_output=True,text=True)
                evidence=root/(mode+'-events');evidence.mkdir()
                (evidence/'stdout.txt').write_text(raw.stdout)
                write_json(evidence/'execution.json',{'process':{'status':'finished','exitCode':raw.returncode,'cleanupComplete':True,'cleanupErrors':[],'residualProcessGroup':False}})
                unwraps={(str(source),2)} if mode=='unwrap' else set()
                if mode=='unwrap':
                    outcomes,categories=native_outcomes(evidence,baseline,set(),unwraps)
                    self.assertEqual(sorted(outcomes.values()),['killed','notKilled'])
                    self.assertEqual(categories['otherFailureTests'],['rust-lib::boundary'])
                    (evidence/'stdout.txt').write_text(raw.stdout.replace('2 failed','3 failed')+'\ntest result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out;')
                    with self.assertRaisesRegex(ValueError,'duplicated'):native_outcomes(evidence,baseline,set(),unwraps)
                else:
                    with self.assertRaisesRegex(ValueError,'not a frozen'):native_outcomes(evidence,baseline,set(),unwraps)


if __name__=='__main__':unittest.main()
