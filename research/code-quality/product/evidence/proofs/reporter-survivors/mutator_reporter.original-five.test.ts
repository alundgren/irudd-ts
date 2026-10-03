import assert from "node:assert/strict";
import { test } from "node:test";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import type { TestExecutionResult } from "../sdk/mutator.ts";

const reporter = pathToFileURL(path.resolve("sdk/mutator-vitest-reporter.ts")).href;

function run(scenario: string): TestExecutionResult {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "archguard-reporter-"));
  const resultPath = path.join(directory, "result.json");
  const requestPath = path.join(directory, "request.json");
  fs.writeFileSync(requestPath, JSON.stringify({
    schemaVersion: 1, requestId: scenario, runId: "reporter-tests", inputDigest: "a".repeat(64),
    phase: "baseline", mutationId: null, resultPath, maxResultBytes: 1_048_576,
  }));
  const source = `
    import Reporter from ${JSON.stringify(reporter)};
    import fs from "node:fs";
    import {syncBuiltinESMExports} from "node:module";
    const scenario = ${JSON.stringify(scenario)};
    const reporter = new Reporter();
    if(scenario === "lateExit") process.on("exit",()=>{throw new Error("late exit control");});
    if(scenario === "failedPublication" || scenario === "failedPublicationTwo") process.on("exit",()=>{
      fs.openSync=()=>{throw new Error("ENOSPC control");};syncBuiltinESMExports();throw new Error("late exit control");
    });
    let resolveRun, rejectRun;
    const pending = new Promise((resolve,reject)=>{resolveRun=resolve;rejectRun=reject;});
    const context = {
      config:{watch:scenario === "watch",mergeReports:scenario === "merge" ? "old-results" : undefined},
      close: async () => {if(scenario === "cleanup") context.logger.error("error during close");},
      waitForTestRunEnd: () => pending,
      state: {blobs:scenario === "replay" ? {} : undefined,getUnhandledErrors:()=>scenario === "unhandled" ? [{message:"unhandled control"}] : []},
      logger: {error:()=>{}},
    };
    if (scenario === "missingApi") delete context.waitForTestRunEnd;
    reporter.onInit(context);
    const failed = !["pass", "pending", "missingApi", "replacedApi", "retry", "overflow"].includes(scenario);
    const test = {id:"case-1",type:"test",module:{moduleId:"case.test.ts"},result:()=>({
      state:failed ? "failed" : "passed",
      errors:failed ? [{name:scenario === "runtime" ? "TypeError" : "AssertionError",message:scenario === "unicode" ? "x".repeat(3_999)+"🚀" : "boundary control"}] : scenario === "retry" ? [{name:"AssertionError",message:"retry control"}] : [],
    })};
    const module = {
      type:"module",moduleId:"case.test.ts",errors:()=>scenario === "import" ? [{message:"import control"}] : [],
      children:{allTests:()=>[test],allSuites:()=>scenario === "suite" ? [{type:"suite",errors:()=>[{message:"suite control"}],module:{moduleId:"case.test.ts"}}] : []},
    };
    if (scenario === "hook" || scenario === "goodHook") reporter.onHookEnd({
      name:"afterAll",entity:{task:{result:{hooks:{afterAll:scenario === "hook" ? "fail" : "pass"}}},moduleId:"case.test.ts"},
    });
    if (scenario === "unknownHook") reporter.onHookEnd({name:"afterAll",entity:{moduleId:"case.test.ts"}});
    if (scenario === "overflow") module.errors=()=>Array(10_001).fill({message:"too many"});
    reporter.onTestRunEnd([module],[],scenario === "interrupted" ? "interrupted" : failed ? "failed" : "passed");
    if(scenario === "replacedApi") context.close=async()=>{};
    if(scenario === "pending") process.exit(1);
    if(scenario === "lateRun") rejectRun(new Error("late coverage control")); else resolveRun();
    await Promise.resolve();
    await context.close();
    if(scenario === "postRenamePublication") {
      let opens=0;const originalOpen=fs.openSync;
      fs.openSync=(...args)=>{if(++opens>1)throw new Error("ENOSPC control");return originalOpen(...args);};
      fs.rmSync=()=>{throw new Error("cleanup control after rename");};syncBuiltinESMExports();
    }
    process.exitCode = scenario === "failedPublicationTwo" || scenario === "postRenamePublication" ? 2 : failed || scenario === "retry" || scenario === "overflow" ? 1 : 0;
  `;
  try {
    const child = spawnSync(process.execPath, ["--input-type=module", "--eval", source], {
      env: { ...process.env, ARCHGUARD_MUTATION_REQUEST: requestPath, ARCHGUARD_MUTATION_RESULT: resultPath },
      timeout: 10_000, maxBuffer: 65_536,
    });
    assert.equal(child.error, undefined, child.error?.message);
    assert.ok(fs.existsSync(resultPath), child.stderr.toString());
    const result = JSON.parse(fs.readFileSync(resultPath, "utf8")) as TestExecutionResult;
    if (scenario === "failedPublication" || scenario === "failedPublicationTwo" || scenario === "postRenamePublication") assert.notEqual(result.exitCode, child.status, "Stale complete evidence must disagree with raw exit");
    else assert.equal(result.exitCode, child.status);
    return result;
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
}

