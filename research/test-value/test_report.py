import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from analyze import analyze, compact_evaluation, evaluate_fault
from report import collect
from test_analysis import matrix


class ReportEvidenceControls(unittest.TestCase):
    def test_reporting_profile_does_not_choose_each_faults_best_attempt(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);history=root/'history';old=history/'fault';old.mkdir(parents=True)
            data=matrix();fault={'id':'fault','verified':True,'killedBy':['A'],'relatedGroup':'authored-control'}
            (history/'attempts.json').write_text(json.dumps([fault]))
            (old/'matrix.json').write_text(json.dumps(data));(old/'analysis.json').write_text(json.dumps(analyze(data)))
            (old/'evaluation.json').write_text(json.dumps(compact_evaluation(evaluate_fault(data,fault,seeds=1),data)))
            repaired=root/'repaired';repaired.mkdir()
            newer={**fault,'verified':False,'exclusion':'authored fixed baseline failure'}
            (repaired/'attempts.json').write_text(json.dumps([newer]))
            result=collect(root,history_root=repaired)
            self.assertEqual(result['aggregate']['faults'],0)
            self.assertEqual(len(result['history']),1)
            self.assertFalse(result['history'][0]['verified'])
            self.assertTrue(result['archivedHistory'][0]['verified'])
            self.assertEqual(collect(root)['aggregate']['faults'],1)
            with self.assertRaisesRegex(ValueError,'no attempts ledger'):
                collect(root,history_root=root/'missing')

    def test_stale_analysis_cannot_describe_changed_matrix_as_complete(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);subject=root/'current'/'slice';subject.mkdir(parents=True)
            data=matrix();matrix_path=subject/'matrix.json';analysis_path=subject/'analysis.json'
            matrix_path.write_text(json.dumps(data));analysis_path.write_text(json.dumps(analyze(data)))
            original=analysis_path.read_bytes()
            self.assertTrue(collect(root)['subjects'][0]['analysis']['complete'])
            data['mutants'][0]['outcomes']['B']='notKilled'
            data['mutants'][0]['outcomes']['C']='killed'
            matrix_path.write_text(json.dumps(data))
            with self.assertRaisesRegex(ValueError,'does not match matrix'):
                collect(root)
            self.assertEqual(analysis_path.read_bytes(),original)
            analysis_path.write_text(json.dumps(analyze(data)))
            result=collect(root)['subjects'][0]
            self.assertTrue(result['analysis']['complete'])
            self.assertEqual(result['matrix'],data)

    def test_missing_analysis_remains_incomplete(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);subject=root/'current'/'slice';subject.mkdir(parents=True)
            (subject/'matrix.json').write_text(json.dumps(matrix()))
            self.assertFalse(collect(root)['subjects'][0]['analysis']['complete'])

    def test_archived_subject_matrix_cannot_enter_the_report_as_a_measurement(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);subject=root/'current'/'slice';subject.mkdir(parents=True)
            data=matrix();(subject/'matrix.json').write_text(json.dumps(data))
            (subject/'analysis.json').write_text(json.dumps(analyze(data)))
            archived=subject/'template'/'research'/'old-results';archived.mkdir(parents=True)
            (archived/'matrix.json').write_text(json.dumps(data))
            (archived/'analysis.json').write_text(json.dumps(analyze(data)))
            self.assertEqual([s['id'] for s in collect(root)['subjects']],['slice'])

    def test_secondary_results_must_bind_to_retained_primary_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)/'primary';primary=root/'current'/'slice';primary.mkdir(parents=True)
            data=matrix();path=primary/'matrix.json';path.write_text(json.dumps(data))
            (primary/'analysis.json').write_text(json.dumps(analyze(data)))
            secondary_root=Path(directory)/'secondary';secondary=secondary_root/'slice';secondary.mkdir(parents=True)
            derived={**data,'provenance':{'primaryMatrixSha256':'0'*64,'sensitivityPolicy':'posthoc-test-failures-v1'}}
            (secondary/'matrix.json').write_text(json.dumps(derived))
            (secondary/'analysis.json').write_text(json.dumps(analyze(derived)))
            with self.assertRaisesRegex(ValueError,'does not match retained primary'):collect(root,secondary_root)
            derived['provenance']['primaryMatrixSha256']=hashlib.sha256(path.read_bytes()).hexdigest()
            (secondary/'matrix.json').write_text(json.dumps(derived))
            (secondary/'analysis.json').write_text(json.dumps(analyze(derived)))
            with self.assertRaisesRegex(ValueError,'lacks valid failure categories'):collect(root,secondary_root)
            for mutant in derived['mutants']:
                if mutant['status']=='killed':
                    mutant['failureCategories']={'assertionTests':[test for test,outcome in mutant['outcomes'].items() if outcome=='killed'],'otherFailureTests':[]}
            (secondary/'matrix.json').write_text(json.dumps(derived))
            (secondary/'analysis.json').write_text(json.dumps(analyze(derived)))
            result=collect(root,secondary_root)
            self.assertEqual(len(result['subjects']),1)
            self.assertEqual(len(result['secondarySubjects']),1)

    def test_stale_historical_evaluation_cannot_enter_aggregate(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);history=root/'history';subject=history/'fault';subject.mkdir(parents=True)
            data=matrix();fault={'id':'fault','verified':True,'killedBy':['A'],'relatedGroup':'authored-control'}
            (history/'attempts.json').write_text(json.dumps([fault]))
            (subject/'matrix.json').write_text(json.dumps(data));(subject/'analysis.json').write_text(json.dumps(analyze(data)))
            (subject/'evaluation.json').write_text(json.dumps(compact_evaluation(evaluate_fault(data,fault,seeds=1),data)))
            self.assertEqual(collect(root)['aggregate']['faults'],1)
            wrong=compact_evaluation(evaluate_fault(data,fault,seeds=1),data)
            wrong['fault']={**wrong['fault'],'verified':False}
            (subject/'evaluation.json').write_text(json.dumps(wrong))
            with self.assertRaisesRegex(ValueError,'different fault'):collect(root)
            data['mutants'][0]['outcomes']['C']='killed'
            (subject/'matrix.json').write_text(json.dumps(data));(subject/'analysis.json').write_text(json.dumps(analyze(data)))
            with self.assertRaisesRegex(ValueError,'does not match a complete matrix'):collect(root)
            (subject/'evaluation.json').write_text(json.dumps(compact_evaluation(evaluate_fault(data,fault,seeds=1),data)))
            self.assertEqual(collect(root)['aggregate']['faults'],1)
