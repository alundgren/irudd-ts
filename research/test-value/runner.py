#!/usr/bin/env python3
"""Explicit research execution. No runner installation or command discovery."""

import argparse
import ctypes
import errno
import fnmatch
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
REPOSITORY = HERE.parent.parent
MAX_JSON = 8 * 1024 * 1024
DEFAULT_EXCLUDES = [".git", "**/.git", ".git/**", "**/.git/**", "**/node_modules/**", "node_modules/**", "target/**", "**/dist/**", ".repos/**",
                    ".t3/**", "**/.t3/**", ".env", "**/.env", ".env.local", "**/.env.local", ".vite-plus/**", "**/.vite-plus/**"]
UNSETTLED_CHILDREN = []
CANCEL_SIGNAL = None


class ResearchCancelled(KeyboardInterrupt):
    pass


def install_signal_handlers():
    def interrupt(signal_number, _frame):
        global CANCEL_SIGNAL
        CANCEL_SIGNAL = signal_number

    previous = {number: signal.getsignal(number) for number in [signal.SIGINT, signal.SIGTERM]}
    for number in previous:
        signal.signal(number, interrupt)
    return previous


def check_cancelled():
    if CANCEL_SIGNAL is not None:
        raise ResearchCancelled(f"Execution interrupted by signal {CANCEL_SIGNAL}")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def runner_fingerprint():
    return digest(json.dumps([[path.name, digest(path.read_bytes())] for path in sorted(HERE.iterdir())
                              if path.is_file() and path.suffix in {".py", ".ts", ".mjs"}], separators=(",", ":")).encode())


def node_fingerprint(node, environment=None):
    path = shutil.which(node, path=(environment or {}).get("PATH", os.environ.get("PATH", "")))
    if not path:
        raise ValueError("Explicit Node executable is unavailable")
    executable = Path(path).resolve()
    probe = subprocess.run([str(executable), "--input-type=module", "-e",
                            'console.log(JSON.stringify({path:process.execPath,version:process.version}))'],
                           env={"PATH": str(executable.parent)}, capture_output=True, timeout=15)
    if probe.returncode:
        raise ValueError("Explicit Node executable did not report its runtime identity")
    actual = json.loads(probe.stdout)
    if Path(actual["path"]).resolve() != executable or not isinstance(actual.get("version"), str):
        raise ValueError("Configured executable does not directly launch the selected Node runtime")
    return {"path": str(executable), "version": actual["version"], "sha256": digest(executable.read_bytes())}


def runtime_environment(node_identity, configured=None):
    environment = dict(configured or {})
    environment["PATH"] = str(Path(node_identity["path"]).parent) + os.pathsep + environment.get("PATH", os.environ.get("PATH", ""))
    return environment


def direct_vitest_command(config="vite.config.ts"):
    """Explicit copied Vitest route, resolved by execute_case with its pinned Node."""
    return ["{node}", "{vitest}", "run", "--config", config]


def vitest_entrypoint(source, dependencies, node, environment):
    if dependencies is None:
        raise ValueError("Direct Vitest execution requires an owned dependency store")
    script = ('import {createRequire} from "node:module";import fs from "node:fs";import path from "node:path";'
              'const root=createRequire(path.join(process.cwd(),"package.json"));'
              'const context=root.resolve("vite-plus/package.json");'
              'const packagePath=createRequire(context).resolve("vitest/package.json");'
              'const info=JSON.parse(fs.readFileSync(packagePath,"utf8"));'
              'const bin=typeof info.bin==="string"?info.bin:info.bin?.vitest;'
              'if(typeof bin!=="string")throw new Error("Vitest package has no declared entrypoint");'
              'console.log(JSON.stringify({path:fs.realpathSync(path.resolve(path.dirname(packagePath),bin)),'
              'packagePath:fs.realpathSync(packagePath),context:fs.realpathSync(context),'
              'version:info.version,vitePlusVersion:JSON.parse(fs.readFileSync(context,"utf8")).version}));')
    probe = subprocess.run([node, "--input-type=module", "-e", script], cwd=source,
                           env=environment, capture_output=True, timeout=15)
    if probe.returncode:
        raise ValueError("Copied vite-plus context cannot resolve its declared Vitest entrypoint: " + probe.stderr.decode(errors="replace")[:1000])
    record = json.loads(probe.stdout)
    for field in ["path", "packagePath", "context"]:
        inside(Path(record[field]).resolve(), dependencies.root)
    record["sha256"] = digest(Path(record["path"]).read_bytes())
    return record


