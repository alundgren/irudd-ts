import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("dryer_research_run", Path(__file__).with_name("run.py"))
run = importlib.util.module_from_spec(spec)
spec.loader.exec_module(run)

class TimeoutRetention(unittest.TestCase):
    def test_partial_output_and_timeout_remain_distinct(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            error = subprocess.TimeoutExpired(["example"], 180, output=b"started\n", stderr=b"partial error\n")
            record = run.preserve_timeout(out, "control", ["example"], Path("/tmp"), error, 180.1)
            self.assertEqual((out / "control.stdout").read_text(), "started\n")
            self.assertEqual((out / "control.stderr").read_text(), "partial error\n")
            self.assertEqual(record["execution_status"], "timeout")
            self.assertIsNone(record["exit_code"])
            self.assertEqual(record["cwd"], "/tmp")
            empty = subprocess.TimeoutExpired(["example"], 180)
            run.preserve_timeout(out, "empty", ["example"], Path("/tmp"), empty, 180)
            self.assertEqual((out / "empty.stdout").read_text(), "")
            self.assertEqual((out / "empty.stderr").read_text(), "")

    def test_final_checks_preserve_primary_timeout_and_report_checkout_failure(self):
        records = []
        error = subprocess.TimeoutExpired(["example"], 180, output=b"started")
        with patch.object(run, "checked_revision", side_effect=[None, ValueError("dirty source")]):
            with self.assertRaises(subprocess.TimeoutExpired) as caught:
                try:
                    raise error
                finally:
                    records = run.post_run_checks([(Path("/dryer"), "dryer-pin"), (Path("/t3"), "t3-pin")])
        self.assertIs(caught.exception, error)
        self.assertEqual([r["status"] for r in records], ["clean", "error"])
        self.assertEqual(records[1]["message"], "dirty source")

if __name__ == "__main__":
    unittest.main()
