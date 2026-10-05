"""Publishing must fail before writing when the reviewed-source guards fail."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import publish as publisher


class PublishControls(unittest.TestCase):
    def invoke(self, *, dirty=False, revision="a" * 40, main="a" * 40,
               origin="https://github.com/alundgren/irudd-ts/"):
        calls = []

        def run(*args, **kwargs):
            calls.append(args)
            if args[:3] == ("git", "status", "--porcelain"):
                return " M website/recipes.py" if dirty else ""
            if args[:3] == ("git", "rev-parse", "HEAD"):
                return revision
            if args[0] == "gh":
                return main
            if args[:3] == ("git", "remote", "get-url"):
                return origin
            if args[:2] == ("git", "ls-remote"):
                return ""

        return calls, run

    def test_unreviewed_or_wrong_target_fails_before_build_or_push(self):
        for options, message in [({"dirty": True}, "local changes"),
                                 ({"main": "b" * 40}, "reviewed main"),
                                 ({"origin": "https://github.com/other/repo"}, "Unexpected origin")]:
            with self.subTest(options=options):
                calls, run = self.invoke(**options)
                with patch.object(publisher, "run", run):
                    with self.assertRaisesRegex(RuntimeError, message):
                        publisher.publish()
                self.assertFalse(any(call[0] == "python3" or call[:2] == ("git", "push") for call in calls))

    def test_reviewed_main_publishes_after_checks_without_force(self):
        calls, run = self.invoke()
        with tempfile.TemporaryDirectory() as directory:
            dist = Path(directory) / "dist"
            dist.mkdir()
            (dist / "index.html").write_text("Checked site")
            with patch.object(publisher, "run", run), patch.object(publisher, "DIST", dist):
                publisher.publish()
            self.assertTrue((dist / "deployment.json").exists())
        build = next(i for i, call in enumerate(calls) if call[0] == "python3")
        push = next(i for i, call in enumerate(calls) if call[:2] == ("git", "push"))
        self.assertLess(build, push)
        self.assertEqual(calls[push], ("git", "push", "origin", "HEAD:gh-pages"))


if __name__ == "__main__":
    unittest.main()
