import json,pathlib,subprocess,hashlib,os,tarfile,sys
sys.path.insert(0,'/Users/alun/.t3/worktrees/irudd-ts/t3code-6f4260d3/research/test-value')
from runner import install_signal_handlers,run_command,write_json,disk_check,hash_tree,check_cancelled,runner_fingerprint
install_signal_handlers()
base=pathlib.Path('/tmp/archguard-test-value-20261003/historical-installs')
planpath=pathlib.Path('/tmp/archguard-test-value-20261003/historical-install-plan.json');planraw=planpath.read_bytes();plan=json.loads(planraw)
planhash=hashlib.sha256(planraw).hexdigest()
nodebase=pathlib.Path('/Users/alun/.local/share/vite-plus/js_runtime/node')
pms={'12.6.0':['/Users/alun/.local/share/vite-plus/package_manager/pnpm/12.6.0/pnpm/bin/pnpm.native'],'11.10.0':['/Users/alun/.local/share/vite-plus/package_manager/pnpm/11.10.0/pnpm/bin/pnpm.cjs'],'10.24.0':['/tmp/archguard-test-value-20261003/tooling/pnpm-10.24.0/package/bin/pnpm.cjs']}
helperhash=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()
runnerhash=runner_fingerprint()
pmhashes={v:hash_tree(pathlib.Path(c[0]).parent.parent)[0] for v,c in pms.items()}
selected=set(sys.argv[1:])
mapvalue={'schemaVersion':1,'installations':[],'candidates':{c['id']:c['groupId'] for c in plan['candidates']}}
for group in plan['groups']:
 check_cancelled();disk_check(base)
 if selected and group['id'] not in selected:continue
 if planpath.read_bytes()!=planraw or hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()!=helperhash or runner_fingerprint()!=runnerhash:raise ValueError('Installation plan or preparation helper changed during preparation')
 if group['activeConfigHooks'] or group['configDependencies']:raise ValueError('Executable installer hooks/config dependencies require explicit separate audit')
 version=group['packageManager']['exactVersion'];assert version in pms
 if hash_tree(pathlib.Path(pms[version][0]).parent.parent)[0]!=pmhashes[version]:raise ValueError('Pinned package-manager bytes changed')
 node=nodebase/('26.10.0' if group['subject']=='scope' else '24.21.0')/'bin/node'
 pm=pms[version];pmcommand=pm if version=='12.6.0' else [str(node),*pm]
 root=base/group['id'];root.mkdir(exist_ok=False);source=root/'source';source.mkdir()
 tarpath=root/'source.tar'
 with tarpath.open('wb') as f:subprocess.run(['git','-C',group['repository'],'archive',group['revision']],stdout=f,check=True)
 with tarfile.open(tarpath) as tar:tar.extractall(source,filter='data')
 inputfiles={item['path']:item['sha256'] for item in group['inputFiles']}
 def changed_inputs():
  changed=[]
  for rel,h in inputfiles.items():
   p=source/rel
   if not p.resolve().is_relative_to(source.resolve()) or p.is_symlink() or not p.is_file() or hashlib.sha256(p.read_bytes()).hexdigest()!=h:changed.append(rel)
  return changed
 assert not changed_inputs()
 signature=hashlib.sha256(json.dumps(sorted(inputfiles.items()),separators=(',',':')).encode()).hexdigest()
 private=root/'private';private.mkdir();env={'PATH':str(node.parent)+':/usr/bin:/bin','CI':'true','NO_COLOR':'1','npm_config_userconfig':'/dev/null'}
 for key,name in [('HOME','home'),('TMPDIR','tmp'),('XDG_CONFIG_HOME','config'),('XDG_CACHE_HOME','cache'),('XDG_DATA_HOME','data'),('PNPM_HOME','pnpm')]:
  p=private/name;p.mkdir();env[key]=str(p)
 probe=run_command(pmcommand+['--version'],private,env,root/'pm-version',timeout=30)
 actual=(root/'pm-version/stdout.txt').read_text().strip()
 assert probe['cleanupComplete'] and probe['exitCode']==0 and actual==version
 command=pmcommand+['install','--frozen-lockfile','--ignore-scripts','--ignore-pnpmfile','--reporter=append-only','--store-dir='+str(base/'shared-store')]
 if version=='10.24.0':command+=['--manage-package-manager-versions=false','--package-manager-strict-version=true']
 else:command+=['--no-runtime','--pm-on-fail=error']
 safeguards={'frozenLockfile':True,'lifecycleScripts':False,'pnpmHooks':False,'automaticPackageManagerSwitch':False,'automaticRuntimeInstall':False}
 identity={'schemaVersion':1,'id':group['id'],'subject':group['subject'],'installationRoot':str(source),'inputSignature':signature,'policy':'lock-matched-adapted-v1','safeguards':safeguards,'repository':group['repository'],'revision':group['revision'],'sourceTree':group['rootTreeId'],'archiveSha256':hashlib.sha256(tarpath.read_bytes()).hexdigest(),'sourceSha256':hash_tree(source)[0],'inputFiles':inputfiles,'installationPlanSha256':planhash,'command':command,'preparationHelperSha256':helperhash,'preparationRunnerSha256':runnerhash,'pmPackageTreeSha256':pmhashes[version],'pmVersion':version,'pmSha256':hashlib.sha256(pathlib.Path(pm[0]).read_bytes()).hexdigest(),'nodeVersion':'26.10.0' if group['subject']=='scope' else '24.21.0','nodeSha256':hashlib.sha256(node.read_bytes()).hexdigest(),'adaptation':'Lock-matched adapted dependencies; ignored lifecycle and pnpm hooks, fixed current Node, source reversion; no full historical fidelity claim'}
 write_json(root/'preparation.json',identity)
 result=run_command(command,source,env,root/'install',timeout=1200,max_output=16*1024*1024)
 changed=changed_inputs();complete=result['status']=='finished' and result['exitCode']==0 and result['cleanupComplete'] and not changed
 receipt={**identity,'process':result,'inputFilesUnchanged':not changed,'changedArchivedInputs':changed,'complete':complete}
 receiptpath=root/'receipt.json';write_json(receiptpath,receipt)
 mapvalue['installations'].append({'id':group['id'],'subject':group['subject'],'root':str(source),'receiptPath':str(receiptpath),'receiptSha256':hashlib.sha256(receiptpath.read_bytes()).hexdigest()})
 write_json(base/'dependency-installations.json',mapvalue)
 print(group['id'],'complete' if complete else 'setup-failed',result['exitCode'],round(result['durationMs']/1000,2),'seconds',flush=True)
 if not result['cleanupComplete']:raise RuntimeError('Installer process cleanup uncertain; stop')
write_json(base/('dependency-installations-selected.json' if selected else 'dependency-installations.json'),mapvalue)
