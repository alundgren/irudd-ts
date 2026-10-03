import assert from "node:assert/strict";
import { test } from "node:test";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import {
  readMutationRequest, validateMutationRequest, validateMutationResult, writeMutationResult,
  mutationProtocolLimits,
} from "../sdk/mutator.ts";
import type { TestExecutionRequest, TestExecutionResult } from "../sdk/mutator.ts";

function fixture(directory: string): TestExecutionRequest {
  return {
    schemaVersion: 1, requestId: "run-1-task-1", runId: "run-1", inputDigest: "a".repeat(64),
    phase: "mutation", mutationId: "b".repeat(64), resultPath: path.join(directory, "result.json"),
    maxResultBytes: 1_048_576,
  };
}
function passing(request: TestExecutionRequest): TestExecutionResult {
  return {
    schemaVersion: 1, requestId: request.requestId, runId: request.runId, inputDigest: request.inputDigest,
    complete: true, exitCode: 0, reason: "finished", tests: { passed: 1, failed: 0, skipped: 0 }, failures: [],
  };
}
function withDirectory(run: (directory: string) => void): void {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "archguard-sdk-"));
  try { run(directory); } finally { fs.rmSync(directory, { recursive: true, force: true }); }
}

test("wrong task identity is rejected, corrected result writes atomically, old task stays rejected", () => withDirectory(directory => {
  const request = fixture(directory);
  const result = passing(request);
  assert.throws(() => writeMutationResult(request, { ...result, requestId: "old-task" }), /identity/);
  assert.equal(fs.existsSync(request.resultPath), false);
  writeMutationResult(request, result);
  assert.deepEqual(JSON.parse(fs.readFileSync(request.resultPath, "utf8")), result);
  assert.throws(() => validateMutationResult(result, { ...request, runId: "other-run" }), /identity/);
  assert.deepEqual(fs.readdirSync(directory), ["result.json"]);
}));

test("assertion counts require executed test IDs; mixed hook failure stays distinct", () => withDirectory(directory => {
  const request = fixture(directory);
  const failure = { kind: "assertion" as const, testId: "case-1", file: "boundary.test.ts", message: "expected boundary" };
  const killed = { ...passing(request), exitCode: 1, tests: { passed: 0, failed: 1, skipped: 0 }, failures: [failure] };
  assert.throws(() => validateMutationResult({ ...killed, failures: [{ ...failure, testId: null }] }, request), /executed test ID/);
  assert.throws(() => validateMutationResult({ ...killed, tests: { passed: 0, failed: 2, skipped: 0 } }, request), /failed count/);
  assert.deepEqual(validateMutationResult(killed, request), killed);
  const mixed = { ...killed, failures: [failure, { kind: "hook" as const, testId: null, file: null, message: "late teardown" }] };
  assert.equal(validateMutationResult(mixed, request).failures[1]?.kind, "hook");
  assert.throws(() => validateMutationResult({ ...mixed, exitCode: 0 }, request), /exit zero/);
}));

test("assigned request refuses symlinks, excessive bytes and wrong result path", () => withDirectory(directory => {
  const request = fixture(directory);
  const requestFile = path.join(directory, "request.json");
  const previousRequest = process.env.ARCHGUARD_MUTATION_REQUEST;
  const previousResult = process.env.ARCHGUARD_MUTATION_RESULT;
  try {
    process.env.ARCHGUARD_MUTATION_REQUEST = requestFile;
    process.env.ARCHGUARD_MUTATION_RESULT = request.resultPath;
    fs.writeFileSync(requestFile, JSON.stringify(request));
    assert.deepEqual(readMutationRequest(), request);
    process.env.ARCHGUARD_MUTATION_RESULT = path.join(directory, "wrong.json");
    assert.throws(readMutationRequest, /disagrees/);
    process.env.ARCHGUARD_MUTATION_RESULT = request.resultPath;
    const link = path.join(directory, "linked.json"); fs.symlinkSync(requestFile, link);
    process.env.ARCHGUARD_MUTATION_REQUEST = link;
    assert.throws(readMutationRequest);
    process.env.ARCHGUARD_MUTATION_REQUEST = requestFile;
    fs.writeFileSync(requestFile, " ".repeat(65_537));
    assert.throws(readMutationRequest, /bounded regular/);
    fs.writeFileSync(requestFile, Buffer.from([0xff]));
    assert.throws(readMutationRequest, /encoded data/);
    fs.writeFileSync(requestFile, Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), Buffer.from(JSON.stringify(request))]));
    assert.throws(readMutationRequest, SyntaxError);
    fs.writeFileSync(requestFile, JSON.stringify(request).replace('"schemaVersion":1', '"schemaVersion":2,"schemaVersion":1'));
    assert.throws(readMutationRequest, /duplicate fields/);
    fs.writeFileSync(requestFile, JSON.stringify(request));
    assert.deepEqual(readMutationRequest(), request);
  } finally {
    if (previousRequest === undefined) delete process.env.ARCHGUARD_MUTATION_REQUEST; else process.env.ARCHGUARD_MUTATION_REQUEST = previousRequest;
    if (previousResult === undefined) delete process.env.ARCHGUARD_MUTATION_RESULT; else process.env.ARCHGUARD_MUTATION_RESULT = previousResult;
  }
}));

