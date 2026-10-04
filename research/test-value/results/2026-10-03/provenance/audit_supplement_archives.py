import hashlib,json,pathlib,sys,tempfile,subprocess,os
sys.path.insert(0,"/Users/alun/.t3/worktrees/irudd-ts/t3code-6f4260d3/research/test-value")
from runner import hash_tree,disk_check,write_json,install_signal_handlers,check_cancelled
from rust_slice import archive
install_signal_handlers()
base=pathlib.Path('/tmp/archguard-test-value-20261003')
sourcepath=base/'supplement-candidates.json';source_raw=sourcepath.read_bytes();source=json.loads(source_raw)
planpath=base/'supplement-install-input-plan.json';planraw=planpath.read_bytes();plan=json.loads(planraw)
helperhash=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()
groups={g['id']:g for g in plan['groups']}
rows={c['id']:c for c in plan['candidates']}
assert len(rows)==28
records=[]; manifest_candidates=[]
for candidate in source['t3codeCandidates']+source['scopeExamples']:
 check_cancelled();disk_check(base)
 assert sourcepath.read_bytes()==source_raw and planpath.read_bytes()==planraw
 row=rows[candidate['id']];gid=row['installInputGroupId'];group=groups[gid]
 for key in ['id','fix','parent','repository','subject','sourceFiles','testFiles']:
  assert row[key]==candidate[key],(candidate['id'],key)
 tree=subprocess.check_output(['git','-C',candidate['repository'],'rev-parse',candidate['fix']+'^{tree}'],text=True).strip()
 assert tree==row['fixedTreeId'],candidate['id']
 first=subprocess.check_output(['git','-C',candidate['repository'],'rev-list','--parents','-n','1',candidate['fix']],text=True).split()
 assert len(first)>=2 and first[1]==candidate['parent'],candidate['id']
 inputs={i['path']:i['sha256'] for i in group['inputFiles']}
 signature=hashlib.sha256(json.dumps(sorted(inputs.items()),separators=(',',':')).encode()).hexdigest()
 assert signature==group['inputPathSha256Signature'],(candidate['id'],signature,group['inputPathSha256Signature'])
 with tempfile.TemporaryDirectory(prefix='archive-audit-supp-',dir=base) as temp:
  dest=pathlib.Path(temp)/'source';archive(candidate['repository'],candidate['fix'],dest)
  for rel,expected in inputs.items():
   f=dest/rel
   assert f.is_file() and not f.is_symlink() and f.resolve().is_relative_to(dest.resolve()),(candidate['id'],rel)
   assert hashlib.sha256(f.read_bytes()).hexdigest()==expected,(candidate['id'],rel)
  for kind in ['source','test']:
   for rel in candidate[kind+'Files']:
    f=dest/rel; assert f.is_file(),(candidate['id'],rel)
    expected=candidate['fileBlobs'][kind]['fix']['sha256']
    assert hashlib.sha256(f.read_bytes()).hexdigest()==expected,(candidate['id'],rel)
  sourcehash,file_records=hash_tree(dest)
 pm=group['packageManager']
 excluded=not pm['binaryAvailableAtAudit']
 reason=(f"Exact pinned {pm['name']}@{pm['exactVersion']} binary is unavailable; no version substitution is allowed." if excluded else None)
 update={k:candidate[k] for k in ['id','subject','repository','title','relatedGroup','fix','parent','sourceFiles','testFiles'] if k in candidate}
 update.update(installInputGroupId=gid,installationInputSignature=signature,sourceArchiveTreeSha256=sourcehash,sourceArchiveAlgorithm='runner-hash-tree-v1',fixRootTreeId=tree,firstParent=first[1],preflightEligible=not excluded,preflightExclusionReason=reason,exactPackageManager=pm['name']+'@'+pm['exactVersion'],packageManagerBinaryPath=pm.get('binaryPath'),sourceAndTestArchiveHashesVerified=True)
 manifest_candidates.append(update)
 records.append({'id':candidate['id'],'fix':candidate['fix'],'parent':first[1],'fixRootTreeId':tree,'installInputGroupId':gid,'installationInputSignature':signature,'sourceArchiveTreeSha256':sourcehash,'sourceArchiveAlgorithm':'runner-hash-tree-v1','archiveFileRecords':len(file_records),'auditedInstallInputFiles':len(inputs),'sourceAndTestHashesVerified':True,'preflightEligible':not excluded,'preflightExclusionReason':reason,'exactPackageManager':pm['name']+'@'+pm['exactVersion']})
 candidate.update(installationInputSignature=signature,sourceArchiveTreeSha256=sourcehash,sourceArchiveAlgorithm='runner-hash-tree-v1',fixRootTreeId=tree,preflightEligible=not excluded,preflightExclusionReason=reason,installInputGroupId=gid)
 print(candidate['id'],len(file_records),sourcehash,'eligible='+str(not excluded),flush=True)
 assert hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()==helperhash
assert sourcepath.read_bytes()==source_raw and planpath.read_bytes()==planraw
source_binding={
 'schemaVersion':1,'scope':'supplement28 source archive binding','sourceArchiveAlgorithm':'runner-hash-tree-v1',
 'method':'Independent fresh Git archive of each exact fixed revision, full runner hash before adaptation, every audited install input and selected source/test byte verified against the frozen Git objects.',
 'supplementCandidateFileSha256':hashlib.sha256(source_raw).hexdigest(),
 'supplementInstallPlanSha256':hashlib.sha256(planraw).hexdigest(),
 'auditHelperSha256':helperhash,'counts':{'candidates':len(records),'preflightEligible':sum(x['preflightEligible'] for x in records),'preflightExcluded':sum(not x['preflightEligible'] for x in records)},'candidates':records}
gatepath=base/'supplement-source-binding-audit.json';write_json(gatepath,source_binding)
manifest={
 'scope':'historical lock-matched supplement 28; source and install inputs independently archive-bound',
 'request_id':'history-lock-matched-supplement-28','candidateCount':len(manifest_candidates),
 'bySubject':{'t3code':sum(x['subject']=='t3code' for x in manifest_candidates),'scope':sum(x['subject']=='scope' for x in manifest_candidates)},
 'sourceArchiveAlgorithm':'runner-hash-tree-v1','sourceBindingAuditSha256':hashlib.sha256(gatepath.read_bytes()).hexdigest(),
 'sourceBindingAuditPath':str(gatepath),'auditHelperSha256':helperhash,'supplementInstallPlanSha256':hashlib.sha256(planraw).hexdigest(),
 'supplementCandidateFileSha256':hashlib.sha256(source_raw).hexdigest(),
 'preflightExclusions':[x['id'] for x in manifest_candidates if not x['preflightEligible']],
 'candidates':manifest_candidates}
manifestpath=base/'history-lock-matched-supplement-28.json';write_json(manifestpath,manifest)
print('WROTE',gatepath,flush=True);print('WROTE',manifestpath,flush=True)
