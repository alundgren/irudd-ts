import sys,json,pathlib,subprocess,hashlib,os,tarfile,tempfile
sys.path.insert(0,'/Users/alun/.t3/worktrees/irudd-ts/t3code-6f4260d3/research/test-value')
from runner import install_signal_handlers,run_command,write_json,disk_check,hash_tree
install_signal_handlers()
root=pathlib.Path('/tmp/archguard-test-value-20261003/historical-installs/scope-probe-c466-3')
root.mkdir(parents=True,exist_ok=False);source=root/'source';source.mkdir()
repo='/Users/alun/repos/irudd-scope';rev='c46613230e97bf5a98866f8c44a2dd69c5ffbed8'
tarpath=root/'source.tar'
with tarpath.open('wb') as f:subprocess.run(['git','-C',repo,'archive',rev],stdout=f,check=True)
with tarfile.open(tarpath) as tar:tar.extractall(source,filter='data')
assert not any((source/p).exists() for p in ['.pnpmfile.cjs','.pnpmfile.mjs','pnpmfile.cjs','.npmrc'])
package=json.loads((source/'package.json').read_text());assert package['license']=='MIT'
assert package['devEngines']['packageManager']['version']=='12.6.0'
assert 'configDependencies: {}' in (source/'pnpm-lock.yaml').read_text()
assert 'configDependencies' not in (source/'pnpm-workspace.yaml').read_text()
inputs={p.relative_to(source).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in source.rglob('*') if p.is_file() and not p.is_symlink()}
source_hash=hash_tree(source)[0]
node=pathlib.Path('/Users/alun/.local/share/vite-plus/js_runtime/node/26.10.0/bin/node');pm=pathlib.Path('/Users/alun/.local/share/vite-plus/package_manager/pnpm/12.6.0/pnpm/bin/pnpm.native')
private=root/'private';private.mkdir()
env={'PATH':str(node.parent)+':/usr/bin:/bin','CI':'true','NO_COLOR':'1','npm_config_userconfig':'/dev/null'}
for key,name in [('HOME','home'),('TMPDIR','tmp'),('XDG_CONFIG_HOME','config'),('XDG_CACHE_HOME','cache'),('XDG_DATA_HOME','data'),('PNPM_HOME','pnpm')]:
 p=private/name;p.mkdir();env[key]=str(p)
store=root.parent/'shared-store';cache=root.parent/'shared-cache';store.mkdir(exist_ok=True);cache.mkdir(exist_ok=True)
command=[str(pm),'install','--frozen-lockfile','--ignore-scripts','--ignore-pnpmfile','--no-runtime','--pm-on-fail=error','--reporter=append-only','--store-dir='+str(store)]
identity={'schemaVersion':1,'repository':repo,'revision':rev,'sourceTree':subprocess.check_output(['git','-C',repo,'rev-parse',rev+'^{tree}']).decode().strip(),'archiveSha256':hashlib.sha256(tarpath.read_bytes()).hexdigest(),'sourceSha256':source_hash,'inputFiles':inputs,'command':command,'pmVersion':'12.6.0','pmSha256':hashlib.sha256(pm.read_bytes()).hexdigest(),'nodeSha256':hashlib.sha256(node.read_bytes()).hexdigest(),'adaptation':'Lock-matched dependencies, ignored lifecycle and pnpm hooks, current pinned Node26.10; not full historical replay'}
write_json(root/'preparation.json',identity)
disk_check(root)
result=run_command(command,source,env,root/'install',timeout=1200,max_output=16*1024*1024)
changed=[p for p,h in inputs.items() if not (source/p).is_file() or hashlib.sha256((source/p).read_bytes()).hexdigest()!=h]
write_json(root/'receipt.json',{**identity,'process':result,'changedArchivedInputs':changed,'complete':result['status']=='finished' and result['exitCode']==0 and result['cleanupComplete'] and not changed})
print(json.dumps({'status':result['status'],'exitCode':result['exitCode'],'changed':changed,'cleanup':result.get('cleanupComplete')}),flush=True)
