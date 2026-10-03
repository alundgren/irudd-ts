import copy
from pathlib import Path
import tempfile
import unittest
import hashlib
from unittest.mock import patch
from history import register_experiment
from history import verify_experiment_identity
from history import run_fault
from analyze import compact_evaluation, evaluate_fault, reconstruct_selections, summarize_faults
from runner import MAX_JSON, read_json, write_json
from test_analysis import matrix

class HistoricalIdentityControls(unittest.TestCase):
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