test("unknown fields, versions, accessor data and unsafe counts fail without coercion", () => withDirectory(directory => {
  const request = fixture(directory);
  assert.throws(() => validateMutationRequest({ ...request, schemaVersion: 2 }), /version/);
  assert.throws(() => validateMutationRequest({ ...request, executable: "injected" }), /unknown fields/);
  assert.throws(() => validateMutationRequest({ ...request, requestId: "\ud800" }), /well-formed/);
  assert.throws(() => validateMutationRequest({ ...request, phase: "baseline" }), /cannot name/);
  assert.throws(() => validateMutationRequest({ ...request, maxResultBytes: Number.POSITIVE_INFINITY }), /integer/);
  assert.throws(() => validateMutationResult({ ...passing(request), tests: { passed: Number.MAX_SAFE_INTEGER, failed: 0, skipped: 1 } }, request), /total test count/);
  const accessor = { ...passing(request) };
  Object.defineProperty(accessor, "complete", { get: () => true });
  assert.throws(() => validateMutationResult(accessor, request), /accessors/);
  const overridden = { ...passing(request) };
  Object.defineProperty(overridden, "toJSON", { value: () => ({ complete: true }) });
  assert.throws(() => writeMutationResult(request, overridden), /unknown fields/);
  assert.equal(validateMutationResult(passing(request), request).complete, true);
}));

test("encoding budget rejects escaped errors before replacing an existing good record", () => withDirectory(directory => {
  const request = fixture(directory);
  writeMutationResult(request, passing(request));
  const before = fs.readFileSync(request.resultPath);
  const result = { ...passing(request), exitCode: 1, tests: { passed: 0, failed: 1, skipped: 0 }, failures: [{ kind: "assertion" as const, testId: "case", file: null, message: "\n".repeat(2_000) }] };
  assert.throws(() => writeMutationResult({ ...request, maxResultBytes: 1_024 }, result), /encoding budget/);
  assert.deepEqual(fs.readFileSync(request.resultPath), before);
  writeMutationResult(request, result);
  assert.equal(JSON.parse(fs.readFileSync(request.resultPath, "utf8")).failures[0].message.length, 2_000);
  assert.deepEqual(fs.readdirSync(directory), ["result.json"]);
}));

test("shared wire ceilings accept their boundary and reject one extra byte or failure", () => withDirectory(directory => {
  const request = fixture(directory);
  assert.equal(validateMutationRequest({ ...request, maxResultBytes: mutationProtocolLimits.maxResultBytes }).maxResultBytes, 8_388_608);
  assert.throws(() => validateMutationRequest({ ...request, maxResultBytes: mutationProtocolLimits.maxResultBytes + 1 }), /budget/);
  const failure = { kind: "assertion" as const, testId: "case", file: null, message: "x".repeat(8_192) };
  const result = { ...passing(request), exitCode: 1, tests: { passed: 0, failed: 1, skipped: 0 }, failures: [failure] };
  assert.equal(validateMutationResult(result, request).failures[0]?.message.length, 8_192);
  assert.throws(() => validateMutationResult({ ...result, failures: [{ ...failure, message: failure.message + "x" }] }, request), /bounded well-formed string/);
  const failures = Array.from({ length: 1_024 }, () => ({ ...failure, message: "assertion control" }));
  assert.equal(validateMutationResult({ ...result, failures }, request).failures.length, 1_024);
  assert.throws(() => validateMutationResult({ ...result, failures: [...failures, failure] }, request), /bounded plain array/);
}));

test("failure accessors, custom iteration and proxy traps are rejected without invoking code", () => withDirectory(directory => {
  const request = fixture(directory);
  let invoked = false;
  const failures: unknown[] = [];
  Object.defineProperty(failures, "0", { get: () => { invoked = true; return {}; } });
  assert.throws(() => validateMutationResult({ ...passing(request), failures }, request), /accessors/);
  assert.equal(invoked, false);
  const iterable: unknown[] = [];
  Object.defineProperty(iterable, Symbol.iterator, { value: function* () { invoked = true; while (true) yield {}; } });
  assert.throws(() => validateMutationResult({ ...passing(request), failures: iterable }, request), /indexed data/);
  assert.equal(invoked, false);
  const proxy = new Proxy(passing(request), { getPrototypeOf() { invoked = true; throw new Error("proxy trap"); } });
  assert.throws(() => validateMutationResult(proxy, request), /plain object/);
  assert.equal(invoked, false);
  assert.equal(validateMutationResult(passing(request), request).complete, true);
}));

