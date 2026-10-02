#!/usr/bin/env python3
"""Local process benchmarks. Refuse timing ratios until expected work agrees."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess
import time

REPO = Path(__file__).resolve().parents[1]


def write(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)


def configuration(root, name, value):
    path = root / (name + ".json")
    write(path, json.dumps(value, indent=2) + "\n")
    return path


def generate(root, workload, count):
    sources = {}
    expected = set()
    if workload == "graph":
        for index in range(count // 3):
            client, helper, server = (f"{directory}/{index}.ts" for directory in ["client", "shared", "server"])
            sources[client] = f'import "../{helper}";\n'
            bad = index % 4 == 0
            if bad:
                sources[helper] = (f'import type {{ db }} from "../{server}";\nexport type Data = typeof db;\n'
                                   if index % 8 == 4 else f'import "../{server}";\n')
            else:
                sources[helper] = "export const helper = 1;\n"
            sources[server] = "export const db = 1;\n"
            if bad:
                expected.add((client, server))
        for index in range(count % 3):
            sources[f"shared/padding{index}.ts"] = "export const padding = 1;\n"
    elif workload == "direct":
        sources["server/db.ts"] = "export const db = 1;\n"
        sources["shared/helper.ts"] = "export const helper = 1;\n"
        for index in range(count - 2):
            client = f"client/{index}.ts"
            bad = index % 4 == 0
            sources[client] = 'import "../server/db.ts";\n' if bad else 'import "../shared/helper.ts";\n'
            if bad:
                expected.add(client)
    elif workload == "cycles":
        for index in range(count // 2):
            a, b = f"{index}a.ts", f"{index}b.ts"
            sources[a] = f'import type {{ B }} from "./{b}";\nexport interface A {{ value: B }}\n'
            if index % 4 == 0:
                sources[b] = f'import type {{ A }} from "./{a}";\nexport interface B {{ value: A }}\n'
                expected.add(tuple(sorted([a, b])))
            elif index % 4 == 1:
                sources[b] = f'import("./{a}");\nexport interface B {{ value: string }}\n'
            else:
                sources[b] = "export interface B { value: string }\n"
        if count % 2:
            sources["padding.ts"] = "export const padding = 1;\n"
    else:
        raise ValueError(workload)
    assert len(sources) == count
    for file, source in sources.items():
        write(root / file, source)
    digest = hashlib.sha256()
    for file, source in sorted(sources.items()):
        digest.update(file.encode() + b"\0" + source.encode() + b"\0")
    return sources, expected, digest.hexdigest()


def execute(command, cwd=REPO):
    start = time.perf_counter_ns()
    result = subprocess.run(command, cwd=cwd, capture_output=True, text=True, timeout=60)
    elapsed = (time.perf_counter_ns() - start) / 1_000_000
    if result.returncode not in [0, 1]:
        raise RuntimeError(f"Command failed: {command}\n{result.stdout}\n{result.stderr}")
    if result.stderr:
        raise RuntimeError(f"Unexpected stderr: {command}\n{result.stderr}")
    return elapsed, result.returncode, json.loads(result.stdout)


def normal(report, engine, workload, root, expected_files):
    if engine == "archguard":
        assert report["complete"] and not report["problems"], report
        assert report["files"] == expected_files, report
        diagnostics = report["diagnostics"]
        if workload == "graph":
            return {(d["file"], d["evidence"][-1]) for d in diagnostics}
        if workload == "cycles":
            return {tuple(sorted(set(d["evidence"]))) for d in diagnostics}
        return {d["file"] for d in diagnostics}
    assert report["number_of_files"] == expected_files, report
    assert report["number_of_rules"] == 1, report
    diagnostics = report["diagnostics"]
    files = [Path(d["filename"]).resolve().relative_to(root).as_posix() for d in diagnostics]
    if workload == "graph":
        return {(file, d["message"].split("Forbidden dependency on ", 1)[1]) for file, d in zip(files, diagnostics)}
    if workload == "cycles":
        # Each generated static cycle has exactly two modules. Oxlint emits an
        # import diagnostic per participant; Archguard emits one per component.
        cycles = set()
        for file, diagnostic in zip(files, diagnostics):
            span = diagnostic["labels"][0]["span"]
            source = (root / file).read_text()
            literal = source[span["offset"]:span["offset"] + span["length"]]
            assert len(literal) > 2 and literal[0] in ['"', "'"], diagnostic
            assert literal[-1] == literal[0], diagnostic
            target = (root / file).parent.joinpath(literal[1:-1]).resolve().relative_to(root).as_posix()
            cycles.add(tuple(sorted([file, target])))
        return cycles
    return set(files)


def summary(samples):
    return {"medianMs": statistics.median(samples), "minMs": min(samples), "maxMs": max(samples), "samplesMs": samples}


def benchmark(root, workload, count, args):
    sources, expected, digest = generate(root, workload, count)
    plugin = str(REPO / "benchmarks/plugins/oxlint.cjs")
    oxbase = {"categories": {"correctness": "off"}, "plugins": [], "rules": {}}
    commands = {}
    engines = {}
    def host(name, rules=None, command=None):
        config = {"schemaVersion": 1, "include": ["**/*.ts"], "rules": rules or []}
        if command:
            config["plugins"] = [{"name": "client-server", "command": command, "timeoutMs": 30000}]
        path = configuration(root, name, config)
        commands[name] = [str(args.binary), "check", "--json", "--root", str(root), "--config", str(path)]
        engines[name] = "archguard"
    def ox(name, value):
        path = configuration(root, name, value)
        commands[name] = [str(args.oxlint), "--threads=1", "--no-ignore", "--disable-nested-config", "-c", str(path), "-f", "json", *[str(root / file) for file in sorted(sources)]]
        engines[name] = "oxlint"
    if workload == "graph":
        host("native", [{"id": "client-server", "kind": "forbiddenDependency", "files": ["client/**"], "targets": ["server/**"], "transitive": True, "includeTypes": True}])
        host("typescript-plugin", command=["node", str(REPO / "examples/graph-plugin.ts")])
        host("rust-plugin", command=[str(args.rust_plugin)])
        ox("oxlint-independent-graph", {**oxbase, "jsPlugins": [{"name": "bench", "specifier": plugin}], "rules": {"bench/client-server": ["error", {"root": str(root)}]}})
    elif workload == "direct":
        host("native", [{"id": "direct", "kind": "forbiddenImport", "specifiers": ["../server/db.ts"], "includeTypes": True}])
        ox("oxlint-built-in", {**oxbase, "rules": {"no-restricted-imports": ["error", {"paths": ["../server/db.ts"]}]}})
        ox("oxlint-javascript-plugin", {**oxbase, "jsPlugins": [{"name": "bench", "specifier": plugin}], "rules": {"bench/direct-import": "error"}})
    else:
        host("native", [{"id": "cycles", "kind": "noCycles", "includeTypes": True}])
        ox("oxlint-built-in", {**oxbase, "plugins": ["import"], "rules": {"import/no-cycle": ["error", {"ignoreTypes": False, "ignoreExternal": True, "allowUnsafeDynamicCyclicDependency": True}]}})
    samples = {name: [] for name in commands}
    equivalence = {}
    base_result = {"workload": workload, "files": count, "imports": sum(source.count("import") for source in sources.values()), "sourceBytes": sum(len(source.encode()) for source in sources.values()), "corpusSha256": digest, "expectedViolations": len(expected), "expected": sorted(expected), "equivalence": equivalence, "commands": commands}
    for name, command in commands.items():
        try:
            _, code, report = execute(command)
            observed = normal(report, engines[name], workload, root, count)
            expected_diagnostics = len(expected) * (2 if workload == "cycles" and engines[name] == "oxlint" else 1)
            matched = observed == expected and code == int(bool(expected)) and len(report["diagnostics"]) == expected_diagnostics
            equivalence[name] = {"matched": matched, "expected": len(expected), "observed": len(observed), "diagnostics": len(report["diagnostics"]), "exitCode": code}
        except (AssertionError, RuntimeError, KeyError, ValueError, subprocess.TimeoutExpired) as error:
            equivalence[name] = {"matched": False, "reason": str(error)}
    if not all(item["matched"] for item in equivalence.values()):
        return {**base_result, "status": "incomparable", "reason": "Completion, file count, rules or expected violations differed. No timings or ratios recorded."}
    # Rotate a seeded randomized order to reduce ordering and cache bias.
    rng = random.Random(42)
    for repetition in range(args.repetitions):
        order = list(commands)
        rng.shuffle(order)
        for name in order:
            try:
                elapsed, code, report = execute(commands[name])
                assert normal(report, engines[name], workload, root, count) == expected
                assert code == int(bool(expected))
                assert len(report["diagnostics"]) == len(expected) * (2 if workload == "cycles" and engines[name] == "oxlint" else 1)
                samples[name].append(elapsed)
            except (AssertionError, RuntimeError, KeyError, ValueError, subprocess.TimeoutExpired) as error:
                return {**base_result, "status": "incomparable", "reason": f"Sample {repetition}/{name} changed expected work: {error}"}
    timings = {name: summary(values) for name, values in samples.items()}
    base = timings["native"]["medianMs"]
    return {**base_result, "status": "comparable", "timings": timings, "medianRelativeToNative": {name: values["medianMs"] / base for name, values in timings.items()}}


def overhead(root, count, args):
    sources, _, digest = generate(root, "cycles", count)
    commands = {}
    for name, command in [("host-no-rules", None), ("host-typescript-empty-sdk", ["node", str(REPO / "benchmarks/plugins/empty.ts")]), ("host-rust-no-selected-clients", [str(args.rust_plugin)])]:
        config = {"schemaVersion": 1, "include": ["**/*.ts"]}
        if command:
            config["plugins"] = [{"name": "empty", "command": command, "timeoutMs": 30000}]
        path = configuration(root, name, config)
        commands[name] = [str(args.binary), "check", "--json", "--root", str(root), "--config", str(path)]
    samples = {name: [] for name in commands}
    for repetition in range(args.repetitions + 1):
        for name in random.Random(repetition).sample(list(commands), len(commands)):
            try:
                elapsed, code, report = execute(commands[name])
                assert code == 0 and report["complete"] and not report["diagnostics"] and report["files"] == count
                if repetition:
                    samples[name].append(elapsed)
            except (AssertionError, RuntimeError, KeyError, ValueError, subprocess.TimeoutExpired) as error:
                return {"status": "incomparable", "files": count, "corpusSha256": digest, "commands": commands, "reason": f"Empty plugin {name} failed completion or work checks: {error}"}
    timings = {name: summary(values) for name, values in samples.items()}
    base = timings["host-no-rules"]["medianMs"]
    return {"status": "comparable", "files": count, "corpusSha256": digest, "timings": timings, "addedMedianMs": {name: value["medianMs"] - base for name, value in timings.items()}, "commands": commands}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=REPO / "target/release/archguard")
    parser.add_argument("--rust-plugin", type=Path, default=REPO / "target/release/examples/graph_plugin")
    parser.add_argument("--oxlint", type=Path, default=REPO / "benchmarks/toolchain/node_modules/.bin/oxlint")
    parser.add_argument("--sizes", nargs="+", type=int, default=[100, 1000])
    parser.add_argument("--repetitions", type=int, default=7)
    parser.add_argument("--output", type=Path, default=REPO / "benchmarks/local/results.json")
    args = parser.parse_args()
    if args.repetitions < 3 or any(count < 6 for count in args.sizes):
        parser.error("Use at least 3 repetitions and 6 files")
    args.binary, args.rust_plugin, args.oxlint = (path.resolve() for path in [args.binary, args.rust_plugin, args.oxlint])
    for path in [args.binary, args.rust_plugin, args.oxlint]:
        if not path.is_file():
            parser.error(f"Missing executable {path}")
    def version(command):
        return subprocess.check_output(command, text=True).strip()
    results = {"schemaVersion": 1, "dateUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "platform": platform.platform(), "cpuCount": os.cpu_count(), "machine": platform.machine(), "versions": {"archguard": version([str(args.binary), "--version"]), "oxlint": version([str(args.oxlint), "--version"]), "node": version(["node", "--version"]), "rustc": version(["rustc", "--version"]), "typescript": version([str(REPO / "benchmarks/toolchain/node_modules/.bin/tsc"), "--version"]), "graphParser": json.loads((REPO / "benchmarks/toolchain/node_modules/typescript-parser/package.json").read_text())["version"]}, "gitRevision": version(["git", "-C", str(REPO), "rev-parse", "HEAD"]), "binarySha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(), "rustPluginSha256": hashlib.sha256(args.rust_plugin.read_bytes()).hexdigest(), "repetitions": args.repetitions, "warmups": 1, "measurement": "wall time of complete fresh process, captured JSON output, serial trials, warm filesystem caches, Oxlint threads=1", "benchmarks": [], "overhead": []}
    for count in args.sizes:
        for workload in ["graph", "direct", "cycles"]:
            root = REPO / "benchmarks/local" / f"{workload}-{count}"
            result = benchmark(root, workload, count, args)
            results["benchmarks"].append(result)
            print(f"{workload} {count}: " + (", ".join(f"{name}={data['medianMs']:.2f}ms" for name, data in result["timings"].items()) if result["status"] == "comparable" else "incomparable, " + result["reason"]), flush=True)
        results["overhead"].append(overhead(REPO / "benchmarks/local" / f"overhead-{count}", count, args))
    write(args.output, json.dumps(results, indent=2) + "\n")
    print(args.output)


if __name__ == "__main__":
    main()
