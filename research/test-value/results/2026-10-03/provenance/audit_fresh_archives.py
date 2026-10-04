import hashlib,json,pathlib,sys,tempfile,subprocess
sys.path.insert(0,"/Users/alun/.t3/worktrees/irudd-ts/t3code-6f4260d3/research/test-value")
from runner import hash_tree,disk_check,write_json,install_signal_handlers,check_cancelled
from rust_slice import archive
install_signal_handlers()
base=pathlib.Path("/tmp/archguard-test-value-20261003")
planpath=base/"historical-install-plan.json"; planraw=planpath.read_bytes();plan=json.loads(planraw)
manifestpath=base/"history-registered-50.json";raw=manifestpath.read_bytes();manifest=json.loads(raw)
helperhash=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()
groups={g["id"]:g for g in plan["groups"]}; rows={c["id"]:c for c in plan["candidates"]}
assert len(rows)==len(manifest["candidates"])==50
records=[]
for candidate in manifest["candidates"]:
 check_cancelled(); disk_check(base)
 assert planpath.read_bytes()==planraw and manifestpath.read_bytes()==raw
 row=rows[candidate["id"]];group=groups[row["groupId"]]
 for key in ["fix","parent","repository","subject","sourceFiles","testFiles"]:assert candidate[key]==row[key],(candidate["id"],key)
 tree=subprocess.check_output(["git","-C",candidate["repository"],"rev-parse",candidate["fix"]+"^{tree}"],text=True).strip()
 assert tree==row["fixRootTreeId"]
 parent=subprocess.check_output(["git","-C",candidate["repository"],"rev-parse",candidate["fix"]+"^"],text=True).strip();assert parent==candidate["parent"]
 inputs={i["path"]:i["sha256"] for i in group["inputFiles"]}
 signature=hashlib.sha256(json.dumps(sorted(inputs.items()),separators=(",",":")).encode()).hexdigest()
 with tempfile.TemporaryDirectory(prefix="archive-audit-",dir=base) as temp:
  source=pathlib.Path(temp)/"source";archive(candidate["repository"],candidate["fix"],source)
  for rel,expected in inputs.items():
   path=source/rel
   assert path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(source.resolve())
   assert hashlib.sha256(path.read_bytes()).hexdigest()==expected,(candidate["id"],rel)
  sourcehash,files=hash_tree(source)
 candidate.update(installationInputSignature=signature,sourceArchiveTreeSha256=sourcehash,sourceArchiveAlgorithm="runner-hash-tree-v1")
 records.append({"id":candidate["id"],"fix":candidate["fix"],"parent":parent,"fixRootTreeId":tree,"groupId":row["groupId"],"installationInputSignature":signature,"sourceArchiveTreeSha256":sourcehash,"archiveFileRecords":len(files),"auditedInputFiles":len(inputs)})
 print(candidate["id"],len(files),sourcehash,flush=True)
 assert hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()==helperhash
write_json(base/"historical-source-launch-gate.json",{"schemaVersion":1,"sourceArchiveAlgorithm":"runner-hash-tree-v1","method":"Independent fresh Git archive of every exact fixed revision, full runner hash before adaptation, all frozen audited installer inputs checked","planSha256":hashlib.sha256(planraw).hexdigest(),"originalCandidatesSha256":hashlib.sha256(raw).hexdigest(),"auditHelperSha256":helperhash,"candidates":records})
manifest.update(request_id="lock-matched-posthoc-50",sourceBindingAuditSha256=hashlib.sha256((base/"historical-source-launch-gate.json").read_bytes()).hexdigest())
write_json(base/"history-lock-matched-50.json",manifest)