test("null-prototype protocol records are accepted and custom prototypes remain rejected", () => withDirectory(directory => {
  const request = fixture(directory);
  const plainRequest = Object.assign(Object.create(null), request);
  assert.deepEqual(validateMutationRequest(plainRequest), request);
  const result = Object.assign(Object.create(null), passing(request), {
    tests: Object.assign(Object.create(null), { passed: 1, failed: 0, skipped: 0 }),
  });
  assert.deepEqual(validateMutationResult(result, plainRequest), passing(request));
  const custom = Object.setPrototypeOf({ ...request }, { extra: "custom prototype" });
  assert.throws(() => validateMutationRequest(custom), /plain data/);
  assert.deepEqual(validateMutationRequest(request), request);
}));

test("proxy failure arrays cannot execute prototype traps during validation", () => withDirectory(directory => {
  const request = fixture(directory);
  let invoked = false;
  const failures = new Proxy([], {
    getPrototypeOf() { invoked = true; throw new Error("array proxy trap"); },
  });
  assert.throws(() => validateMutationResult({ ...passing(request), failures }, request), /bounded plain array/);
  assert.equal(invoked, false);
  assert.deepEqual(validateMutationResult(passing(request), request).failures, []);
}));

test("task and failed-test identities reject empty or NUL data while empty messages remain valid", () => withDirectory(directory => {
  const request = fixture(directory);
  for (const field of ["requestId", "runId"] as const) {
    for (const invalid of ["", "invalid\0identity"]) {
      assert.throws(() => validateMutationRequest({ ...request, [field]: invalid }), /bounded well-formed string/);
    }
  }
  const failed = { ...passing(request), exitCode: 1, tests: { passed: 0, failed: 1, skipped: 0 },
    failures: [{ kind: "assertion" as const, testId: "boundary-case", file: null, message: "" }] };
  assert.equal(validateMutationResult(failed, request).failures[0]?.message, "");
  assert.throws(() => validateMutationResult({ ...failed, failures: [{ ...failed.failures[0]!, testId: "" }] }, request), /bounded well-formed string/);
  assert.deepEqual(validateMutationRequest(request), request);
}));

test("required protocol fields must be own data and cannot invoke inherited getters", () => withDirectory(directory => {
  const request = fixture(directory);
  const missing: Record<string, unknown> = { ...request };
  delete missing.requestId;
  const replaced = { ...missing, unexpected: "replacement field" };
  const previous = Object.getOwnPropertyDescriptor(Object.prototype, "requestId");
  let invoked = 0;
  try {
    Object.defineProperty(Object.prototype, "requestId", {
      configurable: true,
      get() { invoked++; return request.requestId; },
    });
    assert.throws(() => validateMutationRequest(missing), /missing or unknown fields/);
    assert.equal(invoked, 0);
    assert.throws(() => validateMutationRequest(replaced), /missing or unknown fields/);
    assert.equal(invoked, 0);
    assert.deepEqual(validateMutationRequest(request), request);
  } finally {
    if (previous) Object.defineProperty(Object.prototype, "requestId", previous);
    else delete (Object.prototype as { requestId?: unknown }).requestId;
  }
}));

function sdkAssignedRequest(
  directory: string,
  run: (request: TestExecutionRequest, requestFile: string) => void,
): void {
  const request = fixture(directory);
  const requestFile = path.join(directory, "request.json");
  const previousRequest = process.env.ARCHGUARD_MUTATION_REQUEST;
  const previousResult = process.env.ARCHGUARD_MUTATION_RESULT;
  try {
    fs.writeFileSync(requestFile, JSON.stringify(request));
    process.env.ARCHGUARD_MUTATION_REQUEST = requestFile;
    process.env.ARCHGUARD_MUTATION_RESULT = request.resultPath;
    run(request, requestFile);
  } finally {
    if (previousRequest === undefined) delete process.env.ARCHGUARD_MUTATION_REQUEST;
    else process.env.ARCHGUARD_MUTATION_REQUEST = previousRequest;
    if (previousResult === undefined) delete process.env.ARCHGUARD_MUTATION_RESULT;
    else process.env.ARCHGUARD_MUTATION_RESULT = previousResult;
  }
}

