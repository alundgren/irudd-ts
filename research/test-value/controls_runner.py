#!/usr/bin/env python3
"""Real runner failure controls. All inputs and outputs are owned copies."""

import argparse
import errno
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
from unittest.mock import patch

import runner as execution_runner
from runner import DependencyStore, OwnedChild, direct_vitest_command, execute_case, group_has_live_members, install_signal_handlers, private_environment, run_command, run_matrix, write_json


def fixture(directory, files):
    directory.mkdir(parents=True)
    for relative, text in files.items():
        destination = directory / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text)
    return directory


def require(value, message):
    if not value:
        raise AssertionError(message)


def ownership_controls(output):
    directory = output / "ownership"
    directory.mkdir()
    environment = private_environment(directory / "private")
    completed = run_command(["/bin/sh", "-c", "exit 7"], directory, environment, directory / "exited")
    require(completed["exitCode"] == 7 and completed["status"] == "finished", "Reserved zombie leader must finish without signaling another process")
    late = run_command(["/bin/sh", "-c", 'sleep 30 & printf "%s\\n" "$!"; exit 0'], directory, environment, directory / "late-descendant")
    require(late["status"] == "residualProcessGroup" and late["residualProcessGroup"], "Live descendant must be observed and cleaned before reaping leader")
    descendant = int((directory / "late-descendant/stdout.txt").read_text().strip())
    try:
        current_group = os.getpgid(descendant)
    except ProcessLookupError:
        current_group = None
    require(current_group is None or not group_has_live_members(current_group), "Late descendant must not remain alive")
    timeout = run_command(["/bin/sh", "-c", "exec sleep 30"], directory, environment, directory / "timeout", timeout=0.15)
    write_json(directory / "timeout/execution.json", timeout)
    require(timeout["status"] == "timeout", f"Deadline must clean an owned live group: {timeout}")
    oversized = run_command([sys.executable, "-c", "import sys;sys.stdout.buffer.write(b'x'*65536)"], directory, environment,
                            directory / "oversized", max_output=1024)
    require(oversized["status"] == "outputLimit", "Fast output must also meet the final output budget")

    helper = directory / "interrupt-helper.py"
    helper.write_text('import pathlib,sys\nsys.path.insert(0,' + repr(str(Path(__file__).resolve().parent)) + ')\n'
                      'from runner import ResearchCancelled,install_signal_handlers,private_environment,run_command\n'
                      'root=pathlib.Path(sys.argv[1]);install_signal_handlers()\n'
                      'try:run_command(["/bin/sh","-c",\'printf "%s\\\\n" "$$"; exec sleep 30\'],root,private_environment(root/"private"),root/"child")\n'
                      'except ResearchCancelled:sys.exit(130)\n')
    interruption = directory / "interruption"
    interruption.mkdir()
    process = subprocess.Popen([sys.executable, str(helper), str(interruption)], start_new_session=True)
    child = OwnedChild(process)
    ready = interruption / "child/stdout.txt"
    deadline = time.monotonic() + 3
    while not ready.exists() or not ready.read_text().strip():
        require(time.monotonic() < deadline and not child.exited(), "Interrupted helper must launch its owned child")
        time.sleep(0.01)
    child_pid = int(ready.read_text().strip())
    require(not child.exited(), "Helper must retain ownership before interruption")
    os.kill(child.pid, signal.SIGTERM)
    deadline = time.monotonic() + 3
    while not child.exited():
        require(time.monotonic() < deadline, "Interrupted helper must finish bounded cleanup")
        time.sleep(0.01)
    require(child.finish()[0] == 130 and not group_has_live_members(child_pid), "SIGTERM must clean the nested owned group before exiting")

    spawn_helper = directory / "spawn-interruption-helper.py"
    spawn_helper.write_text('import os,pathlib,signal,subprocess,sys,time\nfrom unittest.mock import patch\n'
        'sys.path.insert(0,' + repr(str(Path(__file__).resolve().parent)) + ')\n'
        'from runner import ResearchCancelled,group_has_live_members,private_environment,run_command\n'
        'root=pathlib.Path(sys.argv[1]);original=subprocess.Popen;captured=[]\n'
        'def spawn(*arguments,**keywords):\n'
        ' child=original(*arguments,**keywords);captured.append(child);deadline=time.monotonic()+3\n'
        ' while keywords["stdout"].tell()==0:\n'
        '  assert time.monotonic()<deadline;time.sleep(0.001)\n'
        ' os.kill(os.getpid(),signal.SIGTERM);return child\n'
        'with patch("runner.subprocess.Popen",spawn):\n'
        ' try:run_command([sys.executable,"-c",' + repr('import time;print("ready",flush=True);time.sleep(30)') + '],root,private_environment(root/"private"),root/"child")\n'
        ' except ResearchCancelled as error:\n'
        '  assert error.command_result["cleanupComplete"] is True,error.command_result\n'
        '  assert error.command_result["status"] == "cancelled"\n'
        '  assert not group_has_live_members(captured[0].pid)\n'
        ' else:raise AssertionError("Spawn interruption was ignored")\n'
        'print("spawn interruption settled after ownership registration")\n')
    spawn_directory = directory / "spawn-interruption"
    spawn_directory.mkdir()
    spawned = run_command([sys.executable, str(spawn_helper), str(spawn_directory)], spawn_directory, environment,
                          directory / "spawn-interruption-evidence")
    require(spawned["status"] == "finished" and spawned["exitCode"] == 0, "Spawn-boundary cancellation must settle captured child ownership")

    live_template = fixture(directory / "uncertain-template", {"case.test.mjs": "// Owned retention control\n"})
    original_finish = OwnedChild.finish
    captured = []
    checks = 0

    def fail_monitor(_directory, minimum=0.12):
        nonlocal checks
        checks += 1
        if checks >= 5:
            raise RuntimeError("Injected command monitor failure")
        return {}

    def uncertain_finish(child, observe=group_has_live_members, grace=5):
        captured.append(child.process)

        def unavailable(_group):
            raise RuntimeError("Injected unavailable live-child inventory")

        return original_finish(child, observe=unavailable, grace=grace)

    with patch("runner.disk_check", side_effect=fail_monitor), patch.object(OwnedChild, "finish", uncertain_finish):
        result = execute_case(live_template, directory / "uncertain-live-child", ["/bin/sh", "-c", "exec sleep 30"])
    require(not result["complete"] and result["cleanupComplete"] is False and result["cleanupErrors"], "Uncertain exceptional cleanup must preserve incomplete evidence")
    require((directory / "uncertain-live-child/source/case.test.mjs").is_file(), "Live-child uncertainty must retain source and artifacts")
    require(len(captured) == 1 and group_has_live_members(captured[0].pid), "Uncertainty control must observe a real live child")
    # The control restores the real observer before cleaning its captured child.
    require(OwnedChild(captured[0]).finish()[0] < 0, "Control teardown must settle the actually owned group")

    process = subprocess.Popen(["/bin/sh", "-c", "exit 0"], start_new_session=True)
    child = OwnedChild(process)
    pid, status = os.waitpid(process.pid, 0)
    process.returncode = os.waitstatus_to_exitcode(status)
    require(pid == process.pid, "Lost ownership fixture must reap only its own leader")
    with patch("os.killpg") as signaling:
        try:
            child.signal_group(signal.SIGKILL)
        except RuntimeError as error:
            require("ownership" in str(error), "Lost ownership must be explicit")
        else:
            raise AssertionError("Reaped group leader was signaled")
        require(signaling.call_count == 0, "Numeric signals must be suppressed after ownership is lost")

    for name, observation in [("late-observation", True), ("unknown-observation", None)]:
        process = subprocess.Popen(["/bin/sh", "-c", "exit 0"], start_new_session=True)
        child = OwnedChild(process)
        deadline = time.monotonic() + 3
        while not child.exited():
            require(time.monotonic() < deadline, "Owned leader must exit")
            time.sleep(0.01)
        started = time.monotonic()

        def observe(group):
            status = os.waitid(os.P_PID, group, os.WEXITED | os.WNOHANG | os.WNOWAIT)
            require(status is not None and status.si_pid == group, "Leader must stay reserved throughout group observation")
            if observation is None:
                raise RuntimeError("Injected unavailable group inventory")
            return True if time.monotonic() - started < 0.15 else group_has_live_members(group)

        if observation:
            code, _ = child.finish(observe=observe)
            require(code == 0 and time.monotonic() - started >= 0.15, "Delayed liveness must finish only after observation clears")
        else:
            with patch("os.killpg") as signaling:
                try:
                    child.finish(observe=observe)
                except RuntimeError:
                    pass
                else:
                    raise AssertionError("Unknown observation was accepted")
                require(signaling.call_count == 0, "Unknown initial observation must suppress numeric signals")
            require(os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT).si_pid == process.pid,
                    "Uncertain group must retain leader reservation")
            # The control's real observer now proves the group clear. No signal follows this reap.
            require(not group_has_live_members(process.pid), "Unknown-observation control must have no live members")
            _, status = os.waitpid(process.pid, 0)
            process.returncode = os.waitstatus_to_exitcode(status)
    write_json(directory / "controls.json", {"passed": True, "controls": ["exited-leader", "late-descendant", "deadline", "fast-output-budget",
               "sigterm-cleans-nested-group", "spawn-boundary-cancellation", "uncertain-live-child-retains-source", "lost-ownership-signals-suppressed",
               "delayed-observation-reserves-leader", "unknown-observation-signals-suppressed"]})


