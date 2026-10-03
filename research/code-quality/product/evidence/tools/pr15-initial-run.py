import datetime, hashlib, json, os, pathlib, subprocess, sys, tarfile
root = pathlib.Path(__file__).resolve().parent
container = "archguard-pr15-acceptance"
records = []
source_revision = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()

def run(label, args, cwd="/work/archguard", expect=0):
    command = ["docker", "exec", "-w", cwd, container, *args]
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    print("Starting " + label, flush=True)
    log = root / (label + ".log")
    with log.open("wb") as output:
        result = subprocess.run(command, stdout=output, stderr=subprocess.STDOUT)
    record = {"label": label, "command": command, "startedAt": started,
              "finishedAt": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "exitCode": result.returncode, "expectedExitCode": expect,
              "log": log.name, "logSha256": hashlib.sha256(log.read_bytes()).hexdigest()}
    records.append(record)
    (root / "commands.json").write_text(json.dumps({"sourceRevision": source_revision, "commands": records}, indent=2)+"\n")
    print(label + " exit=" + str(result.returncode), flush=True)
    if result.returncode != expect:
        print(log.read_text(errors="replace")[-8000:], flush=True)
        sys.exit(1)

run("environment", ["sh", "-c", "rustc --version; cargo --version; node --version; python3 --version; uname -a; cat /proc/meminfo | head -3; cat /sys/fs/cgroup/cpu.max; cat /sys/fs/cgroup/memory.max"])
run("release-build", ["cargo", "build", "--release", "--locked", "--bins", "--examples"])
run("release-pressure-compile", ["cargo", "test", "--release", "--locked", "--test", "mutation_execution", "--no-run"])
run("release-pressure", ["cargo", "test", "--release", "--locked", "--test", "mutation_execution", "repeated_linux_execution_resources_remain_bounded", "--", "--exact", "--ignored", "--nocapture", "--test-threads=1"])
run("package", ["cargo", "package", "--locked", "--allow-dirty", "--no-verify"])
run("package-copy", ["cp", "target/package/archguard-0.1.0.crate", "/evidence/archguard-0.1.0.crate"])
archive = root / "archguard-0.1.0.crate"
with tarfile.open(archive) as tar:
    names = tar.getnames()
    required = ["sdk/mutator.ts", "sdk/mutator-vitest-reporter.ts", "src/subprocess.rs", "tests/mutator_reporter.test.ts", "examples/code-quality/prepare-domain.ts", "docs/code-quality.md"]
    assert all("archguard-0.1.0/"+name in names for name in required)
    assert not any("/node_modules/" in name or "/research/" in name or "/target/" in name or "/.git/" in name for name in names)
    assert all(name.startswith("archguard-0.1.0/") and ".." not in pathlib.PurePosixPath(name).parts for name in names)
    assert all(member.isfile() for member in tar.getmembers())
    manifest = tar.extractfile("archguard-0.1.0/Cargo.toml").read().decode()
(root / "package-inventory.json").write_text(json.dumps({"sourceRevision":source_revision,"packageSha256":hashlib.sha256(archive.read_bytes()).hexdigest(),"entries":len(names),"files":names,"normalizedManifest":manifest},indent=2)+"\n")
run("package-extract", ["sh", "-c", "mkdir /work/extracted && tar -xf /evidence/archguard-0.1.0.crate -C /work/extracted"])
extracted = "/work/extracted/archguard-0.1.0"
run("extracted-release-build", ["env", "CARGO_TARGET_DIR=/work/archguard/target", "cargo", "build", "--release", "--locked", "--bins", "--examples"], cwd=extracted)
run("extracted-rust-tests", ["env", "CARGO_TARGET_DIR=/work/archguard/target", "cargo", "test", "--locked"], cwd=extracted)
run("extracted-node-tests", ["node", "--test", "tests/mutator_sdk.test.ts", "tests/mutator_reporter.test.ts", "tests/quality_examples.test.ts"], cwd=extracted)
run("extracted-domain-prepare", ["node", "examples/code-quality/prepare-domain.ts", "/work/domain-profiles"], cwd=extracted)
for label, args in [
    ("packaged-dryer", ["dryer", "--root", extracted, "--config", "examples/code-quality/dryer.json", "--json"]),
    ("packaged-plan", ["mutator", "plan", "--root", extracted, "--config", "examples/code-quality/plan.json", "--json"]),
    ("packaged-domain-weak", ["mutator", "run", "--root", extracted, "--config", "/work/domain-profiles/weak.json", "--json"]),
    ("packaged-domain-strong", ["mutator", "run", "--root", extracted, "--config", "/work/domain-profiles/strong.json", "--json"])]:
    run(label, ["/work/archguard/target/release/archguard", *args], cwd=extracted)
print("All acceptance commands passed", flush=True)
