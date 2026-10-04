import os
import re
from pathlib import Path
import subprocess
import tempfile
import unittest

from rust_slice import parse_libtest


class NativeRustControls(unittest.TestCase):
    def test_actual_assertion_failure_and_runtime_panic_remain_distinct(self):
        compiler = os.environ.get("RUSTC_FOR_TESTS")
        self.assertTrue(compiler, "Supply the pinned compiler through RUSTC_FOR_TESTS")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for mode in ["passing", "assertion", "runtime", "lookalike", "printed"]:
                source = root / "case.rs"
                source.write_text('''#[test] fn boundary() {
 BODY
}
#[test] fn unaffected() {
 assert_eq!(2 + 2, 4);
}
'''.replace("BODY", {"passing": "assert_eq!(1, 1);", "assertion": "assert_eq!(1, 2);", "runtime": 'panic!("unexpected runtime control");', "lookalike": 'panic!("assertion failed: runtime control");', "printed": 'println!("assertion failed: printed control"); panic!("unexpected runtime control");'}[mode]))
                binary = root / mode
                subprocess.run([compiler, "--test", str(source), "-o", str(binary)], check=True, capture_output=True)
                execution = subprocess.run([str(binary), "--test-threads=1"], capture_output=True, text=True)
                sites = {(str(source), number) for number, line in enumerate(source.read_text().splitlines(), 1) if re.search(r"^\s*assert(?:_eq|_ne)?!\s*\(", line)}
                result = parse_libtest(execution.stdout, execution.returncode, sites)
                self.assertEqual(result["status"], {"passing": "survived", "assertion": "killed", "runtime": "error", "lookalike":"error", "printed":"error"}[mode])
                self.assertEqual(len(result["tests"]), 2)
                self.assertEqual(result["tests"][1]["outcome"], "notKilled")
                if mode in {"runtime","lookalike","printed"}:
                    self.assertFalse(result["complete"])

    def test_missing_or_duplicate_or_truncated_runner_summary_is_incomplete(self):
        passing = "test a ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;"
        self.assertTrue(parse_libtest(passing, 0)["complete"])
        self.assertFalse(parse_libtest("test a ... ok\n", 0)["complete"])
        self.assertFalse(parse_libtest(passing.replace("1 passed", "2 passed"), 0)["complete"])
        self.assertFalse(parse_libtest(passing, 101)["complete"])
        self.assertFalse(parse_libtest(passing.replace("ok\ntest result", "ignored\ntest result"), 0)["complete"])
        with self.assertRaisesRegex(ValueError, "duplicate"):
            parse_libtest("test a ... ok\n" + passing, 0)


if __name__ == "__main__":
    unittest.main()
