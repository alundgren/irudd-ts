"""Verify and restore retained analytical and baseline-only evidence without dependency stores."""
import argparse,hashlib,json,pathlib,tarfile
p=argparse.ArgumentParser()
p.add_argument('retained',type=pathlib.Path)
p.add_argument('destination',type=pathlib.Path)
a=p.parse_args()
a.destination.mkdir(parents=True,exist_ok=False)
profiles={'current':'primary/current','native':'primary/native','original-history-first':'primary/history','original-history-continuation':'primary/history-continuation','posthoc-current':'secondary','lock-matched-baseline-validation':'baseline-validation'}
receipt=[]
for name,relative in profiles.items():
 source=a.retained/name
 manifest=json.loads((source/'manifest.json').read_text())
 archive=source/manifest['archive']
 if hashlib.sha256(archive.read_bytes()).hexdigest()!=manifest['archiveSha256']:
  raise ValueError('Archive digest mismatch: '+name)
 expected={f['path']:f for f in manifest['files']}
 if len(expected)!=len(manifest['files']):raise ValueError('Duplicate manifest members')
 seen=set();destination=a.destination/relative
 with tarfile.open(archive) as t:
  for member in t:
   path=pathlib.PurePosixPath(member.name)
   if not member.isfile() or path.is_absolute() or '..' in path.parts or member.name not in expected or member.name in seen:
    raise ValueError('Invalid evidence member: '+member.name)
   declaration=expected[member.name]
   if member.size!=declaration['bytes'] or member.size>64*1024*1024:
    raise ValueError('Invalid evidence member size: '+member.name)
   raw=t.extractfile(member).read()
   if hashlib.sha256(raw).hexdigest()!=declaration['sha256']:
    raise ValueError('Evidence member digest mismatch: '+member.name)
   out=destination/path
   out.parent.mkdir(parents=True,exist_ok=True)
   out.write_bytes(raw);seen.add(member.name)
 if seen!=set(expected):raise ValueError('Missing evidence members: '+name)
 receipt.append({'profile':name,'destination':relative,'files':len(seen),'archiveSha256':manifest['archiveSha256']})
(a.destination/'restoration.json').write_text(json.dumps({'schemaVersion':1,'profiles':receipt},indent=2)+'\n')
print(json.dumps({'destination':str(a.destination),'profiles':len(receipt)}))
