#!/usr/bin/env node
// Uses an explicitly supplied, already installed Vite Plus/Vitest environment.
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";

if (process.argv.length !== 3) throw new Error("Usage: node scripts/check_mutator_vitest.mjs /absolute/installed/repository");
const installed = path.resolve(process.argv[2]);
const runner = path.join(installed, "node_modules/.bin/vp");
assert.ok(fs.statSync(runner).isFile(), "Explicit Vite Plus command must already be installed");
const reporter = fileURLToPath(new URL("../sdk/mutator-vitest-reporter.ts", import.meta.url));
const owned = fs.mkdtempSync(path.join(os.tmpdir(), "archguard-real-vitest-"));
const passing = 'import {test,expect} from "vitest"; test("boundary",()=>expect(2).toBe(2));';
const failing = passing.replace("toBe(2)", "toBe(3)");
const auxiliary = body => `export default class ControlReporter {onInit(context){this.context=context;} onTestRunEnd(){${body}}}`;
const cases = [
  {name:"passing", source:passing, complete:true, kinds:[]},
  {name:"assertion", source:failing, complete:true, kinds:["assertion"]},
  {name:"assertion-and-teardown", source:failing, extra:'globalSetup:["./setup.ts"],', setup:'export default function(){return ()=>{throw new Error("teardown control");};}', complete:true, kinds:["runtime","assertion"]},
  {name:"assertion-and-hook", source:failing+'\nimport {afterAll} from "vitest"; afterAll(()=>{throw new Error("hook control");});', complete:true, kinds:["hook","assertion"]},
  {name:"assertion-and-runtime", source:failing+'\ntest("runtime",()=>{throw new Error("runtime control");});', complete:true, kinds:["runtime","assertion"]},
  {name:"import-failure", source:'throw new Error("import control");\n'+passing, complete:true, kinds:["import"]},
  {name:"assertion-and-coverage", source:failing, extra:'coverage:{enabled:true,provider:"custom",customProviderModule:"./coverage.ts",reportOnFailure:true},', coverage:true, complete:false, kinds:["runtime","assertion"]},
  {name:"assertion-and-metadata", source:failing, auxiliary:auxiliary('this.context.state.metadata.control={dumpDir:"./blocked"};'), blocked:true, complete:false, kinds:["runtime","assertion"]},
  {name:"forced-exit", source:failing, auxiliary:auxiliary('process.exit(1);'), complete:false, kinds:["assertion"]},
  {name:"replaced-close", source:failing, auxiliary:auxiliary('this.context.close=async()=>{};'), complete:false, kinds:["assertion"]},
  {name:"missing-wait", source:failing, auxiliary:'export default class ControlReporter {onInit(context){context.waitForTestRunEnd=undefined;}}', complete:false, kinds:["assertion"]},
  {name:"merged-replay", source:passing, replay:true, complete:false, kinds:["runtime"]},
  {name:"corrected-teardown", source:passing, extra:'globalSetup:["./setup.ts"],', setup:'export default function(){return ()=>{};}', complete:true, kinds:[]},
];
const results = [];
try {
  for (const control of cases) {
    const directory = path.join(owned, control.name); fs.mkdirSync(directory);
    fs.symlinkSync(path.join(installed, "node_modules"), path.join(directory, "node_modules"), "dir");
    fs.writeFileSync(path.join(directory, "package.json"), '{"type":"module"}\n');
    fs.writeFileSync(path.join(directory, "case.test.ts"), control.source);
    if (control.setup) fs.writeFileSync(path.join(directory, "setup.ts"), control.setup);
    if (control.auxiliary) fs.writeFileSync(path.join(directory, "control.ts"), control.auxiliary);
    if (control.blocked) fs.writeFileSync(path.join(directory, "blocked"), "not a directory");
    if (control.coverage) fs.writeFileSync(path.join(directory, "coverage.ts"), `
      let options;
      export default {
        getProvider:()=>({name:"custom", initialize:context=>{options=context.config.coverage;},resolveOptions:()=>options,
          clean:()=>{},onAfterSuiteRun:()=>{},generateCoverage:()=>({}),reportCoverage:()=>{throw new Error("late coverage control");}}),
        startCoverage:()=>{},takeCoverage:()=>({}),stopCoverage:()=>{},
      };
    `);
    const request = {
      schemaVersion:1,requestId:control.name,runId:"real-vitest-controls",inputDigest:"a".repeat(64),
      phase:"baseline",mutationId:null,resultPath:path.join(directory,"result.json"),maxResultBytes:1_048_576,
    };
    const requestPath = path.join(directory,"request.json"); fs.writeFileSync(requestPath,JSON.stringify(request));
    const config = `import {defineConfig} from "vite-plus/test/config"; export default defineConfig({test:{
      include:["case.test.ts"],pool:"forks",maxWorkers:1,${control.extra ?? ""}
      reporters:[${JSON.stringify(reporter)}${control.auxiliary ? ',"./control.ts"' : ""}],
    }});`;
    fs.writeFileSync(path.join(directory,"vite.config.ts"),config);
    if (control.replay) {
      fs.mkdirSync(path.join(directory,"blobs"));
      const blob = spawnSync(runner,["test","run","--config","vite.config.ts","--reporter=blob","--outputFile=blobs/control.json"],{
        cwd:directory,env:process.env,timeout:30_000,maxBuffer:262_144,
      });
      assert.equal(blob.error,undefined,blob.error?.message);
      assert.equal(blob.status,0,blob.stderr.toString());
    }
    const child = spawnSync(runner,["test","run","--config","vite.config.ts",...(control.replay ? ["--merge-reports=./blobs"] : [])],{
      cwd:directory,env:{...process.env,ARCHGUARD_MUTATION_REQUEST:requestPath,ARCHGUARD_MUTATION_RESULT:request.resultPath},
      timeout:30_000,maxBuffer:262_144,
    });
    assert.equal(child.error,undefined,`${control.name}: ${child.error?.message}`);
    assert.ok(fs.existsSync(request.resultPath),`${control.name}: ${child.stderr.toString()}`);
    const result = JSON.parse(fs.readFileSync(request.resultPath,"utf8"));
    assert.equal(result.complete,control.complete,`${control.name}: ${JSON.stringify(result)} ${child.stderr.toString()}`);
    assert.equal(result.exitCode,child.status,control.name);
    for (const kind of control.kinds) assert.ok(result.failures.some(failure=>failure.kind === kind),`${control.name}: missing ${kind}: ${JSON.stringify(result)}`);
    if (control.kinds.length === 0) assert.deepEqual(result.failures,[],control.name);
    if (control.name === "assertion") assert.ok(result.failures.every(failure=>failure.kind === "assertion"));
    results.push({control:control.name,sourceSha256:createHash("sha256").update(control.source).digest("hex"),complete:result.complete,exitCode:result.exitCode,tests:result.tests,kinds:result.failures.map(failure=>failure.kind)});
    process.stderr.write(`Passed ${control.name}\n`);
  }
  process.stdout.write(JSON.stringify({schemaVersion:1,runner,controls:results},null,2)+"\n");
} finally { fs.rmSync(owned,{recursive:true,force:true}); }