def read_json(path):
    raw = Path(path).read_bytes()
    if len(raw) > MAX_JSON:
        raise ValueError(f"JSON exceeds byte budget: {path}")

    def unique(pairs):
        value = {}
        for key, item in pairs:
            if key in value:
                raise ValueError(f"Duplicate JSON field: {key}")
            value[key] = item
        return value

    return json.loads(raw.decode("utf-8"), object_pairs_hook=unique)


def disk_check(directory, minimum=0.12):
    usage = shutil.disk_usage(directory)
    if usage.free / usage.total <= minimum:
        raise RuntimeError("DISK STOP: free disk is at or below the 12% research limit")
    return {"freeBytes": usage.free, "totalBytes": usage.total, "freeFraction": usage.free / usage.total}


def guarded_copy(source, destination):
    destination = Path(destination)
    with Path(source).open("rb") as reader, destination.open("wb") as writer:
        while True:
            disk_check(destination.parent)
            chunk = reader.read(1024 * 1024)
            if not chunk:
                break
            writer.write(chunk)
    shutil.copystat(source, destination)
    return str(destination)


def matches(relative, patterns):
    return any(fnmatch.fnmatchcase(relative, pattern) or
               pattern.startswith("**/") and fnmatch.fnmatchcase(relative, pattern[3:]) for pattern in patterns)


def inside(path, root):
    try:
        return path.relative_to(root)
    except ValueError:
        raise ValueError(f"Path escapes configured root: {path}") from None


def snapshot_source(source, destination, includes, excludes):
    manifest = []
    for directory, dirs, files in os.walk(source, followlinks=False):
        disk_check(destination)
        base = Path(directory)
        dirs[:] = sorted(name for name in dirs if name not in {".git", "node_modules", "target"} and
                         not matches((base / name).relative_to(source).as_posix() + "/", excludes))
        for name in dirs:
            if (base / name).is_symlink():
                raise ValueError(f"Source directory symlink requires an explicit owned copy: {base / name}")
        for name in sorted(files):
            path = base / name
            relative = path.relative_to(source).as_posix()
            if not matches(relative, includes) or matches(relative, excludes):
                continue
            if path.is_symlink():
                inside(path.resolve(), source)
            if not path.is_file():
                raise ValueError(f"Source input is not a regular file: {path}")
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            guarded_copy(path, target)
            raw = target.read_bytes()
            manifest.append({"file": relative, "sha256": digest(raw), "bytes": len(raw)})
    if not manifest:
        raise ValueError("Empty configured source snapshot")
    return manifest


def hash_tree(root):
    records = []
    for directory, dirs, files in os.walk(root, followlinks=False):
        base = Path(directory)
        for name in sorted(dirs + files):
            path = base / name
            relative = path.relative_to(root).as_posix()
            if path.is_symlink():
                target = Path(os.path.abspath(path.parent / os.readlink(path)))
                normalized = target.relative_to(root).as_posix() if target.is_relative_to(root) else os.readlink(path)
                records.append([relative, "link", normalized])
            elif path.is_file():
                records.append([relative, "file", digest(path.read_bytes())])
    records.sort()
    return digest(json.dumps(records, separators=(",", ":")).encode()), records


