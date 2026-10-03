import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

const root = "/tmp/archguard-quality-product-reporter-survivor-review";
const sourceRoot = "/tmp/archguard-quality-product-cli-reviewed-worktree";
const recordsDirectory = "/tmp/archguard-quality-product-self-profiles-final18/state/reporter/records";
const digest = bytes => crypto.createHash("sha256").update(bytes).digest("hex");
const reporterSource = fs.readFileSync(path.join(sourceRoot, "sdk/mutator-vitest-reporter.ts"));
const protocolSource = fs.readFileSync(path.join(sourceRoot, "sdk/mutator.ts"));
const records = fs.readdirSync(recordsDirectory).filter(file => file.endsWith(".json")).map(file => {
  const outer = JSON.parse(fs.readFileSync(path.join(recordsDirectory, file), "utf8"));
  if (digest(outer.record) !== outer.checksum) throw new Error(`Invalid record checksum ${file}`);
  const record = JSON.parse(outer.record);
  return { file, outer, record };
});
fs.mkdirSync(root, { recursive: true });

export function runScenario(scenario, mutation = null) {
  const directory = fs.mkdtempSync(path.join(root, "owned-child-"));
  const resultPath = path.join(directory, "result.json");
  const requestPath = path.join(directory, "request.json");
  let source = reporterSource;
  if (mutation) {
    const { start, end } = mutation.location;
    if (reporterSource.subarray(start, end).toString() !== mutation.expected) throw new Error(`Edit mismatch ${mutation.mutationId}`);
    source = Buffer.concat([reporterSource.subarray(0, start), Buffer.from(mutation.replacement), reporterSource.subarray(end)]);
  }
  fs.writeFileSync(path.join(directory, "reporter.ts"), source);
  fs.writeFileSync(path.join(directory, "mutator.ts"), protocolSource);
  fs.writeFileSync(requestPath, JSON.stringify({ schemaVersion: 1, requestId: scenario, runId: "independent-reporter-review", inputDigest: "b".repeat(64), phase: "baseline", mutationId: null, resultPath, maxResultBytes: 1_048_576 }));
  const program = `
    import Reporter from ${JSON.stringify(pathToFileURL(path.join(directory, "reporter.ts")).href)};
    const scenario = ${JSON.stringify(scenario)};
    const reporter = new Reporter();
    let secondRejected = false, initThrew = false;
    if(scenario === "secondReporter") { try { new Reporter(); } catch { secondRejected = true; } }
    if(scenario === "lateExitDefault") process.on("exit", () => {throw new Error("late failure without assigned exit code");});
    let resolveRun, rejectRun;
    const pending = new Promise((resolve,reject) => {resolveRun = resolve;rejectRun = reject;});
    const context = {
      config: {watch:false},
      close: async () => { if(scenario === "pendingClose") await new Promise(() => {}); if(scenario === "rejectClose") throw new Error("close rejection control"); if(scenario === "infraOverflow") for(let n=0;n<1_025;n++) context.logger.error("cleanup overflow control"); },
      waitForTestRunEnd: () => scenario === "nonPromiseWait" ? undefined : scenario === "throwWait" ? (()=>{throw new Error("wait failure control")})() : pending,
      state: {getUnhandledErrors: () => scenario === "unhandledOverflow" ? Array(10_001).fill({message:"unhandled limit"}) : []},
      logger: {error: () => {}},
    };
    if(scenario === "missingClose") delete context.close;
    if(scenario === "missingWait") delete context.waitForTestRunEnd;
    if(scenario === "missingUnhandled") delete context.state.getUnhandledErrors;
    if(scenario === "missingLogger") delete context.logger.error;
    try { reporter.onInit(scenario === "nullContext" ? null : context); } catch { initThrew = true; }
    if(scenario === "normalLog") context.logger.error("ordinary diagnostic outside close");
    const states = {skip:"skipped", unknownState:"unknown", noError:"failed", nonArrayErrors:"failed"};
    const assertionScenarios = new Set(["shortMessage", "ascii4000", "multibyteMessage", "loneD800", "loneDBFF", "assertionAlias", "assertionCode", "runtimeCode", "failureOverflow"]);
    const diagnostic = scenario === "ascii4000" ? "x".repeat(4_000) : scenario === "multibyteMessage" ? "prefix" + "é".repeat(3_000) : scenario === "loneD800" ? "prefix\\uD800" : scenario === "loneDBFF" ? "prefix\\uDBFF" : "boundary control";
    const error = {name:scenario === "assertionAlias" ? "AssertionError [ERR_ASSERTION]" : scenario === "assertionCode" || scenario === "runtimeCode" ? "TypeError" : "AssertionError", code:scenario === "assertionCode" ? "ERR_ASSERTION" : scenario === "runtimeCode" ? "ERR_OTHER" : undefined, message:diagnostic};
    const test = {id:"case-1", type:"test", module:{moduleId:"case.test.ts"}, result:()=>({state:states[scenario] ?? (assertionScenarios.has(scenario) ? "failed" : "passed"), errors:scenario === "noError" ? [] : scenario === "nonArrayErrors" ? {length:1} : scenario === "failureOverflow" ? Array(1_025).fill(error) : assertionScenarios.has(scenario) ? [error] : []})};
    const suite = {type:"suite", moduleId:"case.test.ts", errors:()=>[]};
    const module = {type:"module", moduleId:"case.test.ts", errors:()=>scenario === "moduleErrorsOverflow" ? Array(10_001).fill({message:"module overflow"}) : [], children:{allTests:()=>scenario === "duplicateTest" ? [test,test] : [test], allSuites:()=>scenario === "badSuite" ? [{}] : []}};
    if(scenario === "badModule") delete module.children.allTests;
    if(scenario === "badTest") delete test.result;
    if(scenario === "suiteBudget") module.children.allSuites=()=>Array(30_000).fill(suite);
    if(scenario === "aggregateModuleBudget") module.children.allSuites=()=>Array(29_998).fill(suite);
    if(scenario === "testBudget") module.children.allTests=()=>Array.from({length:10_001},(_,n)=>({...test,id:"case-"+n}));
    if(scenario === "hookRunErrors" || scenario === "duplicateHook" || scenario === "hookStartOnly") {
      const hook={name:"beforeAll",entity:{task:{result:{hooks:{beforeAll:"run"},errors:[{message:"hook failure"}]}},moduleId:"case.test.ts"}};
      reporter.onHookStart(hook);
      if(scenario === "duplicateHook") reporter.onHookEnd(hook);
    }
    if(scenario === "hookOverflow") for(let n=0;n<10_001;n++) reporter.onHookEnd({name:"beforeAll",entity:{task:{result:{hooks:{beforeAll:"pass"}}},moduleId:"case.test.ts"}});
    if(scenario === "hookUnknown") reporter.onHookEnd({name:"beforeAll",entity:{task:{result:{hooks:{beforeAll:"unknown"}}},moduleId:"case.test.ts"}});
    if(scenario !== "noRun") reporter.onTestRunEnd(scenario === "moduleBudget" ? Array(10_001).fill(module) : scenario === "aggregateModuleBudget" ? [module,{...module,children:{allSuites:()=>[],allTests:()=>[]}}] : [module],scenario === "earlyErrors" ? [{message:"early run error"}] : scenario === "earlyErrorsOverflow" ? Array(10_001).fill({message:"early errors overflow"}) : [],scenario === "unknownReason" ? "unknown" : "passed");
    if(scenario === "repeatRun") reporter.onTestRunEnd([module],[],"passed");
    if(scenario === "timeout") reporter.onProcessTimeout();
    if(scenario === "replaceLogger") context.logger.error=()=>{};
    if(scenario === "replaceWait") context.waitForTestRunEnd=()=>pending;
    resolveRun();await Promise.resolve();
    if(scenario === "pendingClose") { context.close();await Promise.resolve(); }
    else if(scenario !== "noClose" && typeof context.close === "function") {try {await context.close();}catch{}}
    console.log(JSON.stringify({secondRejected,initThrew}));
    if(scenario !== "lateExitDefault") process.exitCode = assertionScenarios.has(scenario) || scenario === "earlyErrors" || scenario === "timeout" || scenario.startsWith("hook") || scenario === "duplicateHook" ? 1 : 0;
  `;
  let child;
  try {
    child = spawnSync(process.execPath, ["--input-type=module", "--eval", program], {env:{...process.env,ARCHGUARD_MUTATION_REQUEST:requestPath,ARCHGUARD_MUTATION_RESULT:resultPath},timeout:5_000,maxBuffer:65_536});
    if(child.error) throw child.error;
    const result = fs.existsSync(resultPath) ? JSON.parse(fs.readFileSync(resultPath,"utf8")) : null;
    return {scenario,mutationId:mutation?.mutationId??null,reporterBeforeSha256:digest(reporterSource),reporterChildSha256:digest(source),protocolSha256:digest(protocolSource),status:child.status,signal:child.signal,stdout:child.stdout.toString(),stderr:child.stderr.toString(),result};
  } finally {
    if(child && (child.status !== null || child.signal !== null)) fs.rmSync(directory,{recursive:true,force:true});
  }
}

