import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

// Generate explicit local profiles. This performs no installation and makes no
// changes inside the T3 checkout or dependency installation.
const [checkoutArgument, dependencyArgument, outputArgument] = process.argv.slice(2);
if (!checkoutArgument || !dependencyArgument || !outputArgument || process.argv.length !== 5) {
  throw new Error("Usage: node examples/code-quality/prepare-t3.ts <T3 checkout> <self-contained node_modules> <new output directory>");
}
const checkout = fs.realpathSync(checkoutArgument);
const dependencies = fs.realpathSync(dependencyArgument);
const requestedOutput = path.resolve(outputArgument);
const output = path.join(fs.realpathSync(path.dirname(requestedOutput)), path.basename(requestedOutput));
if (fs.existsSync(output)) throw new Error("Output directory must be new");
if (output === checkout || output.startsWith(checkout + path.sep) || output === dependencies || output.startsWith(dependencies + path.sep)) {
  throw new Error("Output must be outside source and dependency inputs");
}
const repository = fileURLToPath(new URL("../../", import.meta.url));
const modules = ["path", "hostClassification", "delimitedPreview", "gitPatchPath"];
for (const name of modules) for (const suffix of [".ts", ".test.ts"]) {
  if (!fs.statSync(path.join(checkout, "packages/shared/src", name + suffix)).isFile()) throw new Error(`Missing T3 source ${name}${suffix}`);
}
if (!fs.statSync(path.join(dependencies, "vite-plus/bin/vp")).isFile()) throw new Error("Explicit Vite Plus installation is required");
fs.mkdirSync(output, { recursive: true });
const support = path.join(output, "support");
fs.mkdirSync(support);
fs.writeFileSync(path.join(support, "vite.config.ts"), `import { defineConfig } from "vite-plus/test/config";
export default defineConfig({ test: {
  include: ["packages/shared/src/{path,hostClassification,delimitedPreview,gitPatchPath}.test.ts", "quality-controls/*.test.ts"],
  pool: "forks", maxWorkers: 1, fileParallelism: false,
  reporters: ["./sdk/mutator-vitest-reporter.ts"],
}});
`);
for (const name of modules) {
  const plan = { schemaVersion: 1, selection: { include: [`packages/shared/src/${name}.ts`] } };
  const config = { schemaVersion: 1, plan, execution: {
    command: [process.execPath, "node_modules/vite-plus/bin/vp", "test", "run", `packages/shared/src/${name}.test.ts`, "--config", "quality-support/vite.config.ts"],
    workspace: { exclude: [".git/**", "**/node_modules/**", "node_modules/**", "target/**"], include: ["package.json", "packages/shared/package.json", `packages/shared/src/${name}.ts`, `packages/shared/src/${name}.test.ts`],
      dependencies: [{ source: dependencies, destination: "node_modules" },
        { source: path.join(repository, "sdk"), destination: "sdk" }, { source: support, destination: "quality-support" }] },
    limits: { workers: 1, maxInventoryBytes: 67_108_864, commandTimeoutMs: 30000, runTimeoutMs: 3600000 },
  } };
  fs.writeFileSync(path.join(output, `${name}.json`), JSON.stringify(config, null, 2) + "\n");
  fs.writeFileSync(path.join(output, `${name}-plan.json`), JSON.stringify(plan, null, 2) + "\n");
  if (name === "path") {
    const controls = path.join(output, "controls");
    fs.mkdirSync(controls);
    fs.writeFileSync(path.join(controls, "path.test.ts"), `import { expect, it } from "vite-plus/test";
import { isExplicitRelativePath, normalizeProjectPathForDispatch } from "../packages/shared/src/path.ts";

it("recognizes relative parent and Windows current-directory prefixes", () => {
  expect(isExplicitRelativePath("../repo")).toBe(true);
  expect(isExplicitRelativePath(".\\\\repo")).toBe(true);
  expect(isExplicitRelativePath("./repo")).toBe(true);
  expect(isExplicitRelativePath("..\\\\repo")).toBe(true);
  expect(isExplicitRelativePath("~/repo")).toBe(false);
});

it("trims a trailing separator after a single-character project name", () => {
  expect(normalizeProjectPathForDispatch("x/")).toBe("x");
  expect(normalizeProjectPathForDispatch("x")).toBe("x");
  expect(normalizeProjectPathForDispatch("/repo/")).toBe("/repo");
  expect(normalizeProjectPathForDispatch("/")).toBe("/");
});
`);
    config.execution.command.splice(5, 0, "quality-controls/path.test.ts");
    config.execution.workspace.dependencies.push({ source: controls, destination: "quality-controls" });
    fs.writeFileSync(path.join(output, "path-strengthened.json"), JSON.stringify(config, null, 2) + "\n");
  }
}
fs.writeFileSync(path.join(output, "dryer.json"), JSON.stringify({ schemaVersion: 1,
  selection: { include: modules.map(name => `packages/shared/src/${name}.ts`) },
}, null, 2) + "\n");
fs.writeFileSync(path.join(output, "dryer-native.json"), JSON.stringify({ schemaVersion: 1,
  selection: { include: ["apps/mobile/src/native/T3ComposerEditor.tsx", "apps/mobile/src/native/T3ComposerEditor.ios.tsx", "apps/mobile/src/native/T3ComposerEditor.native.tsx"] },
}, null, 2) + "\n");
fs.writeFileSync(path.join(output, "dryer-native-exploratory.json"), JSON.stringify({ schemaVersion: 1,
  selection: { include: ["apps/mobile/src/native/T3ComposerEditor.tsx", "apps/mobile/src/native/T3ComposerEditor.ios.tsx", "apps/mobile/src/native/T3ComposerEditor.native.tsx"] },
  similarityThreshold: 0.7,
}, null, 2) + "\n");
console.log(`Profiles written to ${output}. Run archguard with --root ${checkout}.`);