class DependencyStore:
    """One copied external store; source workspace links are recreated per execution."""

    def __init__(self, installed_root, destination):
        self.installed_root = Path(installed_root).resolve()
        self.root = Path(destination).resolve()
        self.modules = []
        self.workspace_links = {}
        self.external_links = {}
        self.owned_directories = set()
        self.manifest = []

    def copy(self):
        self.root.mkdir(parents=True)
        for directory, dirs, _ in os.walk(self.installed_root, followlinks=False):
            dirs[:] = sorted(name for name in dirs if name not in {".git", ".repos", "target", "dist"})
            if Path(directory).name == "node_modules":
                relative = Path(directory).relative_to(self.installed_root)
                self.modules.append(relative.as_posix())
                self._copy_directory(Path(directory), self.root / relative)
                dirs[:] = []
        if not self.modules:
            raise ValueError("Explicit dependency root has no installed node_modules")
        for relative, target in self.external_links.items():
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.symlink_to(self.root / target)
        for relative in self.workspace_links:
            current = Path(relative).parent
            while current != Path("."):
                self.owned_directories.add(current.as_posix())
                current = current.parent
        self.manifest = hash_tree(self.root)[1]
        self.sha256 = self.current_digest()
        return self

    def current_digest(self):
        return digest(json.dumps({"files": hash_tree(self.root)[0], "workspaceLinks": self.workspace_links,
                                  "moduleDirectories": self.modules}, sort_keys=True).encode())

    def _copy_directory(self, source, destination):
        disk_check(self.root)
        destination.mkdir(parents=True, exist_ok=True)
        for item in sorted(source.iterdir()):
            relative = item.relative_to(self.installed_root).as_posix()
            target = destination / item.name
            if item.is_symlink():
                resolved = item.resolve(strict=True)
                resolved_relative = inside(resolved, self.installed_root).as_posix()
                if "node_modules" in Path(resolved_relative).parts:
                    self.external_links[relative] = resolved_relative
                else:
                    self.workspace_links[relative] = resolved_relative
            elif item.is_dir():
                if item.name not in {".cache", ".vite", ".vite-temp", ".vite-plus"}:
                    self._copy_directory(item, target)
            elif item.is_file():
                guarded_copy(item, target)
            else:
                raise ValueError(f"Unsupported dependency input: {item}")

    def attach(self, execution_root):
        execution_root = Path(execution_root).resolve()

        def attach_directory(relative):
            source = self.root / relative
            destination = execution_root / relative
            destination.mkdir(parents=True, exist_ok=True)
            names = {item.name for item in source.iterdir()}
            names.update(Path(link).name for link in self.workspace_links if Path(link).parent.as_posix() == relative)
            for name in sorted(names):
                child = f"{relative}/{name}"
                target = destination / name
                item = self.root / child
                if child in self.workspace_links:
                    workspace = execution_root / self.workspace_links[child]
                    if not workspace.exists():
                        raise ValueError(f"Missing owned workspace dependency: {self.workspace_links[child]}")
                    target.symlink_to(workspace)
                elif item.is_dir() and not item.is_symlink() and (child in self.owned_directories or name == ".bin"):
                    attach_directory(child)
                elif relative.endswith("/.bin") and item.is_file() and not item.is_symlink():
                    raw = item.read_bytes().replace(str(self.installed_root).encode(), str(execution_root).encode())
                    target.write_bytes(raw)
                    shutil.copymode(item, target)
                else:
                    target.symlink_to(item.resolve())

        for relative in self.modules:
            attach_directory(relative)


class BsdInfo(ctypes.Structure):
    _fields_ = [(name, ctypes.c_uint32) for name in
                ["flags", "status", "xstatus", "pid", "ppid", "uid", "gid", "ruid", "rgid", "svuid", "svgid", "reserved"]] + [
                    ("comm", ctypes.c_char * 16), ("name", ctypes.c_char * 32)] + [
                        (name, ctypes.c_uint32) for name in ["nfiles", "pgid", "jobc", "tdev", "tpgid"]] + [
                            ("nice", ctypes.c_int32), ("startSeconds", ctypes.c_uint64), ("startMicroseconds", ctypes.c_uint64)]


def group_has_live_members(group):
    if sys.platform == "darwin":
        library = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
        library.proc_listpgrppids.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_int]
        library.proc_listpgrppids.restype = ctypes.c_int
        library.proc_pidinfo.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64, ctypes.c_void_p, ctypes.c_int]
        library.proc_pidinfo.restype = ctypes.c_int
        members = (ctypes.c_int * 4096)()
        ctypes.set_errno(0)
        count = library.proc_listpgrppids(group, members, ctypes.sizeof(members))
        if count < 0 or count == 0 and ctypes.get_errno():
            raise RuntimeError("Process-group inventory unavailable")
        if count >= len(members):
            raise RuntimeError("Process-group inventory exceeds member budget")
        for pid in members[:count]:
            if pid <= 0:
                continue
            info = BsdInfo()
            ctypes.set_errno(0)
            size = library.proc_pidinfo(pid, 3, 0, ctypes.byref(info), ctypes.sizeof(info))
            if size != ctypes.sizeof(info):
                if ctypes.get_errno() == errno.ESRCH:
                    continue
                raise RuntimeError(f"Process-group member observation unavailable: {size} bytes, errno {ctypes.get_errno()}")
            if info.pgid == group and info.status != 5:
                return True
        return False
    if sys.platform.startswith("linux"):
        for index, entry in enumerate(Path("/proc").iterdir()):
            if index >= 65536:
                raise RuntimeError("Process inventory exceeds entry budget")
            if not entry.name.isdigit():
                continue
            try:
                if os.getpgid(int(entry.name)) != group:
                    continue
                with (entry / "stat").open("r") as descriptor:
                    record = descriptor.read(4097)
            except OSError as error:
                if error.errno in {errno.ESRCH, errno.ENOENT}:
                    continue
                raise RuntimeError("Process-group member observation unavailable") from error
            if len(record) > 4096 or ")" not in record:
                raise RuntimeError("Invalid process-group member record")
            fields = record[record.rindex(")") + 1:].split()
            if len(fields) < 3 or not fields[2].isdigit():
                raise RuntimeError("Invalid process-group member fields")
            if int(fields[2]) == group and fields[0] not in {"Z", "X"}:
                return True
        return False
    raise RuntimeError("Process-group observation requires Linux or macOS")