def settling_controls(output):
    """Darwin signal denial must settle with positive evidence or stay incomplete."""
    require(sys.platform == "darwin", "Darwin settling controls require macOS")
    directory = output / "ownership-settling"
    directory.mkdir()
    records = []
    denied = PermissionError(errno.EPERM, "Injected Darwin signal denial")

    process = subprocess.Popen(["/bin/sh", "-c", "sleep 0.2; exit 7"], start_new_session=True)
    child = OwnedChild(process)
    try:
        with patch("os.killpg", side_effect=denied) as signaling:
            code, residual = child.finish(grace=1)
            require(code == 7 and residual and signaling.call_count == 1,
                    "Initially live denied group must settle without another numeric signal")
    finally:
        if process.returncode is None:
            # The before-fix regression also leaves its reservation intact.
            deadline = time.monotonic() + 2
            while os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT) is None:
                require(time.monotonic() < deadline, "Short-lived control must exit naturally")
                time.sleep(0.01)
            require(not group_has_live_members(process.pid), "Control group must be empty before reaping")
            OwnedChild(process).finish()
    records.append({"control": "delayed-darwin-denial-settles", "passed": True})

    template = fixture(directory / "template", {"case.py": 'import time;time.sleep(0.3)\n'})
    original_finish = OwnedChild.finish
    captured = []

    def short_finish(child, observe=group_has_live_members, grace=5):
        captured.append(child.process)
        return original_finish(child, observe=observe, grace=0.04)

    with patch("os.killpg", side_effect=denied) as signaling:
        timed = execute_case(template, directory / "settled-timeout", [sys.executable, "case.py"], timeout=0.03,
                             baseline=[{"id": "explicit-timeout-control"}])
    require(timed["status"] == "timeout" and timed["cleanupComplete"] is True and not timed["complete"] and
            timed["outcomes"] == {"explicit-timeout-control": "unknown"} and signaling.call_count == 1,
            "Settled timeout cleanup must preserve timeout classification and unknown cells")
    records.append({"control": "settled-timeout-remains-unknown", "passed": True, "evidence": "settled-timeout/execution.json"})

    (template / "case.py").write_text('import time;time.sleep(30)\n')
    try:
        with patch("os.killpg", side_effect=denied) as signaling, patch.object(OwnedChild, "finish", short_finish):
            live = execute_case(template, directory / "live-deadline", [sys.executable, "case.py"], timeout=0.03)
        require(live["cleanupComplete"] is False and live["cleanupErrors"] and signaling.call_count == 1,
                "Denied live group must remain incomplete after its bounded deadline")
        require((directory / "live-deadline/source/case.py").exists(), "Unconfirmed cleanup must retain the source")
        require(group_has_live_members(captured[-1].pid), "Live deadline negative must retain a real live group")
    finally:
        if captured and captured[-1].returncode is None:
            OwnedChild(captured[-1]).finish()
    records.append({"control": "live-deadline-retains-source", "passed": True, "evidence": "live-deadline/execution.json"})

    process = subprocess.Popen(["/bin/sh", "-c", "sleep 0.2; exit 0"], start_new_session=True)
    calls = 0

    def unavailable_after_denial(group):
        nonlocal calls
        calls += 1
        if calls > 1:
            raise RuntimeError("Injected unavailable inventory after signal denial")
        return group_has_live_members(group)

    try:
        with patch("os.killpg", side_effect=denied) as signaling:
            try:
                OwnedChild(process).finish(observe=unavailable_after_denial, grace=1)
            except RuntimeError as error:
                require("unavailable" in str(error), "Unknown observation must be explicit")
            else:
                raise AssertionError("Unavailable observation was accepted after signal denial")
            require(signaling.call_count == 1 and os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT).si_pid == process.pid,
                    "Unknown observation must suppress signals and retain the exited leader")
    finally:
        if process.returncode is None:
            OwnedChild(process).finish()
    records.append({"control": "unknown-after-denial-keeps-reservation", "passed": True})

    process = subprocess.Popen(["/bin/sh", "-c", "sleep 0.2; exit 0"], start_new_session=True)
    original_waitid = os.waitid

    def reap_after_denial(*arguments):
        status = original_waitid(*arguments)
        if status is not None and signaling.call_count:
            require(not group_has_live_members(process.pid), "External reaper control must first prove the group empty")
            pid, code = os.waitpid(process.pid, 0)
            require(pid == process.pid, "Control must reap only its own leader")
            process.returncode = os.waitstatus_to_exitcode(code)
            raise ChildProcessError(errno.ECHILD, "Injected ownership loss after denial")
        return status

    try:
        with patch("os.killpg", side_effect=denied) as signaling, patch("os.waitid", side_effect=reap_after_denial):
            try:
                OwnedChild(process).finish(grace=1)
            except RuntimeError as error:
                require("ownership" in str(error), "Ownership loss after denial must remain explicit")
            else:
                raise AssertionError("Ownership loss after denial was accepted")
            require(signaling.call_count == 1 and process.returncode == 0, "No numeric signal may follow the external reap")
    finally:
        if process.returncode is None:
            OwnedChild(process).finish()
    records.append({"control": "ownership-lost-after-denial", "passed": True})

    for name, platform, failure in [("io-error", "darwin", OSError(errno.EIO, "Injected signal I/O error")),
                                    ("non-darwin-denial", "linux", denied)]:
        process = subprocess.Popen(["/bin/sh", "-c", "exec sleep 30"], start_new_session=True)
        try:
            with patch("os.killpg", side_effect=failure) as signaling, patch.object(execution_runner.sys, "platform", platform):
                try:
                    OwnedChild(process).finish(observe=lambda _group: True, grace=0.04)
                except OSError as error:
                    require(error.errno == failure.errno, "Unrecoverable signal error must be retained")
                else:
                    raise AssertionError("Unrecoverable signal error was accepted")
                require(signaling.call_count == 1, "Unrecoverable signal error must suppress further signals")
        finally:
            if process.returncode is None:
                OwnedChild(process).finish()
        records.append({"control": name, "passed": True})

    # signal_group itself must not accept an empty zombie group on another OS.
    process = subprocess.Popen(["/bin/sh", "-c", "exit 0"], start_new_session=True)
    child = OwnedChild(process)
    deadline = time.monotonic() + 2
    while not child.exited():
        require(time.monotonic() < deadline, "Direct signal control must exit")
        time.sleep(0.01)
    try:
        with patch("os.killpg", side_effect=denied), patch.object(execution_runner.sys, "platform", "linux"):
            try:
                child.signal_group(signal.SIGKILL)
            except PermissionError as error:
                require(error.errno == errno.EPERM, "Non-Darwin direct denial must retain EPERM")
            else:
                raise AssertionError("Non-Darwin direct denial was accepted")
    finally:
        child.finish()
    records.append({"control": "direct-non-darwin-empty-group-denial", "passed": True})
    write_json(directory / "controls.json", {"schemaVersion": 1, "controls": records, "timingUse": "validation only"})
    return records