test("assertion failures retain IDs and passing controls finish cleanly", () => {
  const failed = run("assertion");
  assert.equal(failed.complete, true);
  assert.deepEqual(failed.tests, { passed: 0, failed: 1, skipped: 0 });
  assert.equal(failed.failures[0]?.kind, "assertion");
  assert.equal(failed.failures[0]?.testId, "case-1");
  const corrected = run("pass");
  assert.equal(corrected.complete, true);
  assert.deepEqual(corrected.failures, []);
  assert.deepEqual(corrected.tests, { passed: 1, failed: 0, skipped: 0 });
});

test("assertion plus late run or cleanup failure cannot be assertion-only", () => {
  const late = run("lateRun");
  assert.equal(late.complete, false);
  assert.ok(late.failures.some(error => error.kind === "runtime"));
  const cleanup = run("cleanup");
  assert.ok(cleanup.failures.some(error => error.kind === "runtime"));
  assert.ok(cleanup.failures.some(error => error.kind === "assertion"));
  const lateExit = run("lateExit");
  assert.equal(lateExit.complete, false);
  assert.ok(lateExit.failures.some(error => error.kind === "unhandled"));
  const stale = run("failedPublication");
  assert.equal(stale.complete, true);
  assert.equal(stale.exitCode, 1);
  assert.equal(run("failedPublicationTwo").exitCode, 2);
  const postRename = run("postRenamePublication");
  assert.equal(postRename.complete, true);
  assert.equal(postRename.exitCode, 2);
  assert.equal(run("assertion").failures.every(error => error.kind === "assertion"), true);
});

test("runtime, import, suite, hook and unhandled failures remain distinct", () => {
  for (const kind of ["runtime", "import", "suite", "hook", "unhandled"] as const) {
    const result = run(kind);
    assert.equal(result.complete, true, kind);
    assert.ok(result.failures.some(error => error.kind === kind), kind);
  }
  assert.equal(run("goodHook").failures.every(error => error.kind === "assertion"), true);
  assert.equal(run("unknownHook").complete, false);
});

test("missing or replaced lifecycle APIs, interruption and early exit stay incomplete", () => {
  for (const scenario of ["missingApi", "replacedApi", "interrupted", "pending", "watch", "merge", "replay"]) {
    assert.equal(run(scenario).complete, false, scenario);
  }
  assert.equal(run("pass").complete, true);
});

test("retained retry errors and oversized runner metadata fail conservatively", () => {
  assert.equal(run("retry").complete, false);
  assert.equal(run("overflow").complete, false);
  assert.equal(run("assertion").complete, true);
  const unicode = run("unicode");
  assert.equal(unicode.complete, true);
  assert.ok(unicode.failures[0]?.message.endsWith("[truncated]"));
  assert.equal(Buffer.from(unicode.failures[0]!.message).toString("utf8"), unicode.failures[0]?.message);
});
