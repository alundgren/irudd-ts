import * as fs from "node:fs";
import * as path from "node:path";
import { randomUUID } from "node:crypto";
import { types } from "node:util";

export interface TestExecutionRequest {
  schemaVersion: 1;
  requestId: string;
  runId: string;
  inputDigest: string;
  phase: "baseline" | "mutation";
  mutationId: string | null;
  resultPath: string;
  maxResultBytes: number;
}

export interface TestCounts { passed: number; failed: number; skipped: number }
export type TestFailureKind = "assertion" | "runtime" | "import" | "hook" | "suite" | "unhandled";
export interface TestFailure {
  kind: TestFailureKind;
  testId: string | null;
  file: string | null;
  message: string;
}
export interface TestExecutionResult {
  schemaVersion: 1;
  requestId: string;
  runId: string;
  inputDigest: string;
  complete: boolean;
  exitCode: number;
  reason: "finished" | "interrupted" | "infrastructureError";
  tests: TestCounts;
  failures: readonly TestFailure[];
}

export const mutationProtocolLimits = Object.freeze({
  maxRequestBytes: 65_536,
  maxResultBytes: 8_388_608,
  maxIdentityBytes: 4_096,
  maxMessageBytes: 8_192,
  maxFailures: 1_024,
});
const failureKinds = new Set(["assertion", "runtime", "import", "hook", "suite", "unhandled"]);
const digest = /^[a-f0-9]{64}$/;