class OwnedChild:
    """Reserve the leader until group observation settles, including after exit."""

    def __init__(self, process):
        self.process = process
        self.pid = process.pid
        self.owned = True
        self.finished = False

    def exited(self):
        if not self.owned:
            raise RuntimeError("Child ownership lost; numeric cleanup suppressed")
        try:
            status = os.waitid(os.P_PID, self.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)
        except OSError as error:
            self.owned = False
            if error.errno != errno.ECHILD:
                UNSETTLED_CHILDREN.append(self.process)
            raise RuntimeError("Child ownership lost or unavailable; numeric cleanup suppressed") from error
        return status is not None and status.si_pid == self.pid

    def signal_group(self, signal_number):
        self.exited()
        try:
            os.killpg(self.pid, signal_number)
        except ProcessLookupError:
            pass
        except PermissionError:
            # Darwin refuses a signal when the reserved leader is the only zombie.
            if not self.exited() or group_has_live_members(self.pid):
                raise

    def finish(self, observe=group_has_live_members, grace=5):
        self.finished = True
        try:
            self.exited()
            residual = observe(self.pid)
            if residual:
                self.signal_group(signal.SIGKILL)
            deadline = time.monotonic() + grace
            while True:
                exited = self.exited()
                if exited and not observe(self.pid):
                    self.exited()
                    pid, status = os.waitpid(self.pid, 0)
                    if pid != self.pid:
                        raise RuntimeError("Owned child could not be reaped")
                    self.process.returncode = os.waitstatus_to_exitcode(status)
                    self.owned = False
                    return self.process.returncode, residual
                if time.monotonic() >= deadline:
                    raise RuntimeError("Process-group cleanup exceeded its deadline")
                self.signal_group(signal.SIGKILL)
                time.sleep(0.01)
        except BaseException:
            # No numeric signals follow uncertain observation. Retain the Popen
            # reference so its destructor cannot release the reserved leader.
            self.owned = False
            UNSETTLED_CHILDREN.append(self.process)
            raise


def run_command(command, cwd, environment, evidence, timeout=60, max_output=4 * 1024 * 1024):
    previous = install_signal_handlers()
    try:
        return _run_command(command, cwd, environment, evidence, timeout, max_output)
    finally:
        for number, handler in previous.items():
            signal.signal(number, handler)


def _run_command(command, cwd, environment, evidence, timeout, max_output):
    evidence.mkdir(parents=True, exist_ok=True)
    start = time.monotonic()
    status = "finished"
    residual_group = False
    process = None
    child = None
    cleanup_errors = []
    exit_code = None
    cleanup_complete = True
    pending_error = None
    with (evidence / "stdout.txt").open("wb") as stdout, (evidence / "stderr.txt").open("wb") as stderr:
        try:
            disk_check(cwd)
            check_cancelled()
            if signal.getsignal(signal.SIGCHLD) != signal.SIG_DFL:
                raise RuntimeError("Child ownership requires the default SIGCHLD disposition")
            # Signal handlers only record cancellation until ownership is registered.
            process = subprocess.Popen(command, cwd=cwd, env=environment, stdout=stdout, stderr=stderr, start_new_session=True)
            child = OwnedChild(process)
            cleanup_complete = False
            check_cancelled()
            while not child.exited():
                disk_check(cwd)
                check_cancelled()
                if time.monotonic() - start > timeout:
                    status = "timeout"
                    break
                if stdout.tell() + stderr.tell() > max_output:
                    status = "outputLimit"
                    break
                time.sleep(0.1)
        except BaseException as error:
            pending_error = error
            status = "cancelled" if isinstance(error, KeyboardInterrupt) else "monitorError"
            if process is not None and child is None:
                child = OwnedChild(process)
                cleanup_complete = False
        if child is not None:
            try:
                exit_code, residual_group = child.finish()
                cleanup_complete = True
            except BaseException as error:
                status = "cleanupUncertain"
                cleanup_errors.append(str(error))
                if pending_error is None and isinstance(error, (KeyboardInterrupt, SystemExit)):
                    pending_error = error
    if status == "finished" and residual_group:
        status = "residualProcessGroup"
    stdout_bytes = (evidence / "stdout.txt").stat().st_size
    stderr_bytes = (evidence / "stderr.txt").stat().st_size
    if status == "finished" and stdout_bytes + stderr_bytes > max_output:
        status = "outputLimit"
    try:
        disk_check(cwd)
    except RuntimeError as error:
        status = "diskStop"
        if pending_error is None:
            pending_error = error
    if pending_error is None and CANCEL_SIGNAL is not None:
        pending_error = ResearchCancelled(f"Execution interrupted by signal {CANCEL_SIGNAL}")
        if cleanup_complete:
            status = "cancelled"
    result = {"exitCode": exit_code, "status": status, "durationMs": (time.monotonic() - start) * 1000,
            "command": command, "cwd": str(cwd), "residualProcessGroup": residual_group, "cleanupErrors": cleanup_errors,
            "stdoutBytes": stdout_bytes, "stderrBytes": stderr_bytes, "cleanupComplete": cleanup_complete,
            "capturedPid": process.pid if process is not None else None}
    if pending_error is not None:
        pending_error.command_result = result
        raise pending_error
    return result


