#!/usr/bin/env node
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { createHash } from "node:crypto";
import { pathToFileURL } from "node:url";

const [checkout, planFile, reportFile, outputArgument] = process.argv.slice(2);
if (!checkout || !planFile || !reportFile || !outputArgument || process.argv.length !== 6) throw new Error("Usage: node inspect-path.mjs <trusted T3 checkout> <path plan JSON> <path report JSON> <new output directory>");
const requested = path.resolve(outputArgument);
const output = path.join(fs.realpathSync(path.dirname(requested)), path.basename(requested));
const root = fs.realpathSync(checkout);
if (output === root || output.startsWith(root + path.sep) || fs.existsSync(output)) throw new Error("Choose a new output directory outside T3");
const source = fs.readFileSync(path.join(root, "packages/shared/src/path.ts"));
const plan = JSON.parse(fs.readFileSync(planFile, "utf8"));
const report = JSON.parse(fs.readFileSync(reportFile, "utf8"));
assert.equal(plan.root, root); assert.equal(report.root, root);
assert.equal(plan.files.length, 1);
assert.equal(plan.files[0].path, "packages/shared/src/path.ts");
assert.equal(createHash("sha256").update(source).digest("hex"), plan.files[0].sha256);
const normalizationInputs = ["", "/", "\\", "C:/", "C:\\", "x/", "///", "\\\\"];
const controls = new Map([
  ["isExplicitRelativePath", { name: "isExplicitRelativePath", inputs: ["../repo", ".\\repo", "./repo", "..\\repo"] }],
  ["isRootPath", { name: "normalizeProjectPathForDispatch", inputs: normalizationInputs }],
  ["trimTrailingPathSeparators", { name: "normalizeProjectPathForDispatch", inputs: normalizationInputs }],
]);
const mutations = report.results.filter(row => row.outcome === "survived").map(result => {
  const site = plan.sites.find(row => row.id === result.mutationId);
  assert.ok(site); assert.deepEqual(site.location, result.location);
  assert.ok(typeof site.id === "string" && /^[a-f0-9]{64}$/.test(site.id), "Invalid mutation ID");
  const control = controls.get(site.owner?.name);
  assert.ok(control, "Unexpected path mutation owner");
  assert.ok((site.expected === "||" && site.replacement === "&&") || (site.expected === "0" && site.replacement === "1"), "Unexpected path control edit");
  const { start, end, file } = site.location;
  assert.equal(file, plan.files[0].path);
  assert.ok(Number.isSafeInteger(start) && Number.isSafeInteger(end) && start >= 0 && end > start && end <= source.length);
  assert.equal(source.subarray(start, end).toString(), site.expected);
  return { site, control };
});
fs.mkdirSync(output);
fs.writeFileSync(path.join(output, "package.json"), '{"type":"module"}\n');
fs.copyFileSync(path.join(root, "LICENSE"), path.join(output, "LICENSE"));
fs.writeFileSync(path.join(output, "original.ts"), source);
const original = await import(pathToFileURL(path.join(output, "original.ts")).href);
const rows = [];
for (const { site, control } of mutations) {
  const { start, end, line } = site.location;
  assert.equal(source.subarray(start, end).toString(), site.expected);
  const mutated = Buffer.concat([source.subarray(0, start), Buffer.from(site.replacement), source.subarray(end)]);
  const filename = path.join(output, `${site.id}.ts`); fs.writeFileSync(filename, mutated);
  const module = await import(pathToFileURL(filename).href);
  const { name, inputs } = control;
  const examples = inputs.map(input => ({ input, original: original[name](input), mutant: module[name](input) }));
  rows.push({ id: site.id, line, expected: site.expected, replacement: site.replacement, function: name,
    examples, observedDifferences: examples.filter(row => !Object.is(row.original, row.mutant)) });
}
fs.writeFileSync(path.join(output, "counterexamples.json"), JSON.stringify(rows, null, 2) + "\n");
console.log(JSON.stringify(rows.map(({ line, expected, replacement, observedDifferences }) => ({ line, expected, replacement, observedDifferences })), null, 2));
