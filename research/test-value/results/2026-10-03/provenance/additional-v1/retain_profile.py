import argparse,pathlib,os,json,hashlib,tarfile,gzip,io,shutil
p=argparse.ArgumentParser();p.add_argument('source',type=pathlib.Path);p.add_argument('destination',type=pathlib.Path);a=p.parse_args()
a.destination.mkdir(parents=True,exist_ok=False)
excluded={'shared-store','template','source','private','home','tmp','cache','fixed-template','faulty-template','parent-tests-template','native-target','target','node_modules','active','__pycache__','.git'}
files=[]
for base,dirs,names in os.walk(a.source,followlinks=False):
 dirs[:]=[d for d in sorted(dirs) if d not in excluded and 'dependencies' not in d and not (pathlib.Path(base)/d).is_symlink()]
 for name in sorted(names):
  f=pathlib.Path(base)/name
  if f.is_symlink():raise ValueError('Evidence file must be regular')
  if f.name == "stdout.txt" and f.parent.name == "archive":continue
  if f.suffix not in {'.json','.txt','.log','.diff','.patch','.html'}:continue
  files.append(f)
manifest=[]
with (a.destination/'raw-evidence.tar.gz').open('wb') as compressed:
 with gzip.GzipFile(fileobj=compressed,mode='wb',mtime=0,filename='') as zipped:
  with tarfile.open(fileobj=zipped,mode='w|') as tar:
   for f in sorted(files):
    rel=f.relative_to(a.source).as_posix();raw=f.read_bytes()
    item=tarfile.TarInfo(rel);item.size=len(raw);item.mode=0o644;item.mtime=0
    tar.addfile(item,io.BytesIO(raw));manifest.append({'path':rel,'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw)})
    if f.name in {'matrix.json','analysis.json','evaluation.json','fault.json','attempts.json','aggregate.json','preregistration.json','candidates.json','plan-config.json','baseline-repeats.json','policy-and-events.json','workspace-adapters.json'}:
     out=a.destination/'data'/rel;out.parent.mkdir(parents=True,exist_ok=True);out.write_bytes(raw)
receipt={'schemaVersion':1,'sourceRoot':str(a.source.resolve()),'archive':'raw-evidence.tar.gz','archiveSha256':hashlib.sha256((a.destination/'raw-evidence.tar.gz').read_bytes()).hexdigest(),'files':manifest,'excludedDirectories':sorted(excluded),'exclusionRule':'Dependency stores, mutable source copies and private runtime/cache directories are not exported; their recorded digests remain in execution records. Raw Git tar stdout under archive/ is omitted with the source copies; archive execution receipts and payload hashes remain.'}
(a.destination/'manifest.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({'files':len(files),'rawBytes':sum(f['bytes'] for f in manifest),'archiveBytes':(a.destination/'raw-evidence.tar.gz').stat().st_size}))
