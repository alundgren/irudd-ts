import { readMutationRequest, writeMutationResult } from "../../sdk/mutator.ts";
import * as fs from "node:fs";
import * as path from "node:path";
import { createHash } from "node:crypto";

const request = readMutationRequest();
const entries = new Map();
const outcomes = new Map();
const problems = [];
const failures = [];
let ended = false;
let summary;
const isAssertion = error => error?.name === "AssertionError" || error?.name === "AssertionError [ERR_ASSERTION]" || error?.code === "ERR_ASSERTION";
const keyOf = data => JSON.stringify([data.entryFile, data.testId]);
const detail = error => String(error?.message ?? "Node runner error").slice(0, 1000);

process.on("uncaughtExceptionMonitor", error => {
  problems.push("Uncaught exception in Node reporter");
  failures.push({kind:"unhandled", testId:null, file:null, message:detail(error)});
  publish(process.exitCode === undefined ? 1 : Number(process.exitCode));
});
process.once("exit", code => publish(code));

function publish(exitCode) {
  try {
    const tests = [];
    const occurrences = new Map();
    const root = process.env.ARCHGUARD_RESEARCH_ROOT;
    const counts = {passed:0, failed:0, skipped:0};
    for (const [rawId, entry] of entries) {
      if (entry.type !== "test") continue;
      const chain = [entry.name];
      let parentId = entry.parentId;
      for (let depth = 0; parentId && depth < 100; depth++) {
        const parent = entries.get(JSON.stringify([entry.entryFile, parentId]));
        if (!parent) { problems.push("Missing Node test parent"); break; }
        chain.unshift(parent.name); parentId = parent.parentId;
      }
      if (parentId) problems.push("Node test parent cycle or excessive depth");
      const file = path.relative(root, entry.file).split(path.sep).join("/");
      if (file.startsWith("../") || path.isAbsolute(file)) problems.push("Node test outside owned source");
      const key = JSON.stringify(["node",file,chain]);
      const occurrence = (occurrences.get(key) ?? 0)+1; occurrences.set(key,occurrence);
      const result = outcomes.get(rawId);
      const state = result?.skip || result?.todo ? "skipped" : result?.details?.passed === true ? "passed" : result?.details?.passed === false ? "failed" : "pending";
      if (state in counts) counts[state]++;
      const error = result?.details?.error;
      const cause = error?.cause;
      const errorData = cause && typeof cause === "object" ? cause : error;
      const errors = error ? [{name:errorData?.name ?? "",code:errorData?.code ?? ""}] : [];
      if (state === "failed") failures.push({kind:isAssertion(errorData) && error?.failureType === "testCodeFailure" ? "assertion" : "runtime",testId:rawId,file:entry.file,message:detail(errorData)});
      tests.push({id:JSON.stringify(["node",file,chain,occurrence]),rawId,project:"node",file,name:chain.join(" > "),hierarchy:chain,occurrence,state,
        readyCount:1,resultCount:result ? 1 : 0,retryCount:0,repeatCount:0,errors,
        ...(Number.isFinite(result?.details?.duration_ms) ? {durationMs:result.details.duration_ms} : {})});
    }
    if (!summary || summary.counts.tests !== tests.length || summary.counts.cancelled || summary.counts.todo) problems.push("Node summary disagrees with inventory or has cancelled/todo tests");
    const result = {schemaVersion:1,requestId:request.requestId,runId:request.runId,inputDigest:request.inputDigest,complete:ended && problems.length === 0,exitCode,
      reason:ended && problems.length === 0 ? "finished" : "infrastructureError",tests:counts,failures};
    writeMutationResult(request,result);
    const executable = fs.realpathSync(process.execPath);
    const runtime = {path:executable,version:process.version,sha256:createHash("sha256").update(fs.readFileSync(executable)).digest("hex")};
    const inventory = {schemaVersion:1,requestId:request.requestId,runId:request.runId,inputDigest:request.inputDigest,
      protocolSha256:createHash("sha256").update(fs.readFileSync(request.resultPath)).digest("hex"),runtime,tests,problems};
    const destination = process.env.ARCHGUARD_RESEARCH_INVENTORY;
    const bytes = JSON.stringify(inventory);
    if (Buffer.byteLength(bytes)>8*1024*1024) throw new Error("Node inventory budget exceeded");
    fs.writeFileSync(`${destination}.tmp`,bytes,{mode:0o600});fs.renameSync(`${destination}.tmp`,destination);
  } catch { process.exitCode = 3; }
}

export default async function* reporter(events) {
  for await (const {type,data} of events) {
    if (entries.size > 10000 || outcomes.size > 10000 || failures.length > 1000) throw new Error("Node event budget exceeded");
    if (type === "test:enqueue" && data.entryFile) {
      if (!Number.isSafeInteger(data.testId) || typeof data.file !== "string") { problems.push("Unsupported Node event identity API"); continue; }
      const key = keyOf(data);
      if (entries.has(key)) problems.push("Duplicate Node test collection");
      entries.set(key,data);
    } else if (type === "test:complete" && data.entryFile) {
      const key = keyOf(data);
      if (outcomes.has(key)) problems.push("Repeated Node test completion");
      outcomes.set(key,data);
      if (data.details?.type === "suite" && data.details.error && data.details.error.failureType !== "subtestsFailed") {
        failures.push({kind:"suite",testId:null,file:data.file,message:detail(data.details.error)});
      }
    } else if (type === "test:complete" && data.details?.error && data.details.error.failureType !== "subtestsFailed") {
      failures.push({kind:"import",testId:null,file:data.file ?? null,message:detail(data.details.error)});
    } else if (type === "test:summary" && !data.file) summary = data;
    else if (type === "test:diagnostic" && /uncaughtException|unhandledRejection|asynchronous activity/i.test(data.message ?? "")) {
      failures.push({kind:"unhandled",testId:null,file:data.file ?? null,message:String(data.message).slice(0,1000)});
    }
  }
  ended = true;
  yield "";
}
