#!/usr/bin/env node
import * as fs from "node:fs";
import * as path from "node:path";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";

const [binaryArgument, repositoryArgument, outputArgument] = process.argv.slice(2);
if (!binaryArgument || !repositoryArgument || !outputArgument || process.argv.length !== 5) {
  throw new Error("Usage: node replay-dryer.mjs <trusted Archguard binary> <repository> <new output directory>");
}
const binary = fs.realpathSync(binaryArgument);
const repository = fs.realpathSync(repositoryArgument);
const requested = path.resolve(outputArgument);
const output = path.join(fs.realpathSync(path.dirname(requested)), path.basename(requested));
if (fs.existsSync(output)) throw new Error("Output directory must be new");
if (output === repository || output.startsWith(repository + path.sep)) throw new Error("Output must be outside source inputs");
const corpus = path.join(repository, "research/code-quality/dryer/fixtures");
const cases = ["A", "B", "C", "D", "E", "F"];
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const sources = Object.fromEntries(cases.map(name => [name, hash(fs.readFileSync(path.join(corpus, `${name}.ts`)))]));
const modes = [
  ["default", {}],
  ["erased-properties", { properties: "erase" }],
  ["erased-locals", { localIdentifiers: "erase" }],
  ["preserved-literals", { literals: "value" }],
];
fs.mkdirSync(output);
const records = [];
for (const name of cases) {
  for (const [mode, normalization] of modes) {
    const label = `${name}-${mode}`;
    const configuration = { schemaVersion: 1, selection: { include: [`${name}.ts`] }, similarityThreshold: 0, normalization };
    const configPath = path.join(output, `${label}-config.json`);
    fs.writeFileSync(configPath, JSON.stringify(configuration, null, 2) + "\n");
    const args = ["dryer", "--root", corpus, "--config", configPath, "--json"];
    const result = spawnSync(binary, args, { encoding: "utf8", timeout: 30000, maxBuffer: 9 * 1024 * 1024 });
    fs.writeFileSync(path.join(output, `${label}.stdout`), result.stdout ?? "");
    fs.writeFileSync(path.join(output, `${label}.stderr`), result.stderr ?? "");
    const record = { label, command: [binary, ...args], exitCode: result.status, signal: result.signal,
      error: result.error?.message ?? null, configuration, sourceSha256: sources[name] };
    try {
      const report = JSON.parse(result.stdout);
      record.complete = report.complete;
      record.problems = report.problems;
      record.pairs = report.pairs.map(pair => ({
        left: pair.left.name, right: pair.right.name, similarity: pair.similarity,
        exactNormalizedMatch: pair.exactNormalizedMatch,
        leftOpaqueNodes: pair.leftOpaqueNodes, rightOpaqueNodes: pair.rightOpaqueNodes,
      }));
    } catch (error) { record.parseError = error.message; }
    records.push(record);
    console.log(JSON.stringify(record));
  }
}
const preserved = cases.every(name => hash(fs.readFileSync(path.join(corpus, `${name}.ts`))) === sources[name]);
fs.writeFileSync(path.join(output, "provenance.json"), JSON.stringify({
  scope: "Six archived authored syntax-only fixtures, analyzed separately. Undefined domain types and calls are not executed or compiler-checked.",
  interpretation: "Threshold zero retains each comparison. Scores do not decide whether refactoring is appropriate.",
  runtime: process.version, binary, binarySha256: hash(fs.readFileSync(binary)),
  sources, sourceBytesPreserved: preserved, records,
}, null, 2) + "\n");
if (!preserved || records.some(record => record.exitCode !== 0 || record.complete !== true)) process.exitCode = 1;