function invalid(message: string): never { throw new Error(`Mutation protocol: ${message}`); }
function record(value: unknown, fields: readonly string[], label: string): Record<string, unknown> {
  if (value === null || typeof value !== "object" || types.isProxy(value) || Array.isArray(value)) invalid(`${label} must be a plain object`);
  const object = value as Record<string, unknown>;
  const prototype = Object.getPrototypeOf(object);
  if (prototype !== Object.prototype && prototype !== null) invalid(`${label} must contain plain data`);
  const keys = Reflect.ownKeys(object);
  if (keys.length !== fields.length || keys.some(key => typeof key !== "string" || !fields.includes(key))) invalid(`${label} has missing or unknown fields`);
  if (keys.some(key => !("value" in Object.getOwnPropertyDescriptor(object, key)!))) invalid(`${label} cannot contain accessors`);
  return object;
}
function array(value: unknown, maximum: number, label: string): readonly unknown[] {
  if (!Array.isArray(value) && types.isProxy(value) || Object.getPrototypeOf(value) !== Array.prototype || value.length > maximum) invalid(`${label} must be a bounded plain array`);
  const keys = Reflect.ownKeys(value);
  if (keys.length !== value.length + 1) invalid(`${label} must contain indexed data only`);
  for (let index = 0; index < value.length; index++) {
    const descriptor = Object.getOwnPropertyDescriptor(value, String(index));
    if (!descriptor || !("value" in descriptor)) invalid(`${label} cannot contain accessors or missing indices`);
  }
  return value;
}
function text(value: unknown, label: string, nonempty = true, maximum: number = mutationProtocolLimits.maxIdentityBytes): asserts value is string {
  if (typeof value !== "string" || (nonempty && value.length === 0) || value.includes("\0") || Buffer.byteLength(value) > maximum || /[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/.test(value)) invalid(`${label} is not a bounded well-formed string`);
}
function count(value: unknown, label: string): asserts value is number {
  if (!Number.isSafeInteger(value) || (value as number) < 0) invalid(`${label} must be a nonnegative safe integer`);
}
function nullableText(value: unknown, label: string): void { if (value !== null) text(value, label); }

export function validateMutationRequest(value: unknown): TestExecutionRequest {
  const request = record(value, ["schemaVersion", "requestId", "runId", "inputDigest", "phase", "mutationId", "resultPath", "maxResultBytes"], "request");
  if (request.schemaVersion !== 1) invalid("unsupported request version");
  text(request.requestId, "requestId"); text(request.runId, "runId"); text(request.inputDigest, "inputDigest");
  if (!digest.test(request.inputDigest)) invalid("inputDigest must be SHA256");
  if (request.phase !== "baseline" && request.phase !== "mutation") invalid("unknown phase");
  if (request.phase === "baseline" && request.mutationId !== null) invalid("baseline cannot name a mutation");
  if (request.phase === "mutation") {
    text(request.mutationId, "mutationId");
    if (!digest.test(request.mutationId)) invalid("mutationId must be SHA256");
  }
  text(request.resultPath, "resultPath");
  if (!path.isAbsolute(request.resultPath)) invalid("resultPath must be absolute");
  count(request.maxResultBytes, "maxResultBytes");
  if (request.maxResultBytes < 1_024 || request.maxResultBytes > mutationProtocolLimits.maxResultBytes) invalid("result byte budget is outside supported limits");
  return {
    schemaVersion: 1, requestId: request.requestId, runId: request.runId,
    inputDigest: request.inputDigest, phase: request.phase,
    mutationId: request.mutationId as string | null, resultPath: request.resultPath,
    maxResultBytes: request.maxResultBytes,
  };
}

function readBoundedRegularFile(file: string, limit: number): Buffer {
  const descriptor = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
  try {
    const metadata = fs.fstatSync(descriptor);
    if (!metadata.isFile() || metadata.size > limit) invalid("request must be a bounded regular file");
    const bytes = Buffer.alloc(limit + 1);
    let used = 0;
    while (used < bytes.length) {
      const read = fs.readSync(descriptor, bytes, used, bytes.length - used, null);
      if (read === 0) break;
      used += read;
    }
    if (used > limit) invalid("request exceeds its byte budget");
    return bytes.subarray(0, used);
  } finally { fs.closeSync(descriptor); }
}

export function readMutationRequest(): TestExecutionRequest {
  const assigned = process.env.ARCHGUARD_MUTATION_REQUEST;
  if (!assigned || !path.isAbsolute(assigned)) invalid("ARCHGUARD_MUTATION_REQUEST must name an absolute file");
  const raw = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(readBoundedRegularFile(assigned, mutationProtocolLimits.maxRequestBytes));
  const request = validateMutationRequest(JSON.parse(raw));
  rejectDuplicateRequestFields(raw);
  if (process.env.ARCHGUARD_MUTATION_RESULT !== request.resultPath) invalid("assigned result path disagrees with request");
  return request;
}

function rejectDuplicateRequestFields(raw: string): void {
  // Validated requests contain only scalar values. Scan their keys without
  // allowing JSON.parse to hide a repeated field behind its final value.
  const tokens = /"(?:\\.|[^"\\])*"|[^\s,{}:]+|[{},:]/gy;
  const fields = new Set<string>();
  let offset = 0;
  const next = (): string => {
    while (/[\t\r\n ]/.test(raw[offset] ?? "")) offset++;
    tokens.lastIndex = offset;
    const token = tokens.exec(raw);
    if (!token) invalid("request token unavailable");
    offset = tokens.lastIndex;
    return token[0];
  };
  if (next() !== "{") invalid("request must be an object");
  while (true) {
    const key = next();
    if (key === "}") break;
    const name = JSON.parse(key) as string;
    if (fields.has(name)) invalid("request contains duplicate fields");
    fields.add(name);
    if (next() !== ":") invalid("request field separator unavailable");
    next();
    const separator = next();
    if (separator === "}") break;
    if (separator !== ",") invalid("request contains non-scalar data");
  }
}