test("baseline and mutation requests enforce their distinct mutation identities", () => withDirectory(directory => {
  const request = fixture(directory);
  const baseline = { ...request, phase: "baseline" as const, mutationId: null };
  assert.deepEqual(validateMutationRequest(baseline), baseline);
  assert.deepEqual(validateMutationRequest(request), request);
  for (const mutationId of [null, "", "g".repeat(64)]) {
    assert.throws(() => validateMutationRequest({ ...request, mutationId }));
  }
  assert.throws(() => validateMutationRequest({ ...baseline, mutationId: request.mutationId }), /cannot name/);
  assert.throws(() => validateMutationRequest({ ...request, phase: "other" }), /unknown phase/);
  assert.deepEqual(validateMutationRequest(baseline), baseline);
}));

test("callable values cannot impersonate plain request data", () => withDirectory(directory => {
  const request = fixture(directory);
  const callable = () => undefined;
  Reflect.deleteProperty(callable, "length");
  Reflect.deleteProperty(callable, "name");
  Object.setPrototypeOf(callable, Object.prototype);
  Object.assign(callable, request);
  assert.throws(() => validateMutationRequest(callable), /plain object/);
  assert.deepEqual(validateMutationRequest(request), request);
  assert.deepEqual(validateMutationRequest(Object.assign(Object.create(null), request)), request);
  assert.throws(() => validateMutationRequest(Object.setPrototypeOf({ ...request }, { custom: true })), /plain data/);
}));

test("assigned request files require absolute paths even when a relative file exists", () => withDirectory(directory => {
  sdkAssignedRequest(directory, (request, requestFile) => {
    assert.deepEqual(readMutationRequest(), request);
    const relative = path.relative(process.cwd(), requestFile);
    assert.equal(path.isAbsolute(relative), false);
    process.env.ARCHGUARD_MUTATION_REQUEST = relative;
    assert.throws(readMutationRequest, /absolute file/);
    process.env.ARCHGUARD_MUTATION_REQUEST = requestFile;
    assert.deepEqual(readMutationRequest(), request);
  });
}));

test("formatted request JSON is accepted while escaped duplicate keys remain rejected", () => withDirectory(directory => {
  sdkAssignedRequest(directory, (request, requestFile) => {
    fs.writeFileSync(requestFile, `\n\t${JSON.stringify(request, null, 2)} \r\n`);
    assert.deepEqual(readMutationRequest(), request);
    const duplicate = JSON.stringify(request).replace('"requestId":', '"request\\u0049d":"duplicate","requestId":');
    fs.writeFileSync(requestFile, duplicate);
    assert.throws(readMutationRequest, /duplicate fields/);
    fs.writeFileSync(requestFile, JSON.stringify(request));
    assert.deepEqual(readMutationRequest(), request);
  });
}));

test("the request byte ceiling includes an exact-size valid JSON document", () => withDirectory(directory => {
  sdkAssignedRequest(directory, (request, requestFile) => {
    const encoded = JSON.stringify(request);
    const padding = " ".repeat(mutationProtocolLimits.maxRequestBytes - Buffer.byteLength(encoded));
    // Put the closing brace at the final byte so truncated reads cannot parse a valid prefix.
    const exact = `${padding}${encoded}`;
    assert.equal(Buffer.byteLength(exact), mutationProtocolLimits.maxRequestBytes);
    fs.writeFileSync(requestFile, exact);
    assert.deepEqual(readMutationRequest(), request);
    fs.writeFileSync(requestFile, `${exact}\n`);
    assert.throws(readMutationRequest, /bounded regular/);
    fs.writeFileSync(requestFile, encoded);
    assert.deepEqual(readMutationRequest(), request);
  });
}));

test("result exit codes accept integer endpoints and reject values outside the process range", () => withDirectory(directory => {
  const request = fixture(directory);
  const result = passing(request);
  for (const exitCode of [-1, 256, 1.5, Number.NaN, Number.POSITIVE_INFINITY, "0", null]) {
    assert.throws(() => validateMutationResult({ ...result, exitCode }, request), /process exit code/);
  }
  for (const exitCode of [0, 1, 255]) {
    assert.equal(validateMutationResult({ ...result, exitCode }, request).exitCode, exitCode);
  }
}));

test("a completed hook-only failure cannot declare exit zero", () => withDirectory(directory => {
  const request = fixture(directory);
  const result = passing(request);
  const hookFailure = { kind: "hook" as const, testId: null, file: null, message: "after-all failed" };
  const bad = { ...result, failures: [hookFailure] };
  assert.throws(() => validateMutationResult(bad, request), /exit zero/);
  const corrected = { ...bad, exitCode: 1 };
  assert.deepEqual(validateMutationResult(corrected, request), corrected);
  assert.deepEqual(validateMutationResult(result, request), result);
}));
