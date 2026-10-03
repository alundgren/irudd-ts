#!/usr/bin/env python3
"""Run independently authored T3 convention fixtures and an optional pinned-tree scan."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import tempfile
import time

REPO = Path(__file__).resolve().parents[3]
PIN = "e0db2a5e58bcbe7d738bca7667d2440ddb83e30f"
PRESET = REPO / "examples/t3code/profiles/t3code-structure.json"
MIGRATIONS = "apps/server/src/persistence/Migrations"
REGISTRY = "apps/server/src/persistence/Migrations.ts"
SERVICES = "apps/server/src/persistence/Services"
LAYERS = "apps/server/src/persistence/Layers"
TOOLKIT = "apps/server/src/mcp/toolkits/preview"
CONTRACT = "packages/contracts/src/rpc.ts"
SOURCES = {
    f"{MIGRATIONS}/001_Events.ts": "export default 1;\n",
    f"{MIGRATIONS}/002_Projects.ts": "export default 2;\n",
    f"{MIGRATIONS}/001_Events.test.ts": "import m from './001_Events.ts';\n",
    REGISTRY: "import a from './Migrations/001_Events.ts'; import b from './Migrations/002_Projects.ts'; export const entries=[a,b];\n",
    f"{SERVICES}/Projects.ts": "export class Projects {}\n",
    f"{LAYERS}/Projects.ts": "import {Projects} from '../Services/Projects.ts'; export const live=Projects;\n",
    f"{LAYERS}/Sqlite.ts": "export const sqlite=1;\n",
    f"{TOOLKIT}/tools.ts": "export const standard=1; export const screenshot=2;\n",
    f"{TOOLKIT}/handlers.ts": "import {standard,screenshot} from './tools.ts'; export const layers=[standard,screenshot];\n",
    f"{TOOLKIT}/handlers.test.ts": "import './handlers.ts';\n",
    CONTRACT: "export type Message={id:string};\n",
    "apps/server/src/ws.ts": "import type {Message} from '../../../packages/contracts/src/rpc.ts';\n",
}
CASES = {
    "clean": ({}, []),
    "unregistered-migration": ({f"{MIGRATIONS}/003_New.ts": "export default 3;"}, ["migration-registry-import"]),
    "service-without-layer": ({f"{SERVICES}/Missing.ts": "export class Missing {}"}, ["service-layer-companion"]),
    "tool-without-handler": ({"apps/server/src/mcp/toolkits/new/tools.ts": "export const toolkit=1;"}, ["tool-handler-companion"]),
    "handler-without-test": ({f"{TOOLKIT}/handlers.test.ts": None}, ["tool-handler-test"]),
    "unclassified-migration-file": ({f"{MIGRATIONS}/helper.ts": "export {};"}, ["structure-classification"]),
    "contract-imports-host": ({CONTRACT: "import '../../../apps/server/src/ws.ts';"}, ["contracts-no-host"]),
    "service-imports-layer": ({f"{SERVICES}/Projects.ts": "import type {live} from '../Layers/Projects.ts';"}, ["services-no-live-layer"]),
    "handler-imports-migration": ({f"{TOOLKIT}/handlers.ts": "import '../../../persistence/Migrations/001_Events.ts';"}, ["handlers-no-migrations"]),
    "migration-without-default": ({f"{MIGRATIONS}/001_Events.ts": "export const migration=1;"}, ["migration-default-export"]),
    "production-imports-test": ({f"{TOOLKIT}/handlers.ts": "import './handlers.test.ts';"}, ["production-no-test-import"]),
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run_check(binary, root):
    start = time.perf_counter()
    result = subprocess.run([str(binary), "check", "--root", str(root), "--config", str(PRESET), "--json"], capture_output=True, text=True, check=False)
    elapsed = (time.perf_counter() - start) * 1000
    if result.returncode not in (0, 1, 2) or not result.stdout:
        raise RuntimeError(result.stderr or "check produced no report")
    return result.returncode, json.loads(result.stdout), elapsed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=REPO / "target/release/archguard")
    parser.add_argument("--inventory", type=Path, default=REPO / "target/release/examples/role_inventory")
    parser.add_argument("--t3", type=Path)
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--output", type=Path, default=REPO / "research/local/structure-simulation.json")
    args = parser.parse_args()
    if args.samples < 1:
        parser.error("samples must be positive")
    binary = args.binary.resolve(strict=True)
    artifact = {
        "schemaVersion": 1,
        "sourceRevision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip(),
        "binarySha256": digest(binary), "presetSha256": digest(PRESET),
        "inputSha256": {str(path.relative_to(REPO)): digest(path) for path in sorted(list((REPO / "src").rglob("*.rs")) + [PRESET, Path(__file__), REPO / "Cargo.lock"])},
        "method": "serial end-to-end process latency, warm filesystem, one warmup per synthetic case; parsing, resolution, all selected native rules and JSON output included; no cross-tool ratio",
        "kind": "synthetic policy fixtures inspired by pinned T3 conventions; not historical regressions or a full replay",
        "cases": [],
    }
    for name, (changes, expected) in CASES.items():
        with tempfile.TemporaryDirectory(prefix="archguard-structure-") as directory:
            root = Path(directory)
            sources = SOURCES | changes
            for path, source in sources.items():
                if source is None:
                    continue
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(source)
            samples = []
            report = None
            for sample in range(args.samples + 1):
                code, report, elapsed = run_check(binary, root)
                observed = sorted(d["rule"] for d in report["diagnostics"])
                if code != bool(expected) or not report["complete"] or report["problems"] or observed != sorted(expected):
                    raise RuntimeError(f"{name}: status {code}, expected {expected}, observed {observed}, problems {report['problems']}")
                if sample:
                    samples.append(elapsed)
            # Every mutation has a correction check against the same clean control.
            for path in changes:
                target = root / path
                if path in SOURCES:
                    target.write_text(SOURCES[path])
                elif target.exists():
                    target.unlink()
            code, corrected, _ = run_check(binary, root)
            if code or not corrected["complete"] or corrected["diagnostics"]:
                raise RuntimeError(f"{name}: correction did not pass: {corrected}")
            corpus = json.dumps(sources, sort_keys=True).encode()
            artifact["cases"].append({"name": name, "corpusSha256": hashlib.sha256(corpus).hexdigest(), "files": report["files"], "expectedRules": expected, "complete": True, "correctionPassed": True, "processMs": samples, "medianMs": statistics.median(samples), "minMs": min(samples), "maxMs": max(samples)})
    if args.t3:
        root = args.t3.resolve(strict=True)
        revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
        if revision != PIN or subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=all"], cwd=root, text=True):
            raise RuntimeError("T3 scan requires the clean documented pinned revision")
        code, report, elapsed = run_check(binary, root)
        facts = subprocess.run([str(binary), "facts", "--root", str(root), "--config", str(PRESET)], capture_output=True, check=False)
        if facts.returncode not in (0, 2) or not facts.stdout:
            raise RuntimeError(facts.stderr.decode())
        inventory = args.inventory.resolve(strict=True)
        classified = json.loads(subprocess.check_output([str(inventory), str(PRESET)], input=facts.stdout))
        project = json.loads(facts.stdout)
        tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=root).decode().split("\0")
        paths = {file["path"] for file in project["files"]} | {package["path"] for package in project["packages"]}
        paths.update(path for path in tracked if path.endswith(".json") and not path.startswith(".repos/"))
        inputs = {path: digest(root / path) for path in sorted(paths)}
        artifact["t3InputSha256"] = inputs
        counts = Counter(role for roles in classified.values() for role in roles)
        artifact["t3"] = {"revision": revision, "exitCode": code, "files": report["files"], "imports": report["imports"], "complete": report["complete"], "problemCount": len(report["problems"]), "problemExamples": report["problems"][:5], "roleCounts": dict(sorted(counts.items())), "assignedFiles": sum(bool(roles) for roles in classified.values()), "unassignedFiles": sum(not roles for roles in classified.values()), "overlappingFiles": sum(len(roles) > 1 for roles in classified.values()), "diagnostics": report["diagnostics"], "provisionalDiagnostics": not report["complete"], "processMs": elapsed, "factsSha256": hashlib.sha256(facts.stdout).hexdigest(), "inventorySha256": digest(inventory), "note": "Single real-tree timing; incomplete source resolution is retained. Role coverage is required only in the preset's explicit classification scope. A static registry import is a prerequisite, not proof of registry tuple membership or execution."}
        artifact["t3"]["corpusSha256"] = hashlib.sha256(json.dumps(inputs, sort_keys=True).encode()).hexdigest()
        artifact["t3"]["corpusDefinition"] = "analyzed source files, inventoried package manifests and tracked JSON files outside .repos"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(artifact, indent=2) + "\n")
    print(json.dumps({"output": str(args.output), "cases": len(artifact["cases"]), "t3": artifact.get("t3", {}).get("exitCode")}))


if __name__ == "__main__":
    main()
