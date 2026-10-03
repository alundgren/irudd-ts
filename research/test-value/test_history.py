import copy
from pathlib import Path
import tempfile
import unittest
from history import register_experiment

class HistoricalIdentityControls(unittest.TestCase):
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