def private_environment(directory, configured=None):
    environment = {"PATH": os.environ.get("PATH", ""), "LANG": "en_US.UTF-8", "TZ": "UTC", "CI": "1"}
    environment.update(configured or {})
    for key, name in [("HOME", "home"), ("TMPDIR", "tmp"), ("TMP", "tmp"), ("TEMP", "tmp"),
                      ("XDG_CACHE_HOME", "cache"), ("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"),
                      ("npm_config_cache", "npm-cache")]:
        destination = directory / name
        destination.mkdir(parents=True, exist_ok=True)
        environment[key] = str(destination)
    return environment


def validate_inventory(inventory, protocol, request, raw_exit, node_identity):
    problems = list(inventory.get("problems", []))
    if inventory.get("runtime") != node_identity:
        problems.append("Reporter runtime disagrees with the selected Node path, version or hash")
    for field in ["schemaVersion", "requestId", "runId", "inputDigest"]:
        if inventory.get(field) != request[field]:
            problems.append(f"Inventory {field} disagrees with request")
    if inventory.get("protocolSha256") != digest(Path(request["resultPath"]).read_bytes()):
        problems.append("Inventory hash disagrees with final SDK result")
    if protocol["exitCode"] != raw_exit:
        problems.append("SDK exit code disagrees with actual process status")
    if not protocol["complete"] or protocol["reason"] != "finished":
        problems.append("SDK lifecycle is incomplete")
    tests = inventory.get("tests", [])
    if not isinstance(tests, list) or not tests:
        return [], problems + ["Empty or invalid active test inventory"]
    ids = set()
    raw_ids = set()
    counts = {"passed": 0, "failed": 0, "skipped": 0}
    by_raw = {}
    for test in tests:
        if not isinstance(test, dict) or not isinstance(test.get("id"), str) or not isinstance(test.get("rawId"), str):
            problems.append("Invalid individual test identity")
            continue
        if test["id"] in ids or test["rawId"] in raw_ids:
            problems.append("Duplicate stable or raw test ID")
        ids.add(test["id"]); raw_ids.add(test["rawId"]); by_raw[test["rawId"]] = test
        state = test.get("state")
        if state in counts:
            counts[state] += 1
        if state not in {"passed", "failed"}:
            problems.append(f"Test did not finish actively: {test['id']}")
        if test.get("readyCount") != 1 or test.get("resultCount") != 1:
            problems.append(f"Test did not execute exactly once: {test['id']}")
        if test.get("retryCount") != 0 or test.get("repeatCount") != 0:
            problems.append(f"Retry/repetition metadata unavailable or nonzero: {test['id']}")
    if counts != protocol["tests"]:
        problems.append("SDK aggregate counts disagree with individual inventory")
    failures = protocol["failures"]
    assertion_ids = set()
    for failure in failures:
        if failure["kind"] != "assertion":
            problems.append(f"Infrastructure failure: {failure['kind']}: {failure['message']}")
        elif failure["testId"] not in by_raw or by_raw[failure["testId"]].get("state") != "failed":
            problems.append("SDK assertion ID has no failed inventory test")
        else:
            assertion_ids.add(failure["testId"])
    for test in tests:
        if test.get("state") == "failed" and test.get("rawId") not in assertion_ids:
            problems.append("Failed test lacks an SDK assertion")
        if test.get("state") == "passed" and test.get("errors"):
            problems.append("Passing test retained error metadata")
    if raw_exit != 0 and not assertion_ids and not problems:
        problems.append("Nonzero command status has no assertion evidence")
    return tests, problems


