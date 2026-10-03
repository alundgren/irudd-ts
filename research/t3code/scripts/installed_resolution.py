#!/usr/bin/env python3
"""Record installed source coverage and merged platform-change graph evidence."""
import argparse
from collections import Counter, deque
import hashlib
import json
from pathlib import Path
import subprocess
import time


REVISIONS = {
    "installed": "e0db2a5e58bcbe7d738bca7667d2440ddb83e30f",
    "12380-before": "e0649ed7d8f8b5f21d114ff2b09ef803184f1d05",
    "12380-fixed": "53830d413474b2a5749c040d4382c96002894b88",
    "12381-fixed": "7e7cd32463b528838cb10ef6b89ef6ceac1f7a4c",
}
PULL_REQUESTS = {
    "installed": "https://github.com/pingdotgg/t3code/pull/13295",
    "12380-before": "https://github.com/pingdotgg/t3code/pull/12380",
    "12380-fixed": "https://github.com/pingdotgg/t3code/pull/12380",
    "12381-fixed": "https://github.com/pingdotgg/t3code/pull/12381",
}
ROOTS = {"apps", "packages", "infra", "scripts", "oxlint-plugin-t3code"}
EXTENSIONS = {".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def dependency_path(project, start, target, include_types):
    files = {file["path"]: file for file in project["files"]}
    queue = deque([[start]])
    visited = {start}
    while queue:
        path = queue.popleft()
        if path[-1] == target:
            return path
        for edge in files.get(path[-1], {}).get("imports", []):
            if (edge["status"] != "internal" or edge["target"] in visited
                    or edge["kind"] not in {"import", "reExport", "require"}
                    or (edge["typeOnly"] and not include_types)):
                continue
            visited.add(edge["target"])
            queue.append(path + [edge["target"]])
    return None


def platform_evidence(project, case, platform):
    if case == "installed" or platform not in {"ios", "android"}:
        return None
    start = "apps/mobile/src/features/agent-awareness/remoteRegistration.ts"
    target = "apps/mobile/src/widgets/AgentActivity.tsx"
    value_path = dependency_path(project, start, target, False)
    type_path = dependency_path(project, start, target, True)
    expected = case == "12380-before" or platform == "ios"
    assert bool(value_path) == expected, (case, platform, value_path)
    assert type_path is not None, (case, platform, "missing type-inclusive control")
    evidence = {"staticValueDependencyPath": value_path, "typeInclusivePath": type_path,
                "coverage": "resolved static internal edges; incomplete-analysis problems retained"}
    if case == "12381-fixed":
        caller = "apps/mobile/src/features/home/HomeRouteScreen.tsx"
        imports = next(file["imports"] for file in project["files"] if file["path"] == caller)
        edge = next(edge for edge in imports if edge["specifier"] == "./HomeHeader")
        suffix = ".android.tsx" if platform == "android" else ".tsx"
        expected_target = "apps/mobile/src/features/home/HomeHeader" + suffix
        assert edge["target"] == expected_target, (platform, edge)
        evidence["headerTarget"] = expected_target
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("checkout", type=Path)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--archguard-revision", required=True)
    parser.add_argument("--case", choices=REVISIONS, default="installed")
    parser.add_argument("--profiles", nargs="+", default=["inventory", "node", "browser", "android", "ios"])
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--measure", action="store_true")
    parser.add_argument("--runs", type=int, default=1)
    parser.add_argument("--warmups", type=int, default=0)
    args = parser.parse_args()
    if args.runs < 1 or args.warmups < 0 or ((args.runs > 1 or args.warmups) and not args.measure):
        parser.error("positive --runs greater than one requires --measure")
    root = args.checkout.resolve()
    revision = git(root, "rev-parse", "HEAD")
    assert revision == REVISIONS[args.case], (args.case, revision)
    assert not git(root, "status", "--porcelain", "--untracked-files=no"), "tracked checkout changes"
    tracked = git(root, "ls-files").splitlines()
    eligible = {path for path in tracked if Path(path).suffix in EXTENSIONS
                and (path.split("/")[0] in ROOTS or path == "vite.config.ts")}
    hashes = {path: digest((root / path).read_bytes()) for path in sorted(eligible)}
    report = {
        "case": args.case, "upstreamRevision": revision,
        "upstreamRepository": "https://github.com/pingdotgg/t3code",
        "upstreamPullRequest": PULL_REQUESTS[args.case],
        "archguardRevision": args.archguard_revision,
        "archguardSourceSha256": digest(json.dumps({
            str(path.relative_to(Path(__file__).resolve().parents[3])): digest(path.read_bytes())
            for path in sorted((Path(__file__).resolve().parents[3] / "src").rglob("*.rs"))
        }, sort_keys=True).encode()),
        "executableSha256": digest(args.cli.read_bytes()),
        "eligibleTrackedFiles": len(eligible),
        "sourceCorpusSha256": digest(json.dumps(hashes, sort_keys=True).encode()),
        "inputHashes": {path: digest((root / path).read_bytes())
                        for path in ["pnpm-lock.yaml", "pnpm-workspace.yaml", "package.json"]},
        "profiles": [],
    }
    profiles = Path(__file__).resolve().parents[3] / "research/t3code/installed/profiles"
    for name in args.profiles:
        assert name in {"inventory", "node", "browser", "android", "ios", "server-closure"}, name
        config = profiles / (name + ".json")
        samples = []
        warmup_samples = []
        expected_output = None
        for index in range(args.runs + args.warmups):
            started = time.perf_counter()
            run = subprocess.run([str(args.cli.resolve()), "facts", "--root", str(root),
                                  "--config", str(config)], capture_output=True, check=False)
            elapsed = (time.perf_counter() - started) * 1000
            assert run.returncode in {0, 2} and not run.stderr, run.stderr.decode()
            if expected_output is None:
                expected_output = run.stdout
            assert run.stdout == expected_output, (name, "facts changed between runs")
            if args.measure:
                (warmup_samples if index < args.warmups else samples).append(elapsed)
        project = json.loads(run.stdout)
        files = {file["path"] for file in project["files"]}
        if name in {"inventory", "node", "browser"}:
            assert files == eligible, {"missing": sorted(eligible-files), "extra": sorted(files-eligible)}
        if name == "server-closure":
            selected = set(json.loads(config.read_text())["include"])
            assert files == selected, {"missing": sorted(selected-files), "extra": sorted(files-selected)}
            assert not project["problems"], project["problems"]
        counts = Counter(edge["status"] for file in project["files"] for edge in file["imports"])
        normalized = dict(project, root="<checkout>")
        report["profiles"].append({
            "profile": name, "configSha256": digest(config.read_bytes()),
            "resolution": project["resolution"], "files": len(files),
            "excludedTrackedSources": sorted(eligible-files),
            "imports": sum(counts.values()), "statuses": dict(sorted(counts.items())),
            "exit": run.returncode, "complete": not project["problems"],
            "factsSha256": digest(json.dumps(normalized, sort_keys=True).encode()),
            "problems": project["problems"],
            "externalLookupFailures": [
                {"file": file["path"], "specifier": edge["specifier"], "detail": edge["detail"]}
                for file in project["files"] for edge in file["imports"]
                if edge["status"] == "external" and edge["detail"]
                and "Cannot find module" in edge["detail"]],
            "platformEvidence": platform_evidence(project, args.case, name),
            "elapsedMs": samples,
            "warmupElapsedMs": warmup_samples,
            "serverSourceRoots": sum(path.startswith("apps/server/src/") for path in files)
                                 if name == "server-closure" else None,
        })
        print(f"{args.case}/{name}: {len(files)} files, {sum(counts.values())} imports, "
              f"{len(project['problems'])} problems, exit {run.returncode}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
