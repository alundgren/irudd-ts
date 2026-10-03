import assert from "node:assert/strict";
import { test } from "node:test";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { spawnSync } from "node:child_process";

test("T3 profile generation refuses source aliases, corrects missing inputs and preserves existing output", () => {
  const owned = fs.mkdtempSync(path.join(os.tmpdir(), "archguard-profiles-"));
  try {
    const checkout = path.join(owned, "t3");
    const dependencies = path.join(owned, "dependencies");
    fs.mkdirSync(path.join(checkout, "packages/shared/src"), { recursive: true });
    fs.mkdirSync(path.join(dependencies, "vite-plus/bin"), { recursive: true });
    fs.writeFileSync(path.join(dependencies, "vite-plus/bin/vp"), "fixture launcher, never executed");
    const alias = path.join(owned, "alias"); fs.symlinkSync(checkout, alias);
    const invoke = (output: string) => spawnSync(process.execPath, ["examples/code-quality/prepare-t3.ts", checkout, dependencies, output], { encoding: "utf8", timeout: 10000 });
    const unsafe = invoke(path.join(alias, "profiles"));
    assert.equal(unsafe.status, 1); assert.match(unsafe.stderr, /outside source/);
    assert.equal(fs.existsSync(path.join(checkout, "profiles")), false);
    const output = path.join(owned, "profiles");
    assert.equal(invoke(output).status, 1);
    assert.equal(fs.existsSync(output), false);
    for (const name of ["path", "hostClassification", "delimitedPreview", "gitPatchPath"]) {
      for (const suffix of [".ts", ".test.ts"]) fs.writeFileSync(path.join(checkout, "packages/shared/src", name + suffix), "// authored input control\n");
    }
    const before = fs.readdirSync(path.join(checkout, "packages/shared/src"));
    const corrected = invoke(output); assert.equal(corrected.status, 0, corrected.stderr);
    const configuration = fs.readFileSync(path.join(output, "path.json"));
    const profile = JSON.parse(configuration.toString());
    assert.equal(profile.execution.command[0], process.execPath);
    assert.deepEqual(profile.plan.selection.include, ["packages/shared/src/path.ts"]);
    assert.deepEqual(fs.readdirSync(path.join(checkout, "packages/shared/src")), before);
    assert.equal(fs.existsSync(path.join(checkout, "profiles")), false);
    assert.equal(invoke(output).status, 1);
    assert.deepEqual(fs.readFileSync(path.join(output, "path.json")), configuration);
  } finally { fs.rmSync(owned, { recursive: true, force: true }); }
});
