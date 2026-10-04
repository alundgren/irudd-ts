import copy
from pathlib import Path
import tempfile
import unittest
import hashlib
import subprocess
from unittest.mock import patch
from history import register_experiment
from history import verify_experiment_identity
from history import ExecutionProfiles, HistoricalInstallations, execute_case, run_fault
from analyze import compact_evaluation, evaluate_fault, reconstruct_selections, summarize_faults
from runner import MAX_JSON, DependencyStore, hash_tree, read_json, write_json
from test_analysis import matrix

class HistoricalIdentityControls(unittest.TestCase):
    def test_build_profile_reaches_every_historical_execution_phase(self):
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);cli=root/'cli';cli.write_bytes(b'configured CLI')
            candidate={'id':'build','subject':'scope','repository':str(root),'fix':'a'*40,'parent':'b'*40,
                       'sourceFiles':['source.ts'],'testFiles':['case.test.ts'],'relatedGroup':'authored'}
            spec={'explicit':'one shared build declaration'}
            profiles=SimpleNamespace(apply_overlay=lambda *_:None,build_preparation=lambda _:spec,
                                     registration=lambda :{'declaredMap':{'build':spec}})
            dependencies=SimpleNamespace(sha256='c'*64,current_digest=lambda :'c'*64)
            tests=[{'id':'affected','name':'affected','file':'case.test.ts','state':'passed'},
                   {'id':'unaffected','name':'unaffected','file':'case.test.ts','state':'passed'}]
            calls=[]
            def archive(_repository,_revision,template):
                template.mkdir();(template/'source.ts').write_text('export const value=true;\n')
                (template/'case.test.ts').write_text('original test bytes\n')
            def git(arguments,**_):
                if 'rev-parse' in arguments: return candidate['parent' if arguments[-1].endswith('^') else 'fix']+'\n'
                return b'original test bytes\n' if arguments[-1].endswith('case.test.ts') else b'export const value=false;\n'
            def execute(template,output,_command,**options):
                calls.append((output.name,options))
                killed=bool(options.get('mutation')) or 'false' in (template/'source.ts').read_text()
                return {'complete':True,'status':'killed' if killed else 'survived','tests':tests,
                        'outcomes':{'affected':'killed' if killed else 'notKilled','unaffected':'notKilled'},'infrastructureErrors':[]}
            runtime={'path':'/explicit/node'}
            with patch('history.archive',side_effect=archive),patch('history.configure',return_value=[]),\
                 patch('history.subprocess.check_output',side_effect=git),patch('history.subprocess.run',return_value=SimpleNamespace(returncode=0)),\
                 patch('history.execute_case',side_effect=execute),patch('history.plan',return_value={'sites':[{'id':'mutation'}]}):
                result=run_fault(candidate,dependencies,cli,root/'fault',None,24,'/explicit/node',runtime,lambda :None,execution_profiles=profiles)
            self.assertTrue(result['verified'])
            self.assertEqual([name for name,_ in calls],['fixed-before','faulty','fixed-after','parent-tests-fixed-source','mutant-0000'])
            self.assertTrue(all(options['build_preparation'] is spec and options['expected_node'] is runtime for _,options in calls))

    def test_original_archive_validation_precedes_overlay_and_execution(self):
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            candidate={'id':'overlay','repository':str(root),'fix':'a'*40,'parent':'b'*40}
            profiles=SimpleNamespace(apply_overlay=lambda *_: self.fail('overlay cannot precede archive validation'))
            with patch('history.archive'),patch('history.subprocess.check_output',side_effect=['a'*40+'\n','b'*40+'\n']),\
                 patch('history.configure') as configured,patch('history.execute_case') as executed:
                def mismatch(_): raise ValueError('original archive mismatch')
                result=run_fault(candidate,None,None,root/'fault',None,24,None,None,None,
                                 verify_source=mismatch,execution_profiles=profiles)
            self.assertFalse(result['verified']);self.assertFalse(result['executionAttempted'])
            configured.assert_not_called();executed.assert_not_called()

    def test_profiles_revalidate_before_and_after_each_case(self):
        observations=[]
        def verify(): observations.append('verify')
        result={'cleanupComplete':True,'cleanupErrors':[]}
        with patch('history.raw_execute_case',side_effect=lambda *args,**kwargs: observations.append('case') or result):
            self.assertIs(execute_case('source','output',[],verify_execution=verify),result)
        self.assertEqual(observations,['verify','case','verify'])
        observations.clear()
        with patch('history.raw_execute_case',side_effect=ValueError('rejected')):
            with self.assertRaisesRegex(ValueError,'rejected'):
                execute_case('source','output',[],verify_execution=verify)
        self.assertEqual(observations,['verify','verify'])

    def test_prepared_stores_reuse_without_copy_or_repair(self):
        from controls_installations import receipt
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); donors=root/'donors';donors.mkdir()
            row=receipt(donors,'first',1)
            inputs=read_json(row['receiptPath'])
            candidate={'id':'one','subject':'t3code','installationInputSignature':inputs['inputSignature'],
                       'sourceArchiveTreeSha256':'a'*64,'sourceArchiveAlgorithm':'runner-hash-tree-v1'}
            mapping=root/'map.json'
            write_json(mapping,{'schemaVersion':1,'installations':[row],'candidates':{'one':'first'}})
            prepared=root/'prepared'
            original=HistoricalInstallations(mapping,[candidate],prepared)
            before=hash_tree(prepared)[0]
            with patch.object(DependencyStore,'copy',side_effect=AssertionError('reuse must never copy')):
                reused=HistoricalInstallations(mapping,[candidate],root/'new-profile',prepared_output=prepared)
                self.assertEqual(reused.dependency(candidate).sha256,original.dependency(candidate).sha256)
                self.assertEqual(reused.registration()['preparedRoot'],str(prepared.resolve()))
                self.assertEqual(hash_tree(prepared)[0],before)
                reused.verify(candidate)
                metadata=next((prepared/'installation-metadata').glob('*.json'))
                raw=metadata.read_bytes();metadata.write_bytes(raw+b'\n')
                with self.assertRaisesRegex(RuntimeError,'metadata changed'): reused.verify(candidate)
                metadata.write_bytes(raw)
                with self.assertRaisesRegex(ValueError,'existing metadata and store'):
                    HistoricalInstallations(mapping,[candidate],root/'new-profile',prepared_output=root/'absent')
                file=reused.dependency(candidate).root/'node_modules/control-pkg/index.mjs'
                data=file.read_bytes();file.write_bytes(b'changed')
                with self.assertRaisesRegex(ValueError,'store has changed'):
                    HistoricalInstallations(mapping,[candidate],root/'new-profile',prepared_output=prepared)
                file.write_bytes(data)
                self.assertEqual(hash_tree(prepared)[0],before)

    def test_canonical_fixture_overlay_binding_and_negative_controls(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);repository=root/'repository';repository.mkdir()
            def git(*arguments):
                return subprocess.check_output(['git','-C',str(repository),'-c','core.hooksPath=/dev/null',
                    '-c','commit.gpgsign=false','-c','user.name=Research control','-c','user.email=research@example.invalid',*arguments],text=True).strip()
            git('init','-q')
            (repository/'source.ts').write_text('export const value=false;\n')
            (repository/'case.test.ts').write_text('unchanged test bytes\n')
            git('add','.');git('commit','-qm','parent');parent=git('rev-parse','HEAD')
            (repository/'source.ts').write_text('export const value=true;\n')
            git('add','.');git('commit','-qm','fixed');fix=git('rev-parse','HEAD')
            relative='testkit/fixtures/kiro_unknown_model/kiro_transcript.ndjson'
            fixture=repository/relative;fixture.parent.mkdir(parents=True);fixture.write_text('{"event":"canonical"}\n')
            git('add','.');git('commit','-qm','canonical fixture');canonical=git('rev-parse','HEAD')
            candidate={'id':'kiro','repository':str(repository),'fix':fix,'parent':parent,
                       'sourceFiles':['source.ts'],'testFiles':['case.test.ts']}
            bytes_path=root/'canonical.ndjson';bytes_path.write_bytes(fixture.read_bytes())
            overlay={'path':relative,'bytesPath':str(bytes_path),'sha256':hashlib.sha256(bytes_path.read_bytes()).hexdigest(),
                     'sourceRevision':canonical,'gitBlob':git('rev-parse',canonical+':'+relative)}
            path=root/'profile.json'
            def manifest(value): write_json(path,{'schemaVersion':1,'candidates':{'kiro':{'fixtureOverlay':value}}})
            manifest(overlay);profiles=ExecutionProfiles(path,[candidate])
            template=root/'archive';template.mkdir()
            before=hash_tree(template)[0]
            evidence=profiles.apply_overlay(candidate,template)
            self.assertEqual((template/relative).read_bytes(),bytes_path.read_bytes())
            self.assertNotEqual(evidence['sourceAfterOverlaySha256'],before)
            with self.assertRaisesRegex(ValueError,'replace an existing'): profiles.apply_overlay(candidate,template)
            raw=bytes_path.read_bytes();bytes_path.write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError,'bytes changed'): profiles.verify()
            bytes_path.write_bytes(raw)
            for changed,error in [({**overlay,'gitBlob':'0'*40},'Git object'),
                                  ({**overlay,'sourceRevision':parent},'descendant'),
                                  ({**overlay,'path':'case.test.ts'},'non-code fixture')]:
                manifest(changed)
                with self.subTest(error=error),self.assertRaisesRegex(ValueError,error): ExecutionProfiles(path,[candidate])
            (repository/'case.test.ts').write_text('changed assertions\n');git('add','.');git('commit','-qm','changed tests')
            manifest({**overlay,'sourceRevision':git('rev-parse','HEAD')})
            with self.assertRaisesRegex(ValueError,'changes a selected test'): ExecutionProfiles(path,[candidate])
            manifest(overlay);profiles=ExecutionProfiles(path,[candidate])
            write_json(path,{'schemaVersion':1,'candidates':{}})
            with self.assertRaisesRegex(RuntimeError,'profile changed'): profiles.verify()

    def test_desktop_preflight_exclusion_never_archives_or_executes(self):
        with tempfile.TemporaryDirectory() as directory:
            candidate={'id':'desktop','preflightExclusion':'requires desktop integration harness'}
            with patch('history.archive') as archived, patch('history.execute_case') as executed:
                result=run_fault(candidate,None,None,Path(directory)/'excluded',None,24,None,None,None)
                self.assertFalse(result['verified']);self.assertFalse(result['executionAttempted'])
                archived.assert_not_called();executed.assert_not_called()
                for invalid in ['',True,10]:
                    with self.subTest(invalid=invalid),self.assertRaisesRegex(ValueError,'preflight exclusion'):
                        run_fault({**candidate,'preflightExclusion':invalid},None,None,Path(directory)/('invalid-'+str(invalid)),None,24,None,None,None)

    def test_registered_runtime_and_implementation_cannot_be_replaced(self):
        with tempfile.TemporaryDirectory() as directory:
            cli=Path(directory)/'cli';cli.write_bytes(b'fixed CLI bytes')
            node={'path':'/explicit/node','version':'v26.10.0','sha256':'a'*64}
            registration={'runnerSha256':'b'*64,'sdkSha256':'c'*64,
                          'archguardSha256':hashlib.sha256(cli.read_bytes()).hexdigest(),'node':{'scope':node}}
            with patch('history.runner_fingerprint',return_value='b'*64), patch('history.hash_tree',return_value=('c'*64,[])), patch('history.node_fingerprint',return_value=node) as runtime:
                verify_experiment_identity(registration,cli,{'scope':Path('/explicit/node')})
                cli.write_bytes(b'replaced CLI bytes')
                with self.assertRaisesRegex(RuntimeError,'identity|changed'):
                    verify_experiment_identity(registration,cli,{'scope':Path('/explicit/node')})
                cli.write_bytes(b'fixed CLI bytes')
                runtime.return_value={**node,'sha256':'d'*64}
                with self.assertRaisesRegex(RuntimeError,'identity|changed'):
                    verify_experiment_identity(registration,cli,{'scope':Path('/explicit/node')})
                runtime.return_value=node
                verify_experiment_identity(registration,cli,{'scope':Path('/explicit/node')})
                with patch('history.hash_tree',return_value=('d'*64,[])), self.assertRaisesRegex(RuntimeError,'identity|changed'):
                    verify_experiment_identity(registration,cli,{'scope':Path('/explicit/node')})

    def test_long_test_ids_fit_budget_without_changing_selections_or_summary(self):
        data=matrix()
        ids={test['id']:test['id']+' '+('parameterized test context '*50) for test in data['tests']}
        for test in data['tests']:
            test['id']=ids[test['id']]
        for mutant in data['mutants']:
            mutant['outcomes']={ids[key]:value for key,value in mutant['outcomes'].items()}
        fault={'id':'long-identities','verified':True,'killedBy':[ids['D']]}
        full=evaluate_fault(data,fault)
        compact=compact_evaluation(full,data)
        with tempfile.TemporaryDirectory() as directory:
            full_path=Path(directory)/'full.json';small_path=Path(directory)/'compact.json'
            write_json(full_path,full);write_json(small_path,compact)
            self.assertGreater(full_path.stat().st_size,MAX_JSON)
            with self.assertRaisesRegex(ValueError,'byte budget'):
                read_json(full_path)
            self.assertLess(small_path.stat().st_size,MAX_JSON)
            retained=read_json(small_path)
            self.assertEqual(reconstruct_selections(retained,data),full)
            self.assertEqual(summarize_faults([retained],100),summarize_faults([full],100))
        for key in ['selectedCount','selectedSha256','seed','budget','strategy']:
            changed=copy.deepcopy(compact)
            trial=changed['trials'][0]
            trial[key]=({'selectedCount':0,'selectedSha256':'0'*64,'seed':999,'budget':4,'strategy':'raw'})[key]
            with self.subTest(key=key),self.assertRaisesRegex(ValueError,'selection evidence'):
                reconstruct_selections(changed,data)
        changed=copy.deepcopy(data);changed['tests'][0]['name']='different'
        with self.assertRaisesRegex(ValueError,'identity changed'):
            reconstruct_selections(compact,changed)

    def test_changed_protocol_cannot_relabel_retained_results(self):
        with tempfile.TemporaryDirectory() as directory:
            output=Path(directory)
            baseline={'mutantLimit':50,'operators':['comparison'],'candidate':{'fix':'a'*40,'tests':['a.test.ts']},'dependencySha256':'b'*64,'runnerSha256':'c'*64}
            register_experiment(output,baseline)
            original=(output/'preregistration.json').read_bytes()
            register_experiment(output,baseline)
            for field in ['mutantLimit','operators','candidate','dependencySha256','runnerSha256']:
                changed=copy.deepcopy(baseline);changed[field]='changed'
                with self.subTest(field=field),self.assertRaisesRegex(ValueError,'identity changed'):
                    register_experiment(output,changed)
                self.assertEqual((output/'preregistration.json').read_bytes(),original)
            fresh=output/'corrected';fresh.mkdir()
            register_experiment(fresh,changed)
            self.assertNotEqual((fresh/'preregistration.json').read_bytes(),original)
