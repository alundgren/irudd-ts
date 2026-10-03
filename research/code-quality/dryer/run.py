"""Run a bounded comparison. Record trials locally before retaining evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import shutil
import sys
import time

HERE = Path(__file__).resolve().parent
DRYER_REVISION = "6892667b3441b88379bc8d0439fc2152b0fdb341"

def preserve_timeout(out, label, command, cwd, error, seconds):
    def decoded(value):
        return value.decode("utf-8", errors="replace") if isinstance(value, bytes) else value or ""
    (out / f"{label}.stdout").write_text(decoded(error.stdout))
    (out / f"{label}.stderr").write_text(decoded(error.stderr))
    return dict(label=label, command=[str(x) for x in command], cwd=str(cwd),
                exit_code=None, execution_status="timeout", elapsed_seconds=seconds)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def checked_revision(root, expected):
    actual = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=root, text=True)
    if actual != expected or dirty:
        raise ValueError(f"checkout must be clean at {expected}: {root}")

def post_run_checks(checkouts):
    results = []
    for root, expected in checkouts:
        try:
            checked_revision(root, expected)
            results.append(dict(checkout=str(root), expected_revision=expected, status="clean"))
        except (ValueError, subprocess.SubprocessError) as error:
            results.append(dict(checkout=str(root), expected_revision=expected, status="error", message=str(error)))
    return results

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dryer", type=Path, required=True)
    parser.add_argument("--python", type=Path, required=True)
    parser.add_argument("--t3", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    # Preserve the venv entry path; resolving its symlink would bypass the venv.
    args.python = args.python.absolute()
    for name in ["dryer", "t3", "out"]:
        setattr(args, name, getattr(args, name).resolve())
    manifest = json.loads((HERE / "corpus.json").read_text())
    for package, expected in [("typescript", "5.9.3"), ("jscpd", "4.3.0")]:
        installed = json.loads((HERE / "node_modules" / package / "package.json").read_text())["version"]
        if installed != expected:
            raise ValueError(f"{package} must be {expected}")
    checked_revision(args.dryer, DRYER_REVISION)
    checked_revision(args.t3, manifest["revision"])
    for item in manifest["files"]:
        if digest(args.t3 / item["path"]) != item["sha256"]:
            raise ValueError(f"input hash mismatch: {item['path']}")
    args.out.mkdir(parents=True, exist_ok=False)
    environment = dict(os.environ, PYTHONPATH=str(args.dryer / "src"))
    records = []
    def run(label, command, env=None, cwd=None, allowed=(0,)):
        start = time.perf_counter()
        try:
            result = subprocess.run([str(x) for x in command], env=env, cwd=cwd or HERE,
                                    text=True, capture_output=True, timeout=180)
        except subprocess.TimeoutExpired as error:
            records.append(preserve_timeout(args.out, label, command, cwd or HERE,
                                            error, time.perf_counter() - start))
            raise
        seconds = time.perf_counter() - start
        (args.out / f"{label}.stdout").write_text(result.stdout)
        (args.out / f"{label}.stderr").write_text(result.stderr)
        records.append(dict(label=label, command=[str(x) for x in command],
                            cwd=str(cwd or HERE), exit_code=result.returncode,
                            elapsed_seconds=seconds, execution_status="complete" if result.returncode in allowed else "error"))
        if result.returncode not in allowed:
            raise RuntimeError(f"{label} failed with {result.returncode}; see raw output")
    try:
        for label, root, files in [
            ("authored", HERE / "fixtures", [f"{letter}.ts" for letter in "ABCDEF"]),
            ("t3", args.t3, [item["path"] for item in manifest["files"]]),
        ]:
            run(f"{label}-dryer-probe", [args.python, HERE / "upstream_probe.py", "--checkout", args.dryer, "--root", root, *files])
            # Upstream CLI writes its metrics into this isolated output directory.
            run(f"{label}-dryer-cli", [args.python, "-m", "dryer", "--root", args.out, "--format", "edn", *[root / f for f in files]], environment)
            run(f"{label}-independent", ["node", HERE / "detector.mjs", root, *files])
            run(f"{label}-jscpd", [HERE / "node_modules/.bin/jscpd", "--min-lines", "4", "--min-tokens", "50", "--mode", "mild", "--format", "typescript", "--reporters", "json", "--output", args.out / f"{label}-jscpd", "--no-gitignore", "--silent", *[root / f for f in files]])
        diagnostic_root = args.out / "diagnostic"
        diagnostic_root.mkdir()
        (diagnostic_root / "bad.ts").write_bytes((HERE / "fixtures/diagnostic.txt").read_bytes())
        run("diagnostic-dryer-probe", [args.python, HERE / "upstream_probe.py", "--checkout", args.dryer, "--root", diagnostic_root, "bad.ts"], allowed=(2,))
        run("diagnostic-dryer-cli", [args.python, "-m", "dryer", "--root", diagnostic_root, "bad.ts"], environment)
        run("diagnostic-independent", ["node", HERE / "detector.mjs", diagnostic_root, "bad.ts"], allowed=(2,))
        run("diagnostic-jscpd", [HERE / "node_modules/.bin/jscpd", "--min-lines", "4", "--min-tokens", "50", "--mode", "mild", "--format", "typescript", "--reporters", "json", "--output", args.out / "diagnostic-jscpd", "--no-gitignore", "--silent", diagnostic_root / "bad.ts"])
    finally:
        # Keep cleanup evidence on every exit without replacing a tool error.
        final_checks = post_run_checks([(args.dryer, DRYER_REVISION), (args.t3, manifest["revision"])])
        provenance = dict(dryer_revision=DRYER_REVISION, t3_revision=manifest["revision"],
                          post_run_checkouts=final_checks,
                          runtime=dict(python=subprocess.check_output([args.python, "--version"], text=True).strip(),
                                       node=subprocess.check_output(["node", "--version"], text=True).strip(),
                                       python_executable_sha256=digest(args.python),
                                       node_executable_sha256=digest(Path(shutil.which("node"))),
                                       jscpd="4.3.0", typescript="5.9.3",
                                       jscpd_entry_sha256=digest(HERE / "node_modules/.bin/jscpd")),
                          environment="Shared development host; coordinated window excludes other agents' builds/tests/measurements, unrelated desktop processes may run.",
                          measurements="One wall-time observation per invocation; tools perform different checks. No speed comparison.",
                          inputs={str(p.relative_to(HERE)): digest(p) for p in sorted(HERE.rglob('*'))
                                  if p.is_file() and "node_modules" not in p.parts and "evidence" not in p.parts and "__pycache__" not in p.parts},
                          upstream_sources={str(p.relative_to(args.dryer)): digest(p) for p in sorted((args.dryer / "src").rglob('*.py'))},
                          grammar_artifacts={str(p): digest(p) for p in sorted((Path.home() / ".cache/tree-sitter-language-pack/v1.20.0").rglob('*'))
                                             if p.is_file() and p.suffix in [".so", ".json", ".zst"]},
                          commands=records)
        (args.out / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
    if any(item["status"] != "clean" for item in final_checks):
        raise ValueError("post-run checkout verification failed; see provenance")
    return 0

if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(error, file=sys.stderr)
        raise SystemExit(2)