def execute_case(template, output, command, *, dependencies=None, baseline=None, mutation=None,
                 cwd=".", environment=None, timeout=60, input_digest=None, run_id=None, node="node", keep_source=False,
                 import_controls=None, captured_artifacts=None, expected_node=None):
    """Run one explicit fresh copy. Returns records and complete/unknown classification.

    Historical callers can pass their fixed inventory as baseline and a separate
    reduced-reversion template. The caller owns revision and replay provenance.
    """
    template = Path(template).resolve()
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = output / "source"
    disk_check(output)
    shutil.copytree(template, source, symlinks=False, copy_function=guarded_copy)
    if dependencies:
        dependencies.attach(source)
    source_digest = hash_tree(template)[0]
    configured_node = node
    node_identity = node_fingerprint(node, environment)
    node = node_identity["path"]
    environment = runtime_environment(node_identity, environment)
    env = private_environment(output / "private", environment)
    vitest = vitest_entrypoint(source, dependencies, node, env) if any("{vitest}" in str(argument) for argument in command) else None
    runner_digest = runner_fingerprint()
    input_digest = input_digest or digest(json.dumps({"source": source_digest,
        "dependencies": dependencies.sha256 if dependencies else None, "runner": runner_digest,
        "sdk": hash_tree(REPOSITORY / "sdk")[0], "command": command, "cwd": cwd,
        "environment": environment, "node": node_identity,
        "vitest": {key: value for key, value in (vitest or {}).items() if key not in {"path", "packagePath", "context"}}}, sort_keys=True).encode())
    result_path = output / "result.json"
    request_path = output / "request.json"
    request = {"schemaVersion": 1, "requestId": uuid.uuid4().hex, "runId": run_id or uuid.uuid4().hex,
               "inputDigest": input_digest, "phase": "mutation" if mutation else "baseline",
               "mutationId": mutation["id"] if mutation else None, "resultPath": str(result_path), "maxResultBytes": MAX_JSON}
    write_json(request_path, request)
    env.update(ARCHGUARD_MUTATION_REQUEST=str(request_path), ARCHGUARD_MUTATION_RESULT=str(result_path),
               ARCHGUARD_RESEARCH_ROOT=str(source), ARCHGUARD_RESEARCH_INVENTORY=str(output / "inventory.json"),
               ARCHGUARD_RESEARCH_NODE=json.dumps(node_identity, sort_keys=True))
    replacements = {"root": str(source), "reporter": str(HERE / "vitest-reporter.ts"),
                    "nodeReporter": str(HERE / "node-reporter.mjs"), "sdk": str(REPOSITORY / "sdk"), "node": node,
                    "vitest": vitest["path"] if vitest else ""}
    actual_command = [str(argument).format_map(replacements) for argument in command]
    if actual_command and actual_command[0] == configured_node:
        actual_command[0] = node
    actual_cwd = source / cwd
    inside(actual_cwd.resolve(), source)
    evidence = {"templateSha256": source_digest, "mutation": mutation, "request": request,
                "baselineIds": [test["id"] for test in baseline] if baseline is not None else None,
                "node": node_identity, "expectedNode": expected_node or node_identity,
                "vitest": vitest, "runnerSha256": runner_digest,
                "dependencySha256": dependencies.sha256 if dependencies else None, "cleanupComplete": True}
    try:
        if mutation:
            apply_mutation(source, mutation)
        resolutions = []
        for control in import_controls or []:
            specifier = control["specifier"]
            script = ('import fs from "node:fs";import {fileURLToPath} from "node:url";'
                      'const resolved=import.meta.resolve(process.argv[1]);'
                      'console.log(JSON.stringify({specifier:process.argv[1],resolved,'
                      'realPath:resolved.startsWith("file:")?fs.realpathSync(fileURLToPath(resolved)):null}));')
            resolved = subprocess.run([node, "--input-type=module", "-e", script, specifier], cwd=actual_cwd,
                                      env=env, capture_output=True, timeout=15)
            if resolved.returncode:
                raise ValueError(f"Configured import resolution failed: {specifier}")
            record = json.loads(resolved.stdout)
            if control.get("expectedWorkspacePath"):
                expected = (source / control["expectedWorkspacePath"]).resolve()
                if record["realPath"] != str(expected):
                    raise ValueError(f"Workspace import resolved outside this execution: {specifier}")
            if record["realPath"] and dependencies:
                path = Path(record["realPath"])
                if not path.is_relative_to(source) and not path.is_relative_to(dependencies.root):
                    raise ValueError(f"Configured import resolved outside owned inputs: {specifier}")
            resolutions.append(record)
        evidence["importControls"] = resolutions
        evidence["cleanupComplete"] = False
        raw = run_command(actual_command, actual_cwd, env, output, timeout)
        evidence.update(raw)
        records = []
        problems = []
        protocol = None
        if raw["status"] != "finished":
            problems.append(f"Command did not finish: {raw['status']}")
            problems.extend(raw.get("cleanupErrors", []))
        else:
            try:
                read_json(result_path)
                validation = subprocess.run([node, str(HERE / "protocol-check.ts")], env=env, capture_output=True, timeout=15)
                (output / "protocol-validation.stderr.txt").write_bytes(validation.stderr)
                if validation.returncode != 0:
                    raise ValueError("Final SDK protocol validation failed")
                protocol = json.loads(validation.stdout)
                inventory = read_json(output / "inventory.json")
                records, problems = validate_inventory(inventory, protocol, request, raw["exitCode"], expected_node or node_identity)
                if expected_node is not None and node_identity != expected_node:
                    problems.append("Selected Node runtime changed from the frozen execution identity")
            except (OSError, ValueError, subprocess.SubprocessError) as error:
                problems.append(str(error))
        if baseline is not None:
            expected = {test["id"] for test in baseline}
            observed = {test["id"] for test in records}
            if expected != observed:
                problems.append(f"Inventory mismatch: {len(expected-observed)} missing and {len(observed-expected)} extra tests")
        complete = not problems and bool(records)
        outcomes = {test["id"]: "unknown" for test in baseline or records}
        if complete:
            outcomes = {test["id"]: "killed" if test["state"] == "failed" else "notKilled" for test in records}
        status = "timeout" if raw["status"] == "timeout" else "error" if not complete else "killed" if "killed" in outcomes.values() else "survived"
        evidence.update(complete=complete, tests=records, outcomes=outcomes, status=status, infrastructureErrors=problems, protocol=protocol)
    except (ResearchCancelled, KeyboardInterrupt) as error:
        evidence.update(getattr(error, "command_result", {}))
        evidence.update(complete=False, tests=[], outcomes={test["id"]: "unknown" for test in baseline or []},
                        status="cancelled", infrastructureErrors=[str(error) or "Execution interrupted"] + evidence.get("cleanupErrors", []))
        raise
    except (OSError, ValueError, RuntimeError) as error:
        evidence.update(getattr(error, "command_result", {}))
        evidence.update(complete=False, tests=[], outcomes={test["id"]: "unknown" for test in baseline or []},
                        status="error",
                        infrastructureErrors=[str(error)] + evidence.get("cleanupErrors", []))
    finally:
        for relative in captured_artifacts or []:
            path = source / relative
            inside(path.resolve(), source)
            if path.is_file():
                target = output / "artifacts" / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, target)
        write_json(output / "execution.json", evidence)
        if not keep_source and evidence.get("cleanupComplete") is True:
            shutil.rmtree(source)
    return evidence


