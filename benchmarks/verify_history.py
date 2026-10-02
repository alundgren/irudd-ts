#!/usr/bin/env python3
"""Verify the recorded upstream snippets and full-file hashes in a local clone."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("checkout", type=Path)
args = parser.parse_args()
evidence = json.loads((Path(__file__).resolve().parents[1] / "docs/historical-evidence.json").read_text())
sources = [source for case in evidence["cases"] for source in case["sources"]]
sources.extend(case["source"] for case in evidence["policyEvidence"])
for source in sources:
    text = subprocess.check_output(["git", "-C", str(args.checkout), "show", source["commit"] + ":" + source["path"]])
    assert hashlib.sha256(text).hexdigest() == source["sourceSha256"], source
    decoded = text.decode()
    assert source["verifiedText"] in decoded, source
    assert decoded[:decoded.index(source["verifiedText"])].count("\n") + 1 == source["line"], source
print(f"Verified {len(sources)} recorded source references")
