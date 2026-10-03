import assert from "node:assert/strict";
import { test } from "node:test";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";

test("counterexamples follow function ownership after comments shift lines and reject unknown owners", () => {
  const owned = fs.mkdtempSync(path.join(os.tmpdir(), "archguard-path-evidence-"));
  try {
    const root = path.join(owned, "source");
    fs.mkdirSync(path.join(root, "packages/shared/src"), { recursive: true });
    const source = "// Leading documentation changes source lines.\n".repeat(18) + String.raw`export function isExplicitRelativePath(value) {
  return value.startsWith("./") ||
    (value.startsWith("../") || value.startsWith(".\\")) ||
    value.startsWith("..\\");
}
export function normalizeProjectPathForDispatch(value) { return value; }
`;
    fs.writeFileSync(path.join(root, "packages/shared/src/path.ts"), source);
    fs.writeFileSync(path.join(root, "LICENSE"), "Authored test fixture, MIT\n");
    const start = source.indexOf("||", source.indexOf('value.startsWith("../")'));
    const location = { file: "packages/shared/src/path.ts", start, end: start + 2,
      line: source.slice(0, start).split("\n").length, endLine: source.slice(0, start).split("\n").length };
    assert.notEqual(location.line, 18);
    const id = "a".repeat(64);
    const site = { id, location, expected: "||", replacement: "&&", owner: { name: "isExplicitRelativePath" } };
    const plan = { root, files: [{ path: location.file, sha256: createHash("sha256").update(source).digest("hex") }], sites: [site] };
    const report = { root, results: [{ mutationId: id, location, outcome: "survived" }] };
    const planPath = path.join(owned, "plan.json"); const reportPath = path.join(owned, "report.json");
    fs.writeFileSync(planPath, JSON.stringify(plan)); fs.writeFileSync(reportPath, JSON.stringify(report));
    const invoke = output => spawnSync(process.execPath, ["research/code-quality/product/inspect-path.mjs", root, planPath, reportPath, output], { encoding: "utf8", timeout: 10000 });
    const output = path.join(owned, "evidence"); const result = invoke(output);
    assert.equal(result.status, 0, result.stderr);
    const [row] = JSON.parse(fs.readFileSync(path.join(output, "counterexamples.json"), "utf8"));
    assert.equal(row.function, "isExplicitRelativePath");
    assert.ok(row.observedDifferences.some(example => example.input === "../repo" && example.original === true && example.mutant === false));
    assert.ok(row.examples.some(example => example.input === "./repo" && example.original === true && example.mutant === true));
    site.owner.name = "unrecognizedFunction"; fs.writeFileSync(planPath, JSON.stringify(plan));
    const unknown = path.join(owned, "unknown"); const rejected = invoke(unknown);
    assert.equal(rejected.status, 1); assert.match(rejected.stderr, /owner/);
    assert.equal(fs.existsSync(unknown), false);
    const external = path.join(owned, "external-target.ts"); fs.writeFileSync(external, "preserved external bytes");
    site.owner.name = "isExplicitRelativePath"; site.id = "../external-target";
    report.results[0].mutationId = site.id;
    fs.writeFileSync(planPath, JSON.stringify(plan)); fs.writeFileSync(reportPath, JSON.stringify(report));
    const invalidIdOutput = path.join(owned, "invalid-id"); const invalidId = invoke(invalidIdOutput);
    assert.equal(invalidId.status, 1); assert.match(invalidId.stderr, /mutation ID/);
    assert.equal(fs.existsSync(invalidIdOutput), false);
    assert.equal(fs.readFileSync(external, "utf8"), "preserved external bytes");
    assert.equal(fs.readFileSync(path.join(root, "packages/shared/src/path.ts"), "utf8"), source);
  } finally { fs.rmSync(owned, { recursive: true, force: true }); }
});