def apply_mutation(root, mutation):
    location = mutation["location"]
    source = root / location["file"]
    inside(source.resolve(), root)
    raw = source.read_bytes()
    if digest(raw) != mutation["sourceSha256"]:
        raise ValueError("Mutation source hash disagrees with owned source")
    start, end = location["start"], location["end"]
    if not isinstance(start, int) or not isinstance(end, int) or not 0 <= start < end <= len(raw):
        raise ValueError("Mutation byte range is invalid")
    if raw[start:end] != mutation["expected"].encode():
        raise ValueError("Mutation expected bytes disagree with owned source")
    changed = raw[:start] + mutation["replacement"].encode() + raw[end:]
    changed.decode("utf-8")
    source.write_bytes(changed)


def run_matrix(config, output):
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    disk_check(output)
    source = Path(config["sourceRoot"]).resolve()
    template = output / "template"
    template.mkdir()
    manifest = snapshot_source(source, template, config.get("workspaceInclude", ["**"]),
                               DEFAULT_EXCLUDES + config.get("workspaceExclude", []))
    write_json(output / "inputs.json", manifest)
    dependencies = None
    if config.get("dependencyRoot"):
        dependencies = DependencyStore(config["dependencyRoot"], output / "dependencies").copy()
        write_json(output / "dependencies.json", {"sha256": dependencies.sha256, "files": dependencies.manifest,
                   "workspaceLinks": dependencies.workspace_links, "moduleDirectories": dependencies.modules})
    archguard = Path(config["archguard"]).resolve()
    plan_config = config.get("planConfig")
    stored_plan = read_json(config["planPath"]) if plan_config is None else None
    if stored_plan is not None:
        plan_config = stored_plan["configuration"]
        shutil.copy2(config["planPath"], output / "supplied-plan.json")
    write_json(output / "plan-config.json", plan_config)
    process = subprocess.run([str(archguard), "mutator", "plan", "--root", str(template), "--config", str(output / "plan-config.json"), "--json"],
                             capture_output=True, timeout=60)
    (output / "plan.stderr.txt").write_bytes(process.stderr)
    if process.returncode:
        raise ValueError("Archguard mutation planning failed")
    (output / "plan.json").write_bytes(process.stdout)
    plan = read_json(output / "plan.json")
    if plan.get("schemaVersion") != 1 or not plan.get("complete"):
        raise ValueError("Mutation plan is incomplete or unsupported")
    if stored_plan is not None and any(stored_plan.get(field) != plan[field]
                                       for field in ["schemaVersion", "operatorVersion", "configuration", "files", "sites", "complete"]):
        raise ValueError("Supplied mutation plan disagrees with fresh Archguard plan")
    runner_digest = runner_fingerprint()
    node_identity = node_fingerprint(config.get("node", "node"), config.get("environment"))
    inputs_digest = digest(json.dumps({"inputs": manifest, "config": config, "dependencies": dependencies.sha256 if dependencies else None,
                                      "archguard": digest(archguard.read_bytes()), "runner": runner_digest,
                                      "sdk": hash_tree(REPOSITORY / "sdk")[0],
                                      "node": node_identity,
                                      "environment": runtime_environment(node_identity, config.get("environment"))}, sort_keys=True).encode())
    options = dict(dependencies=dependencies, cwd=config.get("cwd", "."), environment=config.get("environment"),
                   timeout=config.get("timeoutSeconds", 60), input_digest=inputs_digest, run_id=uuid.uuid4().hex,
                   node=config.get("node", "node"), keep_source=config.get("keepSource", False),
                   expected_node=node_identity,
                   import_controls=config.get("importControls"), captured_artifacts=config.get("capturedArtifacts"))
    baseline = execute_case(template, output / "baseline", config["command"], **options)
    inventory = baseline["tests"]
    baseline_complete = baseline["complete"] and baseline["status"] == "survived"
    cleanup_blocked = baseline.get("cleanupComplete") is not True or bool(baseline.get("cleanupErrors"))
    result = {"schemaVersion": 1, "subject": config["subject"], "baselineComplete": baseline_complete,
              "tests": [{key: test[key] for key in ["id", "name", "file", "project", "durationMs"] if key in test} for test in inventory],
              "mutants": [], "provenance": {"inputDigest": inputs_digest, "runnerSha256": runner_digest,
                "archguardSha256": digest(archguard.read_bytes()), "dependencySha256": dependencies.sha256 if dependencies else None,
                "planSha256": digest((output / "plan.json").read_bytes()), "configuration": config},
              "completeness": {"baseline": baseline, "plannedMutants": len(plan["sites"]), "mutationLimit": config.get("mutantLimit"),
                               "selectedClosureOnly": True}}
    selected = plan["sites"][:config.get("mutantLimit", len(plan["sites"]))]
    for index, mutation in enumerate(selected):
        if baseline_complete and not cleanup_blocked:
            execution = execute_case(template, output / f"mutant-{index:05d}", config["command"], baseline=inventory, mutation=mutation, **options)
            result["mutants"].append({"id": mutation["id"], "status": execution["status"], "outcomes": execution["outcomes"],
                                       "infrastructureErrors": execution["infrastructureErrors"], "mutation": mutation,
                                       "durationMs": execution.get("durationMs"), "evidence": f"mutant-{index:05d}/execution.json"})
            cleanup_blocked = execution.get("cleanupComplete") is not True or bool(execution.get("cleanupErrors"))
        else:
            result["mutants"].append({"id": mutation["id"], "status": "notRun", "outcomes": {test["id"]: "unknown" for test in inventory},
                                       "infrastructureErrors": ["Process cleanup is unconfirmed; further execution stopped" if cleanup_blocked else
                                                                "Fresh baseline did not pass completely"], "mutation": mutation})
        write_json(output / "matrix.json", result)
        print(f"{index+1}/{len(selected)} {result['mutants'][-1]['status']}", file=sys.stderr, flush=True)
    result["completeness"]["cleanupConfirmed"] = not cleanup_blocked
    if dependencies and not cleanup_blocked:
        final_digest = dependencies.current_digest()
        result["completeness"]["finalDependencySha256"] = final_digest
        if final_digest != dependencies.sha256:
            result["baselineComplete"] = False
            result["completeness"]["dependencyMutation"] = True
            for mutant in result["mutants"]:
                mutant.update(status="error", outcomes={test["id"]: "unknown" for test in inventory})
                mutant["infrastructureErrors"].append("Copied external dependency store changed during execution")
    write_json(output / "matrix.json", result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", choices=["run"])
    parser.add_argument("--config", required=True)
    parser.add_argument("--output", required=True)
    arguments = parser.parse_args()
    previous = install_signal_handlers()
    try:
        result = run_matrix(read_json(arguments.config), arguments.output)
        return 0 if result["baselineComplete"] else 2
    except ResearchCancelled:
        return 130
    finally:
        for number, handler in previous.items():
            signal.signal(number, handler)


if __name__ == "__main__":
    sys.exit(main())
