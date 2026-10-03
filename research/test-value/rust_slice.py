#!/usr/bin/env python3
"""Explicit native Rust slice using cargo-mutants plans and assertion-only libtest evidence."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import sys

from runner import disk_check, hash_tree, install_signal_handlers, runner_fingerprint, run_command, write_json


def parse_libtest(text, returncode, assertion_sites=()):
    records = []
    seen = set()
    for name, state in re.findall(r"^test (\S+) \.\.\. (ok|FAILED|ignored)$", text, re.M):
        if name in seen:
            raise ValueError("duplicate libtest identity")
        seen.add(name)
        outcome = "notKilled" if state == "ok" else "unknown"
        if state == "FAILED":
            block = re.search(r"^---- " + re.escape(name) + r" stdout ----\n(.*?)(?=^---- |^failures:|^test result:|\Z)", text, re.M | re.S)
            if block:
                panics = re.findall(r"^thread .* panicked at (.+):(\d+):(\d+):$", block.group(1), re.M)
                site = (panics[0][0], int(panics[0][1])) if len(panics) == 1 else None
                if site in assertion_sites and re.search(r"(?:assertion (?:`.*?` )?failed|assertion failed:)", block.group(1)):
                    outcome = "killed"
        records.append({"id": "rust-lib::" + name, "name": name, "file": None, "outcome": outcome})
    summary = re.search(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;", text, re.M)
    complete = bool(summary) and bool(records) and all(test["outcome"] != "unknown" for test in records)
    if summary:
        complete = complete and len(records) == sum(int(summary.group(i)) for i in [2, 3, 4])
        complete = complete and sum(test["outcome"] == "killed" for test in records) == int(summary.group(3))
        complete = complete and ((returncode == 0) == (summary.group(1) == "ok"))
        complete = complete and returncode in {0, 101}
    return {"complete": complete, "tests": records,
            "status": "killed" if complete and any(test["outcome"] == "killed" for test in records) else "survived" if complete else "error"}


def archive(repository, revision, destination):
    destination.mkdir(parents=True)
    with tempfile.TemporaryFile() as file:
        subprocess.run(["git", "-C", str(repository), "archive", revision], stdout=file, check=True)
        file.seek(0)
        with tarfile.open(fileobj=file) as tar:
            tar.extractall(destination, filter="data")


def execute(template, active, evidence, cargo, rustc, target, test_filter, mutation=None):
    disk_check(active.parent)
    if active.exists():
        shutil.rmtree(active)
    shutil.copytree(template, active, symlinks=True)
    evidence.mkdir(parents=True)
    if mutation:
        path = active / mutation["file"]
        if not path.resolve().is_relative_to(active.resolve()):
            raise ValueError("mutation path leaves owned source")
        diff = evidence / "mutation.diff"
        diff.write_text(mutation["diff"])
        patched = subprocess.run(["patch", "--batch", "--fuzz=0", str(path), str(diff)], capture_output=True, text=True)
        (evidence / "patch.log").write_text(patched.stdout + patched.stderr)
        if patched.returncode:
            raise ValueError("exact cargo-mutants patch rejected")
    environment = {"PATH": str(cargo.parent) + os.pathsep + os.environ.get("PATH", ""),
                   "CARGO_HOME": str(Path.home() / ".cargo"), "RUSTC": str(rustc),
                   "CARGO_TARGET_DIR": str(target), "CARGO_BUILD_JOBS": "2"}
    for key, name in [("HOME", "home"), ("TMPDIR", "tmp"), ("XDG_CACHE_HOME", "cache")]:
        path = evidence / name
        path.mkdir()
        environment[key] = str(path)
    command = [str(cargo), "test", "--locked", "--lib", test_filter, "--", "--test-threads=1"]
    process = run_command(command, active, environment, evidence, timeout=180)
    if process.get('cleanupErrors'):
        raise RuntimeError('native cleanup uncertain; preserve active source and stop')
    stdout = (evidence / "stdout.txt").read_text()
    stderr = (evidence / "stderr.txt").read_text()
    if process["status"] != "finished":
        result = {"complete": False, "tests": [], "status": "timeout" if process["status"] == "timeout" else "error"}
    elif process["exitCode"] == 101 and "could not compile" in stderr and "test result:" not in stdout:
        result = {"complete": False, "tests": [], "status": "invalid"}
    else:
        # Mutations are limited to production lines before these frozen tests.
        file = "src/mutator/result.rs"
        sites = {(file, number) for number, line in enumerate((template / file).read_text().splitlines(), 1)
                 if re.search(r"^\s*assert(?:_eq|_ne)?!\s*\(", line)}
        result = parse_libtest(stdout, process["exitCode"], sites)
        for test in result["tests"]:
            test["file"] = file
    result["process"] = process
    write_json(evidence / "execution.json", result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True, type=Path)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--cargo", required=True, type=Path)
    parser.add_argument("--rustc", required=True, type=Path)
    parser.add_argument("--cargo-mutants", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    install_signal_handlers()
    output = args.output.resolve()
    output.mkdir(parents=True)
    engine_version = subprocess.check_output([str(args.cargo_mutants), "mutants", "--version"], text=True).strip()
    if engine_version != "cargo-mutants 27.1.0":
        raise ValueError("native experiment requires cargo-mutants 27.1.0")
    tool_digest = runner_fingerprint()
    template = output / "template"
    archive(args.repository, args.revision, template)
    source_digest = hash_tree(template)[0]
    executables = {str(path.resolve()): hashlib.sha256(path.resolve().read_bytes()).hexdigest() for path in [args.cargo,args.rustc,args.cargo_mutants]}
    planned = subprocess.run([str(args.cargo_mutants), "mutants", "--list", "--json", "--file", "src/mutator/result.rs"], cwd=template, capture_output=True, text=True, check=True,
                             env={**os.environ, "PATH": str(args.cargo.parent) + os.pathsep + os.environ.get("PATH", "")})
    all_mutants = json.loads(planned.stdout)
    # Operator and source-region choice is declared before observing outcomes.
    mutants = [mutation for mutation in all_mutants if mutation["genre"] in {"BinaryOperator", "UnaryOperator"} and mutation["span"]["start"]["line"] <= 211]
    write_json(output / "plan.json", {"engine": "cargo-mutants 27.1.0", "engineLicense": "MIT", "planned": len(all_mutants),
                                    "selection": "BinaryOperator and UnaryOperator in result.rs lines 1 through 211", "mutants": mutants})
    active, target = output / "active", output / "target"
    baseline = execute(template, active, output / "baseline", args.cargo, args.rustc, target, "mutator::result::tests")
    tests = [{key: value for key, value in test.items() if key != "outcome"} for test in baseline["tests"]]
    matrix = {"schemaVersion": 1, "baselineComplete": baseline["complete"] and baseline["status"] == "survived", "tests": tests,
              "provenance": {"sourceSha256": source_digest, "executables": executables, "runnerSha256": tool_digest,
                             "pythonSha256": hashlib.sha256(Path(sys.executable).resolve().read_bytes()).hexdigest(), "plannedMutants": len(mutants), "testDurations": "unavailable; cargo wall duration includes build"},
              "subject": {"name": "Archguard Rust protocol slice", "revision": args.revision, "scope": "3 mutator::result::tests; selected binary/unary operators"}, "mutants": []}
    for index, mutation in enumerate(mutants):
        if not matrix["baselineComplete"]:
            break
        mid = hashlib.sha256(mutation["diff"].encode()).hexdigest()
        result = execute(template, active, output / f"mutant-{index:04}", args.cargo, args.rustc, target, "mutator::result::tests", mutation)
        observed = {test["id"]: test["outcome"] for test in result["tests"]}
        if result["complete"] and set(observed) != {test["id"] for test in tests}:
            result["complete"], result["status"] = False, "error"
        column = {"id": mid, "name": mutation["name"], "status": result["status"],
                  "outcomes": observed if result["complete"] else {test["id"]: "unknown" for test in tests}}
        if result["status"] == "invalid":
            column["invalidReason"] = "compilerRejected"
            column["compilerEvidence"] = f"mutant-{index:04}/stderr.txt"
        matrix["mutants"].append(column)
        write_json(output / "matrix.json", matrix)
        print(f"Rust {index + 1}/{len(mutants)} {result['status']}", flush=True)
    if runner_fingerprint() != tool_digest:
        raise RuntimeError("native experiment code changed during measurement")
    if hash_tree(template)[0] != source_digest:
        raise RuntimeError("native immutable template changed")
    write_json(output / "matrix.json", matrix)


if __name__ == "__main__":
    main()