if(process.argv[1] === new URL(import.meta.url).pathname) {
  const id = process.argv[2];
  const selected = id && id !== "baseline" ? records.find(item=>item.record.mutationId===id)?.record.result : null;
  if(id && id !== "baseline" && !selected) throw new Error(`No completed record ${id}`);
  const scenarios = process.argv.slice(3);
  const output = scenarios.map(scenario => runScenario(scenario, selected));
  const destination = path.join(root, `probe-${id??"baseline"}-${Date.now()}.json`);
  fs.writeFileSync(destination, JSON.stringify({createdAt:new Date().toISOString(),node:process.version,scope:"Bounded authored lifecycle probes, not actual Vitest or universal equivalence proof",sourceRoot,sourceReporterSha256:digest(fs.readFileSync(path.join(sourceRoot,"sdk/mutator-vitest-reporter.ts"))),sourceProtocolSha256:digest(fs.readFileSync(path.join(sourceRoot,"sdk/mutator.ts"))),output},null,2));
  console.log(JSON.stringify({destination,results:output.map(({scenario,status,result,stdout})=>({scenario,status,complete:result?.complete,tests:result?.tests,kinds:result?.failures.slice(0,8).map(f=>f.kind),failureCount:result?.failures.length,messages:result?.failures.slice(0,2).map(f=>f.message.slice(0,100)),stdout}))},null,2));
}
