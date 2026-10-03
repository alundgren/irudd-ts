#!/usr/bin/env python3
"""Measure opt-in graph reuse against fresh analysis on verified PR reductions."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import tempfile
import time


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def invoke(binary, root, config, cache=None, action="check"):
    command = [str(binary), action, "--root", str(root), "--config", str(config)]
    if action == "check":
        command.append("--json")
    if cache is not None:
        command.extend(["--cache", str(cache)])
    load = list(os.getloadavg()) if hasattr(os, "getloadavg") else None
    started = time.perf_counter_ns()
    result = subprocess.run(command, capture_output=True)
    elapsed = (time.perf_counter_ns() - started) / 1_000_000
    if result.stderr or result.returncode not in [0, 1, 2]:
        raise RuntimeError(f"{command}: {result.returncode}, {result.stderr.decode()}")
    report = json.loads(result.stdout)
    return {"wallMs": elapsed, "exitStatus": result.returncode, "command": command,
            "loadBefore": load, "report": report}


def normalized(sample):
    report = dict(sample["report"])
    report.pop("elapsedMs", None)
    report.pop("cache", None)
    return sample["exitStatus"], report


def input_record(root, config, facts):
    paths = sorted({file["path"] for file in facts["files"]}
                   | {package["path"] for package in facts["packages"]})
    files = [{"path": path, "sha256": sha256((root / path).read_bytes())} for path in paths]
    metadata = [{"path": path, "sha256": sha256((root / path).read_bytes())}
                for path in ["pnpm-lock.yaml", "pnpm-workspace.yaml", "node_modules/.modules.yaml", "node_modules/.pnpm/lock.yaml"]
                if (root / path).is_file()]
    config_hash = sha256(config.read_bytes())
    return {"sourceFiles": len(facts["files"]), "selectedInputFiles": files,
            "configSha256": config_hash,
            "repositoryMetadata": metadata,
            "corpusSha256": sha256(json.dumps({"files": files, "config": config_hash, "metadata": metadata},
                                               sort_keys=True).encode())}


def apply_step(root, fixed, step):
    if step["operation"] == "delete":
        (root / step["path"]).unlink()
    else:
        destination = root / step["path"]
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(fixed / step["path"], destination)


def restore(workload, root, steps):
    if workload.get("external"):
        return
    if root.exists():
        shutil.rmtree(root)
    shutil.copytree(workload["before"], root)
    for step in workload["steps"][:steps]:
        apply_step(root, workload["fixed"], step)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/archguard"))
    parser.add_argument("--repetitions", type=int, default=7)
    parser.add_argument("--output", type=Path, default=Path("benchmarks/local/cache.json"))
    parser.add_argument("--control-root", type=Path)
    parser.add_argument("--control-config", type=Path)
    parser.add_argument("--control-revision")
    args = parser.parse_args()
    if args.repetitions < 1:
        parser.error("repetitions must be positive")
    if bool(args.control_root) != bool(args.control_config):
        parser.error("control-root and control-config must be supplied together")
    repo = Path(__file__).resolve().parents[1]
    binary = args.binary.resolve()
    history = repo / "benchmarks/history"
    evidence = json.loads((repo / "docs/historical-evidence.json").read_text())
    workloads = []
    for profile in ["generic", "android", "ios"]:
        workloads.append({"id": f"13151/{profile}", "before": history / "13151/before",
                          "fixed": history / "13151/fixed", "config": "archguard.json" if profile == "generic" else f"archguard.{profile}.json",
                          "steps": [{"operation": "replace", "path": path} for path in [
                              "apps/mobile/src/components/FilePreview.tsx",
                              "apps/mobile/src/components/FilePreviewModal.types.ts",
                              "apps/mobile/src/components/FilePreview.ios.tsx",
                              "apps/mobile/src/components/FilePreviewModal.tsx"]]})
    workloads.append({"id": "14389", "before": history / "14389/before", "fixed": history / "14389/fixed",
                      "config": "archguard.json", "steps": [{"operation": "replace", "path": "packages/client-runtime/src/connection/index.ts"}]})
    workloads.append({"id": "historical-controls", "before": history / "controls", "fixed": history / "controls",
                      "config": "archguard.json", "steps": []})
    if args.control_root:
        workloads.append({"id": "installed-server-control", "external": True,
                          "root": args.control_root.resolve(), "configPath": args.control_config.resolve(),
                          "revision": args.control_revision, "steps": []})

    cases = []
    for workload in workloads:
        for step_index in range(len(workload["steps"]) + 1):
            for mode in ["fresh", "empty", "unchanged"] + (["edited"] if step_index else []):
                cases.append({"id": f"{workload['id']}/step-{step_index}/{mode}", "workload": workload,
                              "step": step_index, "mode": mode, "samples": []})
    start_load = list(os.getloadavg()) if hasattr(os, "getloadavg") else None
    sample_order = []
    rng = random.Random(20261003)
    with tempfile.TemporaryDirectory(prefix="archguard-cache-benchmark-") as temporary:
        scratch = Path(temporary)

        def measure(case):
            workload = case["workload"]
            root = workload["root"] if workload.get("external") else scratch / workload["id"].replace("/", "-")
            cache = scratch / (workload["id"].replace("/", "-") + ".cache.json")
            config = workload["configPath"] if workload.get("external") else root / workload["config"]
            cache.unlink(missing_ok=True)
            prime_complete = None
            if case["mode"] in ["unchanged", "edited"]:
                prior = case["step"] - int(case["mode"] == "edited")
                restore(workload, root, prior)
                prime = invoke(binary, root, config, cache)
                prime_complete = prime["report"]["complete"]
                if case["mode"] == "edited":
                    apply_step(root, workload["fixed"], workload["steps"][prior])
            else:
                restore(workload, root, case["step"])
            measured = invoke(binary, root, config, None if case["mode"] == "fresh" else cache)
            fresh = invoke(binary, root, config)
            if normalized(measured) != normalized(fresh):
                raise AssertionError(f"cached/fresh report mismatch: {case['id']}")
            fresh_facts = invoke(binary, root, config, action="facts")
            cached_facts = invoke(binary, root, config, cache, action="facts")
            if fresh_facts["exitStatus"] != cached_facts["exitStatus"] or fresh_facts["report"] != cached_facts["report"]:
                raise AssertionError(f"cached/fresh facts mismatch: {case['id']}")
            report = measured["report"]
            expected_incomplete = workload["id"].startswith("13151/") and case["step"] == 1
            if workload.get("external"):
                if not report["complete"]:
                    raise AssertionError("large control must have a complete source graph")
            elif report["complete"] == expected_incomplete:
                raise AssertionError(f"unexpected completion: {case['id']}")
            if case["step"] == 0 and workload["steps"] and len(report["diagnostics"]) != 1:
                raise AssertionError(f"before policy failure missing: {case['id']}")
            if case["step"] == len(workload["steps"]) and not workload.get("external") and report["diagnostics"]:
                raise AssertionError(f"fixed/control is not clean: {case['id']}")
            stats = report.get("cache")
            if case["mode"] == "unchanged" and report["complete"]:
                if stats["parsedFiles"] or stats["resolvedEdges"] or (report["imports"] and not stats["reusedEdges"]):
                    raise AssertionError(f"unchanged graph was not reused: {case['id']}")
            measured["primeComplete"] = prime_complete
            measured["input"] = input_record(root, config, fresh_facts["report"])
            if report["complete"]:
                snapshot = json.loads(cache.read_text())["snapshot"]
                measured["input"]["resolverInputSha256"] = sha256(json.dumps(snapshot["observations"], sort_keys=True).encode())
                measured["input"]["resolverInputs"] = len(snapshot["observations"])
            normalized_facts = dict(fresh_facts["report"])
            normalized_facts["root"] = "."
            measured["factsSha256"] = sha256(json.dumps(normalized_facts, sort_keys=True).encode())
            return measured

        for case in cases:
            case["warmup"] = measure(case)
        for repetition in range(args.repetitions):
            randomized = list(range(len(cases)))
            rng.shuffle(randomized)
            for index in randomized:
                case = cases[index]
                sample = measure(case)
                sample["round"] = repetition
                case["samples"].append(sample)
                sample_order.append(case["id"])
        for case in cases:
            values = [sample["wallMs"] for sample in case["samples"]]
            case["summary"] = {"minimumMs": min(values), "medianMs": statistics.median(values), "maximumMs": max(values)}
            workload = case.pop("workload")
            case["editSteps"] = workload["steps"]
            case["controlRevision"] = workload.get("revision")
            print(f"{case['id']}: {statistics.median(values):.2f} ms [{min(values):.2f}, {max(values):.2f}]")
    revision = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    local_diff = subprocess.check_output(["git", "-C", str(repo), "diff", "HEAD"])
    output = {"schemaVersion": 1, "description": "Fresh-process latency; empty persistent cache is not cold disk; serial randomized trials",
              "sourceRevision": revision, "trackedDiffSha256": sha256(local_diff),
              "binary": str(binary), "executableSha256": sha256(binary.read_bytes()),
              "platform": platform.platform(), "repetitions": args.repetitions, "seed": 20261003,
              "startLoad": start_load, "endLoad": list(os.getloadavg()) if hasattr(os, "getloadavg") else None,
              "historicalEvidence": [case for case in evidence["cases"] if case["id"] in ["13151", "14389"]],
              "sampleOrder": sample_order, "cases": cases}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2) + "\n")


if __name__ == "__main__":
    main()