export function validateMutationResult(value: unknown, request: TestExecutionRequest): TestExecutionResult {
  validateMutationRequest(request);
  const result = record(value, ["schemaVersion", "requestId", "runId", "inputDigest", "complete", "exitCode", "reason", "tests", "failures"], "result");
  if (result.schemaVersion !== 1 || result.requestId !== request.requestId || result.runId !== request.runId || result.inputDigest !== request.inputDigest) invalid("result identity disagrees with request");
  if (typeof result.complete !== "boolean") invalid("complete must be boolean");
  if (!Number.isInteger(result.exitCode) || (result.exitCode as number) < 0 || (result.exitCode as number) > 255) invalid("exitCode must be a process exit code");
  if (!["finished", "interrupted", "infrastructureError"].includes(result.reason as string)) invalid("unknown completion reason");
  if (result.complete && result.reason !== "finished") invalid("incomplete reason cannot declare completion");
  const counts = record(result.tests, ["passed", "failed", "skipped"], "tests");
  count(counts.passed, "passed"); count(counts.failed, "failed"); count(counts.skipped, "skipped");
  count(counts.passed + counts.failed + counts.skipped, "total test count");
  const failureValues = array(result.failures, mutationProtocolLimits.maxFailures, "failures");
  const failedTests = new Set<string>();
  const failures: TestFailure[] = [];
  for (let index = 0; index < failureValues.length; index++) {
    const failure = record(failureValues[index], ["kind", "testId", "file", "message"], "failure");
    if (!failureKinds.has(failure.kind as string)) invalid("unknown failure kind");
    nullableText(failure.testId, "testId"); nullableText(failure.file, "file"); text(failure.message, "message", false, mutationProtocolLimits.maxMessageBytes);
    if (failure.kind === "assertion" && failure.testId === null) invalid("assertion needs an executed test ID");
    if (failure.testId !== null) failedTests.add(failure.testId as string);
    failures.push({ kind: failure.kind as TestFailureKind, testId: failure.testId as string | null, file: failure.file as string | null, message: failure.message });
  }
  if (result.complete && failedTests.size !== counts.failed) invalid("failed test IDs disagree with failed count");
  if (result.complete && result.exitCode === 0 && (counts.failed !== 0 || failureValues.length !== 0)) invalid("exit zero disagrees with failures");
  return {
    schemaVersion: 1, requestId: request.requestId, runId: request.runId,
    inputDigest: request.inputDigest, complete: result.complete, exitCode: result.exitCode as number,
    reason: result.reason as TestExecutionResult["reason"],
    tests: { passed: counts.passed, failed: counts.failed, skipped: counts.skipped }, failures,
  };
}

function boundedEncoding(value: unknown, limit: number): string {
  // JSON escaping can expand every character to six bytes; check before allocating.
  let conservative = 1;
  const pending: unknown[] = [value];
  while (pending.length > 0) {
    const next = pending.pop();
    if (typeof next === "string") conservative += next.length * 6 + 2;
    else if (next !== null && typeof next === "object") {
      if (Array.isArray(next)) { conservative += next.length + 2; pending.push(...next); }
      else { const entries = Object.entries(next); conservative += entries.length * 2 + 2; for (const [key, child] of entries) pending.push(key, child); }
    } else conservative += 32;
    if (conservative > limit) invalid("result exceeds conservative encoding budget");
  }
  const encoded = JSON.stringify(value);
  if (Buffer.byteLength(encoded) > limit) invalid("encoded result exceeds byte budget");
  return encoded;
}

export function writeMutationResult(request: TestExecutionRequest, value: TestExecutionResult): void {
  const result = validateMutationResult(value, request);
  const encoded = boundedEncoding(result, request.maxResultBytes);
  const destination = request.resultPath;
  const temporary = `${destination}.${randomUUID()}.tmp`;
  let descriptor: number | undefined;
  try {
    descriptor = fs.openSync(temporary, "wx", 0o600);
    fs.writeFileSync(descriptor, encoded); fs.fsyncSync(descriptor); fs.closeSync(descriptor); descriptor = undefined;
    fs.renameSync(temporary, destination);
  } finally {
    if (descriptor !== undefined) fs.closeSync(descriptor);
    fs.rmSync(temporary, { force: true });
  }
}
