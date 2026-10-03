import fs from 'node:fs';
import crypto from 'node:crypto';
const root='/tmp/archguard-quality-product-reporter-survivor-review';
let text=fs.readFileSync(`${root}/mutator_reporter.final-v2.test.ts`,'utf8');
const edits=[
['import type { TestExecutionResult } from "../sdk/mutator.ts";','import type { TestExecutionResult } from "../sdk/mutator.ts";\nimport { mutationProtocolLimits } from "../sdk/mutator.ts";'],
['    const module = {','    if(scenario === "failureOverflow" || scenario === "failureBudget") test.result=()=>({\n      state:"failed",\n      errors:Array(${mutationProtocolLimits.maxFailures} + (scenario === "failureOverflow" ? 1 : 0)).fill({name:"AssertionError",message:"failure budget control"}),\n    });\n    const module = {'],
['    if(scenario === "repeatRun") reporter.onTestRunEnd([module],[],"passed");','    if(scenario === "processTimeout") reporter.onProcessTimeout();\n    if(scenario === "repeatRun") reporter.onTestRunEnd([module],[],"passed");'],
];
for(const[from,to]of edits){if(text.split(from).length!==2)throw new Error('Nonunique edit '+from);text=text.replace(from,to);}
text+=`

test("runner cleanup timeout remains incomplete alongside assertion failures", () => {
  const timedOut = run("processTimeout");
  assert.equal(timedOut.complete, false);
  assert.ok(timedOut.failures.some(error => error.kind === "runtime"));
  assert.ok(timedOut.failures.some(error => error.kind === "assertion"));
  const corrected = run("assertion");
  assert.equal(corrected.complete, true);
  assert.equal(corrected.failures.every(error => error.kind === "assertion"), true);
  assert.deepEqual(run("pass").failures, []);
});

test("failure record overflow stays incomplete while the supported failure budget remains complete", () => {
  const oversized = run("failureOverflow");
  assert.equal(oversized.complete, false);
  assert.ok(oversized.failures.length <= mutationProtocolLimits.maxFailures);
  const corrected = run("failureBudget");
  assert.equal(corrected.complete, true);
  assert.equal(corrected.failures.length, mutationProtocolLimits.maxFailures);
  assert.equal(corrected.failures.every(error => error.kind === "assertion" && error.testId === "case-1"), true);
  assert.deepEqual(run("pass").failures, []);
});
`;
const file=`${root}/mutator_reporter.final-v3.test.ts`;
fs.writeFileSync(file,text,{flag:'wx'});
const sha256=crypto.createHash('sha256').update(text).digest('hex');
fs.writeFileSync(`${root}/final-v3-test-proposal.json`,JSON.stringify({createdAt:new Date().toISOString(),file,sha256,previousFile:`${root}/mutator_reporter.final-v2.test.ts`,additionalControls:['Cleanup timeout cannot complete an otherwise settled failed run.','Dropping assertion errors past the shared failure record budget cannot produce complete assertion-only evidence.']},null,2),{flag:'wx'});
console.log({file,sha256});
