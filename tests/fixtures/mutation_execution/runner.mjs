import fs from "node:fs";
import assert from "node:assert/strict";
import { enabled } from "./subject.ts";

const request = JSON.parse(fs.readFileSync(process.env.ARCHGUARD_MUTATION_REQUEST, "utf8"));
const control = JSON.parse(fs.readFileSync("control.json", "utf8"));
const marker = process.argv[2];
if (marker) fs.appendFileSync(marker, `${request.phase}\n`);
const result = {
  schemaVersion: 1, requestId: request.requestId, runId: request.runId,
  inputDigest: request.inputDigest, complete: true, exitCode: 0,
  reason: "finished", tests: { passed: 1, failed: 0, skipped: 0 }, failures: [],
};
try {
  assert.equal(enabled(), true);
} catch (error) {
  result.exitCode = 1;
  result.tests = { passed: 0, failed: 1, skipped: 0 };
  result.failures = [{ kind: "assertion", testId: "enabled", file: "subject.ts", message: error.message }];
}
if (control.mode === "baselineFailure" && request.phase === "baseline") {
  result.exitCode = 1; result.tests = { passed: 0, failed: 1, skipped: 0 };
  result.failures = [{ kind: "assertion", testId: "baseline", file: "subject.ts", message: "baseline control" }];
}
if (request.phase === "mutation") {
  if (control.mode === "errorEnabled" && enabled() === false) {
    try { await import("./missing-control.ts"); }
    catch (error) { result.failures.push({ kind: "import", testId: null, file: "missing-control.ts", message: error.message }); }
  }
  if (control.mode === "mixed") result.failures.push({ kind: "hook", testId: null, file: null, message: "teardown control" });
  if (control.mode === "stale") result.requestId = "wrong-request";
  if (control.mode === "missing") process.exit(1);
  if (control.mode === "arbitrary") process.exit(7);
  if (control.mode === "zero") { result.tests = { passed: 0, failed: 0, skipped: 1 }; result.failures = []; result.exitCode = 0; }
  if (control.mode === "provisional") { result.complete = false; result.reason = "interrupted"; }
  if (control.mode === "rewrite") {
    fs.writeFileSync("subject.ts", "export function enabled() { return false; }\n");
    fs.writeFileSync("control.json", '{"mode":"baselineFailure"}');
  }
  if (control.mode === "wait") await new Promise(resolve => setTimeout(resolve, 30_000));
  if (control.mode === "waitUnusedOnce" && enabled() && !fs.existsSync(`${marker}.released`)) {
    fs.writeFileSync(`${marker}.waiting`, "waiting");
    await new Promise(resolve => setTimeout(resolve, 300_000));
  }
  if (control.mode === "disk") fs.writeFileSync("generated.bin", Buffer.alloc(2_000_000));
  if (control.mode === "stdout") process.stdout.write("x".repeat(2_000_000));
}
const temporary = `${request.resultPath}.tmp`;
fs.writeFileSync(temporary, JSON.stringify(result), { mode: 0o600, flag: "wx" });
fs.renameSync(temporary, request.resultPath);
process.exitCode = result.exitCode;
