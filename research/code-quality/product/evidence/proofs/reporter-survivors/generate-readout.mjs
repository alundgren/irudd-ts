import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
const root='/tmp/archguard-quality-product-reporter-survivor-review';
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const classification=JSON.parse(fs.readFileSync(`${root}/survivor-classification.final.json`,'utf8'));
const proofs=fs.readdirSync(root).filter(file=>file.startsWith('test-proof-')&&file.endsWith('.json')).map(file=>({file,...JSON.parse(fs.readFileSync(`${root}/${file}`,'utf8'))}));
const summary=proofs.map(({file,mutationId,variant,status,signal,testInputSha256,namePattern,stdout,reporterBeforeSha256,reporterAfterSha256,protocolSha256,protocolAfterSha256})=>({file,mutationId,variant,status,signal,testInputSha256,namePattern,summary:stdout.split('\n').filter(line=>/ℹ (tests|pass|fail|duration_ms)/.test(line)),productionReporterUnchanged:reporterBeforeSha256===reporterAfterSha256,productionProtocolUnchanged:protocolSha256===protocolAfterSha256}));
if(summary.some(item=>!item.productionReporterUnchanged||!item.productionProtocolUnchanged))throw new Error('Production input changed during proof');
fs.writeFileSync(`${root}/test-proof-status.json`,JSON.stringify({createdAt:new Date().toISOString(),proofs:summary},null,2),{flag:'wx'});
const witnessRows=[
['Short diagnostics','0d5c0d6e796efe2b7968ef30e2fab06faf5aada7a386e1714857fd768587543f'],
['Node assertion aliases','77b9124032d070d5bb793df328917b20a17a3e489efdfeb256ad97254235a662'],
['Pending close','d61cb99af205a2c2080a671dbba4112b07b7ea5eba62f65f8c68e166d9d15854'],
['Ordinary logging after close','b85934e0d092931472b11e8985fc3a9429cfe552d3710776195a2e962994ee03'],
['One reporter per process','606b4ce42a192aadf48fc578fc5240f129f311ad5a77fe552768809cc6e311e3'],
['Started hook without completion metadata','81d2333da0d2b56a8dd74d2c322ca2c529310706b9aad211042496868b4305d8'],
['Cleanup timeout','c8d530de556ccfe58dac63cf667bcb2bb2037e4c543add1e89bf1daf8c77ceef'],
['Failure-record overflow','7f6e83750dd598db64ac171b9d0dfd0bc919a805b232452038a577f598f04ff8'],
].map(([behavior,id])=>{
const old=proofs.find(p=>p.mutationId===id&&p.variant==='original'&&p.status===0);
const newer=proofs.find(p=>p.mutationId===id&&p.variant==='final3'&&p.status===1);
if(!old||!newer)throw new Error('Missing proof pair '+id);
return {behavior,mutationId:id,oldOriginalFive:{file:old.file,status:old.status},newFinalV3Control:{file:newer.file,status:newer.status}};
});
const repeatedId='c06a6f4b79b9927050c983be8ce64421af72501165bc032ba5879339c303d417';
const repeatedOld=proofs.find(p=>p.mutationId===repeatedId&&p.variant==='original'&&p.status===0);
const repeatedNew=proofs.find(p=>p.mutationId===repeatedId&&p.variant==='final3'&&p.status===1);
fs.writeFileSync(`${root}/public-control-witnesses.json`,JSON.stringify({createdAt:new Date().toISOString(),baselineFinalV3:{file:'test-proof-baseline-final3-1791051625540.json',status:0,tests:12,sha256:'ae6db639b369ed0f3bd12cd236c01692dc731d5d83a727aaa469d77845c6ea1c'},witnesses:witnessRows,separateTimedOutSiteWitness:repeatedOld&&repeatedNew?{behavior:'Repeated onTestRunEnd',mutationId:repeatedId,recordedMutationOutcome:'timedOut',oldOriginalFive:repeatedOld.file,newFinalV3Control:repeatedNew.file,interpretation:'The bounded Node control exposes the repeated-run completeness defect. It does not replace the original installed-framework timeout or count it as a kill.'}:null},null,2),{flag:'wx'});
const labels={useful_missing_public_control:'Useful public control',implementation_detail:'Implementation detail',equivalent:'Equivalent for one-shot usage',unresolved:'Unresolved'};
const safe=value=>String(value).replaceAll('|','\\|').replaceAll('\n',' ');
const text=[
'All 85 surviving reporter mutations have an individual source classification. The original 154-site installed Vitest run remains incomplete, with 66 kills, 85 survivors and 3 command timeouts. Every planned site was attempted and the report omits no results.',
'',
'The authoritative report SHA256 is `655df5cc91158e7e6ce6add391e1a6a6f1412cae24d62941045d2f06e1ab25d5`. Its exact bytes remain in [original-full-report.json](original-full-report.json). The full per-ID decisions and proposed follow-up controls are in [survivor-classification.final.json](survivor-classification.final.json).',
'',
'| Classification | Count |',
'| --- | ---: |',
'| Useful missing public controls | 43 |',
'| Implementation details | 40 |',
'| Equivalent under the documented one-shot lifecycle | 2 |',
'| Unresolved survivors | 0 |',
'',
'The final proposed test file is [mutator_reporter.final-v3.test.ts](mutator_reporter.final-v3.test.ts), SHA256 `ae6db639b369ed0f3bd12cd236c01692dc731d5d83a727aaa469d77845c6ea1c`. It keeps the original five tests and adds seven groups for diagnostics, assertion aliases, runner close and ordinary logging, one-shot usage, hook metadata, timeout and failure-record overflow. Unchanged production SDK passes all 12. The original five-test file and every earlier test proposal remain separately retained.',
'',
'Each public group has a bounded Node witness. The exact altered reporter passes the original five tests and fails its targeted final V3 control. The corrected unchanged reporter passes the final 12-test baseline. The log control also distinguishes logging outside close from actual cleanup failure. Assertion controls retain TypeError as runtime evidence and passing runs as clean evidence.',
'',
'| Behavior | Mutation ID | Original five | Targeted final V3 |',
'| --- | --- | --- | --- |',
...witnessRows.map(w=>`| ${w.behavior} | \`${w.mutationId}\` | [PASS](${w.oldOriginalFive.file}) | [FAIL](${w.newFinalV3Control.file}) |`),
'',
'The repeated-run control has a separate witness at the original timed-out site `c06a6f4b79b9927050c983be8ce64421af72501165bc032ba5879339c303d417`. Its original five Node controls pass and the new repeated-run control fails. The installed Vitest outcome remains timedOut. [public-control-witnesses.json](public-control-witnesses.json) records this distinction.',
'',
'The failure overflow witness is especially useful. The changed reporter drops the 1,025th assertion failure while publishing complete assertion-only evidence. The unchanged reporter marks that result incomplete. The corrected run at the shared 1,024-record budget still completes and retains every failure. Timeout controls similarly require incomplete runtime evidence even when the run also has a legitimate assertion failure.',
'',
'Both equivalence decisions use source reasoning for the documented one-shot callback order. Successful close sets closeStarted before closeFinished, so changing the initial closeStarted value cannot permit completion on its own. Returning from the invalid-promise branch leaves runSettled false, so changing supported there cannot permit completion. Equal sampled output is not the proof.',
'',
'The three timed-out edits introduce no new loop or wait on the original-five paths. Separate exact Node copies completed those five controls in 13.0s, 8.5s and 8.7s. This supports bounded behavior for those authored inputs. The cause of the installed Vitest deadlines remains unresolved. [timeout-source-review.json](timeout-source-review.json) keeps the source reasoning, receipts and unchanged timedOut outcomes.',
'',
'Two review corrections are retained in [review-corrections.json](review-corrections.json). The reserve assignment was initially called redundant, but the module caller only breaks and relies on reserve to mark incompleteness. A mixed-module counterexample changes incomplete evidence into complete:true, so the classification was corrected. A repeated-run probe for a different logical operator passed; its unfavorable receipt remains retained, and the source review identifies the actual oversized-module case instead.',
'',
'No production SDK changed. [review-provenance.json](review-provenance.json) records the source hashes before and after, role snapshot, scope and cleanup. All probes use owned isolated copies, and their child processes finish before those copies are removed. The fixed full mutation replay and its unfavorable results are preserved. Additional suggested controls remain proposals, and this review does not set a mutation score target.',
'',
'| Mutation ID | Line | Operator | Edit | Classification | Reason |',
'| --- | ---: | --- | --- | --- | --- |',
...classification.survivors.map(r=>`| \`${r.mutationId}\` | ${r.location.line} | ${r.operator} | \`${safe(r.expected)}\` to \`${safe(r.replacement)}\` | ${labels[r.classification]} | ${safe(r.reason)} |`),
'',
].join('\n');
fs.writeFileSync(`${root}/review.md`,text,{flag:'wx'});
const manifest=fs.readdirSync(root).filter(file=>fs.statSync(path.join(root,file)).isFile()).sort().map(file=>({file,sha256:hash(fs.readFileSync(path.join(root,file))),bytes:fs.statSync(path.join(root,file)).size}));
fs.writeFileSync(`${root}/artifact-manifest.json`,JSON.stringify({createdAt:new Date().toISOString(),files:manifest},null,2),{flag:'wx'});
console.log({review:`${root}/review.md`,proofs:summary.length,witnessPairs:witnessRows.length,files:manifest.length,reviewSha256:hash(text),classificationSHA256:hash(fs.readFileSync(`${root}/survivor-classification.final.json`))});
