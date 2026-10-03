import assert, { AssertionError } from "node:assert/strict";
import { readMutationRequest, writeMutationResult } from "../../sdk/mutator.ts";
import type { TestFailure } from "../../sdk/mutator.ts";
import { approval, displayedMagnitude, firstAmount, invoiceTotal, qualifiesForFreeShipping, sumAmounts } from "./domain.ts";

// This small synchronous harness reports actual assertion outcomes. A project
// with Vitest should use MutationVitestReporter instead.
const mode = process.argv[2];
if (mode !== "weak" && mode !== "strong") throw new Error("Choose weak or strong tests");
const cases: [string, () => void][] = [
  ["shipping above minimum", () => assert.equal(qualifiesForFreeShipping({ subtotal: 120, member: false, domestic: true }), true)],
  ["ordinary untaxed invoice", () => assert.equal(invoiceTotal(100, 0), 100)],
  ["approval amount", () => assert.equal(approval({ subtotal: 75, member: false, domestic: false }).amount, 75)],
  ["amount list", () => assert.equal(sumAmounts([10, 20]), 30)],
  ["first imported amount", () => assert.equal(firstAmount([10, 20]), 10)],
  ["displayed magnitude", () => assert.equal(displayedMagnitude(5), 5)],
];
if (mode === "strong") cases.push(
  ["shipping exact boundary", () => assert.equal(qualifiesForFreeShipping({ subtotal: 100, member: false, domestic: true }), true)],
  ["shipping under minimum", () => assert.equal(qualifiesForFreeShipping({ subtotal: 99, member: false, domestic: true }), false)],
  ["shipping international", () => assert.equal(qualifiesForFreeShipping({ subtotal: 120, member: true, domestic: false }), false)],
  ["shipping both conditions absent", () => assert.equal(qualifiesForFreeShipping({ subtotal: 20, member: false, domestic: false }), false)],
  ["tax adds to subtotal", () => assert.equal(invoiceTotal(100, 8), 108)],
  ["approval promise", () => assert.equal(approval({ subtotal: 75, member: true, domestic: true }).approved, true)],
  ["empty amount list", () => assert.equal(sumAmounts([]), 0)],
  ["empty imported list", () => assert.equal(firstAmount([]), 0)],
  ["single imported amount", () => assert.equal(firstAmount([10]), 10)],
  ["nonfinite magnitude", () => assert.throws(() => displayedMagnitude(Number.NaN), /finite/)],
  ["signed zero magnitude", () => assert.equal(Object.is(displayedMagnitude(-0), 0), true)],
  ["negative amount magnitude", () => assert.equal(displayedMagnitude(-5), 5)],
);
const failures: TestFailure[] = [];
let passed = 0;
for (const [id, run] of cases) {
  try { run(); passed++; }
  catch (error) {
    failures.push({ kind: error instanceof AssertionError ? "assertion" : "runtime", testId: id,
      file: "examples/code-quality/run-domain.ts", message: error instanceof Error ? error.message : String(error) });
  }
}
const exitCode = failures.length === 0 ? 0 : 1;
if (process.env.ARCHGUARD_MUTATION_REQUEST) {
  const request = readMutationRequest();
  writeMutationResult(request, { schemaVersion: 1, requestId: request.requestId, runId: request.runId,
    inputDigest: request.inputDigest, complete: true, exitCode, reason: "finished",
    tests: { passed, failed: failures.length, skipped: 0 }, failures });
}
console.log(`${mode}: ${passed} passed, ${failures.length} failed`);
process.exitCode = exitCode;
