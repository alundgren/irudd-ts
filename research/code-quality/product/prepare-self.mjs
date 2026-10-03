#!/usr/bin/env node
import * as fs from "node:fs";
import * as path from "node:path";

const [repositoryArgument, dependencyArgument, outputArgument] = process.argv.slice(2);
if (!repositoryArgument || !dependencyArgument || !outputArgument || process.argv.length !== 5) throw new Error("Usage: node prepare-self.mjs <Archguard repository> <self-contained node_modules> <new output directory>");
const repository = fs.realpathSync(repositoryArgument);
const dependencies = fs.realpathSync(dependencyArgument);
const requestedOutput = path.resolve(outputArgument);
const output = path.join(fs.realpathSync(path.dirname(requestedOutput)), path.basename(requestedOutput));
if (fs.existsSync(output)) throw new Error("Output directory must be new");
if (output === repository || output.startsWith(repository + path.sep) || output === dependencies || output.startsWith(dependencies + path.sep)) throw new Error("Output must be outside source and dependency inputs");
fs.mkdirSync(output, { recursive: true });
const support = path.join(output, "support");
const tests = path.join(output, "tests");
fs.mkdirSync(support); fs.mkdirSync(tests);
for (const name of ["mutator.ts", "mutator-vitest-reporter.ts"]) fs.copyFileSync(path.join(repository, "sdk", name), path.join(support, name));
for (const name of ["mutator_sdk", "mutator_reporter"]) {
  const original = fs.readFileSync(path.join(repository, `tests/${name}.test.ts`), "utf8");
  const adapted = original.replace('import { test } from "node:test";', 'import { test } from "vitest";');
  if (adapted === original) throw new Error("Expected Node test import is unavailable");
  fs.writeFileSync(path.join(tests, `${name}.test.ts`), adapted);
}
fs.writeFileSync(path.join(support, "vite.config.ts"), `import { defineConfig } from "vite-plus/test/config";
export default defineConfig({ test: { include: ["quality-tests/*.test.ts"],
  pool: "forks", maxWorkers: 1, fileParallelism: false,
  reporters: ["./quality-support/mutator-vitest-reporter.ts"],
}});
`);
for (const [profile, source, test] of [
  ["protocol", "sdk/mutator.ts", "quality-tests/mutator_sdk.test.ts"],
  ["reporter", "sdk/mutator-vitest-reporter.ts", "quality-tests/mutator_reporter.test.ts"],
]) {
  const config = { schemaVersion: 1, plan: { schemaVersion: 1, selection: { include: [source] } }, execution: {
    command: [process.execPath, "node_modules/vite-plus/bin/vp", "test", "run", test, "--config", "quality-support/vite.config.ts"],
    workspace: { exclude: [".git/**", "**/node_modules/**", "node_modules/**", "target/**"], include: ["sdk/mutator.ts", "sdk/mutator-vitest-reporter.ts"], dependencies: [
      { source: dependencies, destination: "node_modules" }, { source: support, destination: "quality-support" },
      { source: tests, destination: "quality-tests" }] },
    limits: { workers: 1, maxInventoryBytes: 67_108_864, commandTimeoutMs: 30000, runTimeoutMs: 3600000 },
  } };
  fs.writeFileSync(path.join(output, `${profile}.json`), JSON.stringify(config, null, 2) + "\n");
}
fs.writeFileSync(path.join(output, "dryer.json"), JSON.stringify({ schemaVersion: 1,
  selection: { include: ["sdk/**/*.ts", "examples/code-quality/**/*.ts"] } }, null, 2) + "\n");
fs.writeFileSync(path.join(output, "adaptation.json"), JSON.stringify({
  originalTests: ["tests/mutator_sdk.test.ts", "tests/mutator_reporter.test.ts"], change: "Only the test registration import changes from node:test to vitest; assertions and source imports remain identical.",
  instrumentation: "The reporter and its protocol helper are fixed trusted support copies so mutations in the tested SDK cannot alter result publication.",
}, null, 2) + "\n");
console.log(`Self-dogfooding profiles written to ${output}`);