def execute_controls(output, archguard, installed=None, node="node", wrong_node=None, vitest_node=None):
    output.mkdir(parents=True, exist_ok=False)
    ownership_controls(output)
    if sys.platform == "darwin":
        settling_controls(output)
    command = [node, "--test", "--test-reporter={nodeReporter}", "case.test.mjs"]
    common = 'import {test,describe} from "node:test";import assert from "node:assert/strict";'
    source = "export const lower=(value:number)=>value < 2;\nexport const upper=(value:number)=>value > 3;\n"
    tests = common + 'import {lower,upper} from "./source.ts";describe("boundaries",()=>{test("duplicate",()=>assert.equal(lower(2),false));test("duplicate",()=>assert.equal(upper(3),false));});'
    template = fixture(output / "node-template", {"source.ts": source, "case.test.mjs": tests})
    config = {"sourceRoot": str(template), "archguard": str(archguard), "node": node,
              "subject": {"name": "authored-node-controls", "revision": "authored"}, "command": command,
              "planConfig": {"schemaVersion": 1, "selection": {"include": ["source.ts"]}}}
    matrix = run_matrix(config, output / "node-matrix")
    require(matrix["baselineComplete"], "Native Node baseline must complete")
    require(len(matrix["tests"]) == 2 and matrix["tests"][0]["id"] != matrix["tests"][1]["id"], "Duplicate names need distinct stable IDs")
    require(len(matrix["mutants"]) == 2 and all(mutant["status"] == "killed" for mutant in matrix["mutants"]), "Two independent mutations must be killed")
    kill_sets = [{key for key, cell in mutant["outcomes"].items() if cell == "killed"} for mutant in matrix["mutants"]]
    require(all(len(kills) == 1 for kills in kill_sets) and not kill_sets[0] & kill_sets[1], "Tests must kill different mutants")
    baseline = matrix["completeness"]["baseline"]["tests"]
    records = [{"control": "process-ownership", "passed": True, "evidence": "ownership/controls.json"},
               {"control": "node-dual-kill", "passed": True, "evidence": "node-matrix/matrix.json"}]
    retained = []
    original_case = execution_runner.execute_case
    original_finish = OwnedChild.finish
    calls = []

    def uncertain_column(template, evidence, arguments, **options):
        calls.append(options.get("mutation"))
        if not options.get("mutation"):
            return original_case(template, evidence, arguments, **options)

        def unavailable_finish(child, observe=group_has_live_members, grace=5):
            retained.append(child.process)

            def unavailable(_group):
                raise RuntimeError("Injected unavailable mutant group inventory")

            return original_finish(child, observe=unavailable, grace=grace)

        options["timeout"] = 0.15
        with patch.object(OwnedChild, "finish", unavailable_finish):
            return original_case(template, evidence, [sys.executable, "-c", "import time;time.sleep(30)"], **options)

    with patch("runner.execute_case", uncertain_column):
        stopped = run_matrix(config, output / "scheduler-uncertain")
    try:
        require(len(calls) == 2 and stopped["mutants"][0]["status"] == "error" and stopped["mutants"][1]["status"] == "notRun",
                "Unconfirmed mutant cleanup must stop the next execution")
        require(stopped["completeness"]["cleanupConfirmed"] is False and all(cell == "unknown" for cell in stopped["mutants"][1]["outcomes"].values()),
                "Remaining stopped columns must retain unknown cells and cleanup evidence")
        require((output / "scheduler-uncertain/mutant-00000/source/case.test.mjs").is_file() and not (output / "scheduler-uncertain/mutant-00001").exists(),
                "Uncertain source must remain and no subsequent source execution may start")
        require(len(retained) == 1 and group_has_live_members(retained[0].pid), "Scheduler control must retain a real live child")
    finally:
        for process in retained:
            require(OwnedChild(process).finish()[0] < 0, "Scheduler control restores real observation before cleanup")
    records.append({"control": "uncertain-cleanup-stops-scheduler", "passed": True, "evidence": "scheduler-uncertain/matrix.json"})
    cases = {
        "missing": common + 'test("duplicate",()=>assert.equal(2,2));',
        "skipped": tests.replace('test("duplicate"', 'test.skip("duplicate"', 1),
        "extra": tests + 'test("extra",()=>assert.equal(1,1));',
        "assertion-runtime": tests.replace('assert.equal(lower(2),false)', 'assert.equal(lower(2),true)') + 'test("runtime",()=>{throw new Error("runtime control");});',
        "runtime-looks-like-assertion": common + 'test("duplicate",()=>{throw new Error("ERR_ASSERTION AssertionError");});',
        "import": 'import "./missing-import.mjs";\n' + tests,
    }
    for name, body in cases.items():
        case = fixture(output / f"node-{name}-template", {"source.ts": source, "case.test.mjs": body})
        result = execute_case(case, output / f"node-{name}", command, baseline=baseline, node=node)
        require(not result["complete"] and result["status"] == "error", f"{name} must be incomplete")
        require(all(cell == "unknown" for cell in result["outcomes"].values()), f"{name} must retain unknown cells")
        records.append({"control": f"node-{name}", "passed": True, "evidence": f"node-{name}/execution.json"})
    assertion_only = fixture(output / "node-assertion-template", {"source.ts": source, "case.test.mjs": tests.replace('assert.equal(lower(2),false)', 'assert.equal(lower(2),true)')})
    result = execute_case(assertion_only, output / "node-assertion", command, baseline=baseline, node=node)
    require(result["complete"] and result["status"] == "killed", "Correct inventory with assertion only must be accepted")
    correction = execute_case(template, output / "node-correction", command, baseline=baseline, node=node)
    require(correction["complete"] and correction["status"] == "survived", "Correction must pass frozen inventory")
    records.append({"control": "node-assertion-correction", "passed": True, "evidence": "node-correction/execution.json"})

    writer = tests + '\nimport fs from "node:fs";import path from "node:path";test("artifact isolation",()=>{for(const directory of [process.cwd(),process.env.HOME,process.env.TMPDIR,process.env.XDG_CACHE_HOME]){const file=path.join(directory,"control-artifact");assert.equal(fs.existsSync(file),false);fs.writeFileSync(file,"owned");}});'
    writer_template = fixture(output / "writer-template", {"source.ts": source, "case.test.mjs": writer})
    writer_baseline = execute_case(writer_template, output / "writer-baseline", command, node=node, captured_artifacts=["control-artifact"])
    require(writer_baseline["complete"], "Artifact writer baseline must pass")
    plan = json.loads((output / "node-matrix/plan.json").read_text())
    for order in [[0, 1], [1, 0]]:
        for index in order:
            result = execute_case(writer_template, output / f"writer-order-{order[0]}-mutant-{index}", command,
                                  baseline=writer_baseline["tests"], mutation=plan["sites"][index], node=node)
            require(result["complete"] and result["status"] == "killed", "Artifact writes must not contaminate either order")
    require(not (writer_template / "control-artifact").exists(), "Template must remain unchanged")
    records.append({"control": "artifact-writes-both-orders", "passed": True, "evidence": "writer-baseline/execution.json"})

    installed_fixture = fixture(output / "workspace-installed", {
        "packages/ws/package.json": '{"name":"@control/ws","type":"module","exports":"./source.ts"}',
        "packages/ws/source.ts": "export const control = true;\n",
        "node_modules/external/package.json": '{"name":"external","main":"index.js"}',
        "node_modules/external/index.js": "module.exports=7;\n",
    })
    (installed_fixture / "node_modules/@control").mkdir()
    (installed_fixture / "node_modules/@control/ws").symlink_to(installed_fixture / "packages/ws")
    dependencies = DependencyStore(installed_fixture, output / "workspace-store").copy()
    ws_template = fixture(output / "workspace-template", {
        "packages/ws/package.json": (installed_fixture / "packages/ws/package.json").read_text(),
        "packages/ws/source.ts": "export const control = true;\n",
        "case.test.mjs": common + 'import {control} from "@control/ws";test("workspace",()=>assert.equal(control,true));',
    })
    imported = [{"specifier": "@control/ws", "expectedWorkspacePath": "packages/ws/source.ts"}]
    ws_baseline = execute_case(ws_template, output / "workspace-baseline", command, dependencies=dependencies, node=node, import_controls=imported)
    require(ws_baseline["complete"], "Workspace baseline must pass")
    raw = (ws_template / "packages/ws/source.ts").read_bytes()
    start = raw.index(b"true")
    mutation = {"id": "b" * 64, "sourceSha256": hashlib.sha256(raw).hexdigest(),
                "location": {"file": "packages/ws/source.ts", "start": start, "end": start + 4}, "expected": "true", "replacement": "false"}
    mutated = execute_case(ws_template, output / "workspace-mutated", command, baseline=ws_baseline["tests"], dependencies=dependencies,
                           mutation=mutation, node=node, import_controls=imported)
    require(mutated["complete"] and mutated["status"] == "killed", "Copied workspace import must observe owned mutation")
    require((installed_fixture / "packages/ws/source.ts").read_bytes() == raw, "Installed workspace source must remain unchanged")
    require(dependencies.current_digest() == dependencies.sha256, "External store must remain unchanged")
    invalid = dict(mutation, sourceSha256="c" * 64)
    rejected = execute_case(ws_template, output / "workspace-hash-mismatch", command, baseline=ws_baseline["tests"],
                            dependencies=dependencies, mutation=invalid, node=node)
    require(rejected["status"] == "error" and all(cell == "unknown" for cell in rejected["outcomes"].values()),
            "Wrong source hash must remain an execution error with unknown cells")
    require(not (output / "workspace-hash-mismatch/stdout.txt").exists(), "Wrong source hash must reject the edit before test execution")
    records.append({"control": "owned-workspace-import-and-hash", "passed": True, "evidence": "workspace-mutated/execution.json"})

    if installed:
        node = vitest_node or node
        dependencies = DependencyStore(installed, output / "vitest-store").copy()
        base_files = {"package.json": '{"type":"module"}', "source.ts": source}
        vt_tests = 'import {test,describe,expect} from "vite-plus/test";import {lower,upper} from "./source.ts";describe("boundaries",()=>{test("duplicate",()=>expect(lower(2)).toBe(false));test("duplicate",()=>expect(upper(3)).toBe(false));});'
        pin_check = ('import assert from "node:assert/strict";import fs from "node:fs";import {createHash} from "node:crypto";'
                     'import {execFileSync} from "node:child_process";const expected=JSON.parse(process.env.ARCHGUARD_RESEARCH_NODE);'
                     'const executable=fs.realpathSync(process.execPath);'
                     'assert.deepEqual({path:executable,version:process.version,sha256:createHash("sha256").update(fs.readFileSync(executable)).digest("hex")},expected);'
                     'assert.equal(process.env.PATH.split(":")[0],executable.slice(0,executable.lastIndexOf("/")));'
                     'assert.equal(fs.realpathSync(execFileSync("node",["-p","process.execPath"],{encoding:"utf8"}).trim()),expected.path);')
        vt_command = direct_vitest_command()
        vt_config = 'import {defineConfig} from "vite-plus/test/config";export default defineConfig({test:{include:["case.test.ts"],pool:"forks",maxWorkers:1,reporters:[' + json.dumps(str(Path(__file__).resolve().parent / "vitest-reporter.ts")) + '],EXTRA}});'

        def vt_fixture(name, tests_body=vt_tests, extra="", setup=None, pinned=True):
            files = dict(base_files, **{"case.test.ts": (pin_check if pinned else "") + tests_body, "vite.config.ts": vt_config.replace("EXTRA", extra)})
            if setup:
                files["setup.ts"] = setup
            template = fixture(output / f"vitest-{name}-template", files)
            for target in dependencies.workspace_links.values():
                workspace = template / target
                workspace.mkdir(parents=True, exist_ok=True)
                (workspace / "control.txt").write_text("Owned workspace directory control.\n")
            return template

        vt_template = vt_fixture("passing")
        vt_baseline = execute_case(vt_template, output / "vitest-passing", vt_command, dependencies=dependencies, node=node, timeout=40)
        require(vt_baseline["complete"], f"Vitest baseline must complete: {vt_baseline['infrastructureErrors']}")
        inventory = json.loads((output / "vitest-passing/inventory.json").read_text())
        require(inventory["runtime"] == vt_baseline["node"], "Reporter must agree with the pinned runtime path, version and hash")
        require(vt_baseline["command"][:2] == [vt_baseline["node"]["path"], vt_baseline["vitest"]["path"]], "Vitest must launch directly using the absolute pinned Node")
        require(not list((output / "vitest-passing/private").rglob("js_runtime")), "Direct Vitest must not install another runtime in the private directories")
        records.append({"control": "direct-node-vitest-reporter-and-fork-pin", "passed": True, "evidence": "vitest-passing/execution.json"})
        require(len(vt_baseline["tests"]) == 2 and vt_baseline["tests"][0]["id"] != vt_baseline["tests"][1]["id"], "Vitest duplicate names need distinct IDs")
        for index, mutation in enumerate(plan["sites"]):
            result = execute_case(vt_template, output / f"vitest-mutant-{index}", vt_command,
                                  dependencies=dependencies, baseline=vt_baseline["tests"], mutation=mutation, node=node, timeout=40)
            require(result["complete"] and sum(cell == "killed" for cell in result["outcomes"].values()) == 1,
                    f"Vitest must report exact independent kill set: {result['infrastructureErrors']}")
        records.append({"control": "vitest-dual-kill-duplicate-names", "passed": True, "evidence": "vitest-passing/execution.json"})
        if wrong_node:
            wrong_template = vt_fixture("wrong-runtime", pinned=False)
            wrong_command = [str(Path(wrong_node).resolve()), *vt_command[1:]]
            wrong = execute_case(wrong_template, output / "vitest-wrong-runtime", wrong_command, dependencies=dependencies,
                                 baseline=vt_baseline["tests"], mutation=plan["sites"][0], node=node, timeout=40)
            require(wrong["protocol"] and wrong["protocol"]["complete"] and wrong["protocol"]["tests"]["failed"] == 1,
                    "Wrong runtime control must retain a complete real SDK assertion result")
            require(not wrong["complete"] and all(cell == "unknown" for cell in wrong["outcomes"].values()) and
                    any("Reporter runtime disagrees" in error for error in wrong["infrastructureErrors"]),
                    "A real reporter on another runtime must invalidate every column cell")
            records.append({"control": "vitest-wrong-runtime-real-reporter", "passed": True, "evidence": "vitest-wrong-runtime/execution.json"})
            pin = output / "owned-node-pin"
            pin.symlink_to(Path(node).resolve())
            switched_template = vt_fixture("runtime-switch")
            switched_config = {"sourceRoot": str(switched_template), "dependencyRoot": str(installed), "archguard": str(archguard),
                "node": str(pin), "subject": {"name": "authored-runtime-switch", "revision": "authored"}, "command": vt_command,
                "planConfig": {"schemaVersion": 1, "selection": {"include": ["source.ts"]}}}
            original_execute = execution_runner.execute_case

            def switch_after_baseline(*arguments, **options):
                execution = original_execute(*arguments, **options)
                if options.get("baseline") is None:
                    pin.unlink()
                    pin.symlink_to(Path(wrong_node).resolve())
                return execution

            try:
                with patch.object(DependencyStore, "copy", return_value=dependencies), patch.object(execution_runner, "execute_case", side_effect=switch_after_baseline):
                    switched = run_matrix(switched_config, output / "vitest-runtime-switch")
                require(switched["baselineComplete"], "Runtime switch baseline must pass on the original frozen pin")
                for column in switched["mutants"]:
                    detail = json.loads((output / "vitest-runtime-switch" / column["evidence"]).read_text())
                    require(detail["protocol"]["complete"] and detail["protocol"]["tests"]["failed"] == 1 and detail["node"] != detail["expectedNode"],
                            "Changed runtime must retain actual assertion evidence and the original frozen identity")
                    require(column["status"] == "error" and all(cell == "unknown" for cell in column["outcomes"].values()) and
                            any("frozen execution identity" in error for error in column["infrastructureErrors"]),
                            "Changing the selected runtime after baseline must invalidate every cell")
            finally:
                pin.unlink()
                pin.symlink_to(Path(node).resolve())
            restored = execute_case(switched_template, output / "vitest-runtime-restored", vt_command, dependencies=dependencies,
                                    baseline=vt_baseline["tests"], node=str(pin), expected_node=vt_baseline["node"], timeout=40)
            require(restored["complete"] and restored["status"] == "survived", "Restoring the same frozen runtime must accept a passing execution")
            records.append({"control": "vitest-runtime-switch-between-columns-and-restoration", "passed": True,
                            "evidence": "vitest-runtime-switch/matrix.json"})
        for name, body, extra, setup in [
            ("skipped", vt_tests.replace('test("duplicate"', 'test.skip("duplicate"', 1), "", None),
            ("missing", vt_tests.replace('test("duplicate",()=>expect(upper(3)).toBe(false));', ""), "", None),
            ("assertion-runtime", vt_tests.replace("expect(lower(2)).toBe(false)", "expect(lower(2)).toBe(true)") + 'test("runtime",()=>{throw new Error("runtime control");});', "", None),
            ("assertion-teardown", vt_tests.replace("expect(lower(2)).toBe(false)", "expect(lower(2)).toBe(true)"), 'globalSetup:["./setup.ts"],', 'export default function(){return ()=>{throw new Error("late teardown");};}'),
            ("late-exit", vt_tests.replace("expect(lower(2)).toBe(false)", "expect(lower(2)).toBe(true)"), 'globalSetup:["./setup.ts"],', 'export default function(){process.on("exit",()=>{throw new Error("late exit");});}'),
            ("retry", vt_tests, "retry:1,", None),
        ]:
            case = vt_fixture(name, body, extra, setup)
            result = execute_case(case, output / f"vitest-{name}", vt_command, dependencies=dependencies,
                                  baseline=vt_baseline["tests"], node=node, timeout=40)
            require(not result["complete"] and all(cell == "unknown" for cell in result["outcomes"].values()), f"Vitest {name} must retain unknown cells")
            records.append({"control": f"vitest-{name}", "passed": True, "evidence": f"vitest-{name}/execution.json"})
        metadata = vt_fixture("late-metadata", vt_tests.replace("expect(lower(2)).toBe(false)", "expect(lower(2)).toBe(true)"))
        (metadata / "blocked").write_text("not a directory")
        (metadata / "metadata-reporter.ts").write_text('export default class MetadataControl{onInit(context){this.context=context;}onTestRunEnd(){this.context.state.metadata.control={dumpDir:"./blocked"};}}')
        configuration = (metadata / "vite.config.ts").read_text().replace('],}}', ',"./metadata-reporter.ts"],}}')
        (metadata / "vite.config.ts").write_text(configuration)
        result = execute_case(metadata, output / "vitest-late-metadata", vt_command, dependencies=dependencies,
                              baseline=vt_baseline["tests"], node=node, timeout=40)
        require(not result["complete"] and all(cell == "unknown" for cell in result["outcomes"].values()), "Late metadata failure must reject assertion evidence")
        records.append({"control": "vitest-late-metadata", "passed": True, "evidence": "vitest-late-metadata/execution.json"})
        require(dependencies.current_digest() == dependencies.sha256, "Vitest copied external store must remain unchanged")
    write_json(output / "controls.json", {"schemaVersion": 1, "controls": records, "timingUse": "validation only"})
    return records


