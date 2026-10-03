import json
from pathlib import Path
import tempfile
import unittest
from analyze import analyze
from report import collect
from test_analysis import matrix


class ReportEvidenceControls(unittest.TestCase):
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
