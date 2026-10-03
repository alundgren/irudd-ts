"""The narrow Vitest observer must keep mixed execution failures out of kills."""
import json
import unittest
from pathlib import Path
from types import SimpleNamespace
from evaluate import vitest_status


class VitestStatusTests(unittest.TestCase):
    def test_real_control_failures_and_corrections(self):
        controls = json.loads((Path(__file__).parent/'evidence/classifier-controls.json').read_text())
        for name, control in controls.items():
            with self.subTest(name=name):
                result = SimpleNamespace(**{k:v for k,v in control.items() if k!='status'})
                self.assertEqual(vitest_status(result),control['status'])

    def test_missing_execution_record_is_incomplete(self):
        self.assertEqual(vitest_status(SimpleNamespace(timed_out=False,code=1,output='{}')), 'execution-error')

    def test_timeout_stays_distinct(self):
        self.assertEqual(vitest_status(SimpleNamespace(timed_out=True,code=124,output='')), 'timeout')


if __name__ == '__main__': unittest.main()