def backend_controls(output, installed, bundled_installed, node, copied_stores=None):
    output.mkdir(parents=True, exist_ok=False)
    records = []
    reporter = Path(__file__).resolve().parent / "vitest-reporter.ts"
    pin = ('const expected=JSON.parse(process.env.ARCHGUARD_RESEARCH_NODE);'
           'expect({path:fs.realpathSync(process.execPath),version:process.version,'
           'sha256:createHash("sha256").update(fs.readFileSync(process.execPath)).digest("hex")}).toEqual(expected);')
    tests = ('import {test,expect,describe} from "vite-plus/test";import {lower,upper} from "./source.ts";'
             'import fs from "node:fs";import {createHash} from "node:crypto";'
             'describe("bounds",()=>{test("duplicate",()=>{' + pin + 'expect(lower(2)).toBe(false)});'
             'test("duplicate",()=>expect(upper(3)).toBe(false));});')
    config = ('import {defineConfig} from "vite-plus/test/config";export default defineConfig({test:{'
              'include:["case.test.ts"],pool:"forks",maxWorkers:1,retry:0,bail:0,reporters:[' + json.dumps(str(reporter)) + ']}});')
    for label, donor, expected_package in [("current", installed, "vitest"), ("bundled", bundled_installed, "@voidzero-dev/vite-plus-test")]:
        root = output / label
        if copied_stores:
            metadata = execution_runner.read_json(copied_stores)[label]
            require(Path(metadata["installedRoot"]).resolve() == Path(donor).resolve(), "Reused control store must have the same explicit donor")
            dependencies = DependencyStore(donor, metadata["root"])
            execution_runner.inside(dependencies.root, Path(copied_stores).resolve().parent)
            dependencies.modules = metadata["modules"]
            dependencies.workspace_links = metadata["workspaceLinks"]
            dependencies.owned_directories = set(metadata["ownedDirectories"])
            dependencies.sha256 = metadata["sha256"]
            require(dependencies.current_digest() == dependencies.sha256, "Reused owned control store must retain its exact recorded digest")
        else:
            dependencies = DependencyStore(donor, root / "dependencies").copy()
        files = {"package.json": '{"type":"module"}', "source.ts": "export const lower=(n:number)=>n<2;export const upper=(n:number)=>n>3;",
                 "case.test.ts": tests, "vite.config.ts": config}

        def template(name, changed=False):
            entries = dict(files)
            if changed:
                entries["source.ts"] = entries["source.ts"].replace("n<2", "n<=2")
            case = fixture(root / name, entries)
            for relative in dependencies.workspace_links.values():
                (case / relative).mkdir(parents=True, exist_ok=True)
            return case

        case = template("template")
        baseline = execute_case(case, root / "baseline", direct_vitest_command(), dependencies=dependencies, node=node, keep_source=True)
        require(baseline["complete"] and baseline["status"] == "survived", f"Actual {label} backend must pass: {baseline['infrastructureErrors']}")
        require(baseline["vitest"]["runnerPackage"] == expected_package, "Runner must use its explicit declared package")
        require(len(baseline["tests"]) == 2 and len({test["id"] for test in baseline["tests"]}) == 2, "Actual duplicate tests need two stable IDs")
        require(json.loads((root / "baseline/inventory.json").read_text())["runtime"] == baseline["node"], "Actual reporter and fork must use the pinned runtime")
        require(baseline["command"][:2] == [baseline["node"]["path"], baseline["vitest"]["path"]], "Actual historical entry must launch directly with pinned Node")
        version_process = run_command([baseline["node"]["path"], baseline["vitest"]["path"], "--version"], root / "baseline/source",
                                      private_environment(root / "version-private"), root / "actual-cli-version")
        version_stdout = (root / "actual-cli-version/stdout.txt").read_text().strip()
        require(version_process["status"] == "finished" and version_process["exitCode"] == 0 and version_process["cleanupComplete"] and
                not version_process["cleanupErrors"] and not version_process["residualProcessGroup"], "Real direct CLI version command must finish normally")
        prefix = "vp test" if label == "bundled" else "vitest"
        version_match = re.match(re.escape(prefix) + r"/([^\s]+)", version_stdout)
        require(version_match is not None and version_match.group(1) == baseline["vitest"]["version"], "Actual CLI Vitest version must agree with package metadata")
        require("node-" + baseline["node"]["version"] in version_stdout, "Actual CLI version command must identify the pinned Node version")
        faulty = execute_case(template("faulty-template", True), root / "faulty", direct_vitest_command(), dependencies=dependencies,
                              baseline=baseline["tests"], node=node)
        require(faulty["complete"] and list(faulty["outcomes"].values()).count("killed") == 1 and
                list(faulty["outcomes"].values()).count("notKilled") == 1, "Actual backend must distinguish assertion failure and unaffected test")
        restored = execute_case(case, root / "restored", direct_vitest_command(), dependencies=dependencies, baseline=baseline["tests"], node=node)
        require(restored["complete"] and restored["status"] == "survived", "Correction must restore actual passing inventory")
        route_source = root / "baseline/source"
        environment = private_environment(root / "probe-private")
        if label == "bundled":
            before_script = ('import {createRequire} from "node:module";import path from "node:path";'
                             'const context=createRequire(path.join(process.cwd(),"package.json")).resolve("vite-plus/package.json");'
                             'createRequire(context).resolve("vitest/package.json");')
            before = run_command([baseline["node"]["path"], "--input-type=module", "-e", before_script], route_source,
                                 environment, root / "before-old-assumption")
            require(before["exitCode"] != 0 and before["cleanupComplete"] and "Cannot find module" in
                    (root / "before-old-assumption/stderr.txt").read_text(), "Previous separate Vitest assumption must fail on the real historical package")
        entry = Path(baseline["vitest"]["path"])
        renamed = entry.with_name(entry.name + ".control-retained")
        entry.rename(renamed)
        try:
            try:
                execution_runner.vitest_entrypoint(route_source, dependencies, baseline["node"]["path"], environment)
            except ValueError:
                pass
            else:
                raise AssertionError("Missing declared entrypoint was accepted")
        finally:
            renamed.rename(entry)
        context = Path(baseline["vitest"]["context"])
        original = context.read_bytes()
        metadata = json.loads(original)
        metadata["dependencies"].pop(expected_package)
        context.write_text(json.dumps(metadata))
        try:
            try:
                execution_runner.vitest_entrypoint(route_source, dependencies, baseline["node"]["path"], environment)
            except ValueError as error:
                require("must declare exactly one" in str(error), "Undeclared package must reject before resolving an available transitive entry")
            else:
                raise AssertionError("Undeclared available test package was accepted")
        finally:
            context.write_bytes(original)
        require(dependencies.current_digest() == dependencies.sha256, "Restored copied package bytes must match their frozen digest")
        records.append({"backend": label, "passed": True, "runner": baseline["vitest"], "node": baseline["node"],
                        "actualCliVersion": version_stdout, "actualCliVersionProcess": version_process,
                        "baseline": label + "/baseline/execution.json", "faulty": label + "/faulty/execution.json",
                        "restored": label + "/restored/execution.json", "negativeControls": ["missing-entry", "undeclared-package"]})
    write_json(output / "controls.json", {"schemaVersion": 1, "validationOnly": True, "controls": records})
    return records


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True)
    parser.add_argument("--archguard", required=True)
    parser.add_argument("--installed", help="Explicit installed Vitest repository, optional")
    parser.add_argument("--node", default="node")
    parser.add_argument("--vitest-node", help="Explicit Vitest runtime when the native Node event adapter requires another version")
    parser.add_argument("--wrong-node", help="Different actual Node executable for the real reporter runtime rejection control")
    parser.add_argument("--ownership-only", action="store_true", help="Run only subprocess ownership and Darwin cleanup controls")
    parser.add_argument("--backend-only", action="store_true", help="Run the declared current and bundled Vitest entry controls")
    parser.add_argument("--bundled-installed", help="Explicit installed historical vite-plus test package for backend controls")
    parser.add_argument("--backend-copied-stores", help="Explicit unchanged owned store metadata for repeating backend controls")
    arguments = parser.parse_args()
    if arguments.backend_only:
        if not arguments.installed or not arguments.bundled_installed:
            parser.error("Backend controls require both --installed and --bundled-installed")
        records = backend_controls(Path(arguments.output).resolve(), arguments.installed, arguments.bundled_installed, arguments.node, arguments.backend_copied_stores)
        print(f"Passed {len(records)} backend controls")
        return
    if arguments.ownership_only:
        output = Path(arguments.output).resolve()
        output.mkdir(parents=True, exist_ok=False)
        ownership_controls(output)
        if sys.platform == "darwin":
            settling_controls(output)
        print("Passed subprocess ownership controls")
        return
    records = execute_controls(Path(arguments.output).resolve(), Path(arguments.archguard).resolve(), arguments.installed, arguments.node,
                               arguments.wrong_node, arguments.vitest_node)
    print(f"Passed {len(records)} controls")


if __name__ == "__main__":
    install_signal_handlers()
    main()
