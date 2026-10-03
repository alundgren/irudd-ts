#!/usr/bin/env python3
"""Measure equivalent reduced sources with and without the pinned semantic provider."""
import argparse,hashlib,json,os,pathlib,platform,shutil,statistics,subprocess,tempfile,time
parser=argparse.ArgumentParser();parser.add_argument('--binary',type=pathlib.Path,required=True);parser.add_argument('--output',type=pathlib.Path,required=True);parser.add_argument('--samples',type=int,default=7);parser.add_argument('--request-binary',type=pathlib.Path);args=parser.parse_args()
root=pathlib.Path(__file__).resolve().parents[3];binary=args.binary.resolve();request_binary=(args.request_binary or binary.parent/'examples/semantic_request').resolve();provider=root/'providers/typescript7/provider.mjs';native=root/'providers/typescript7/node_modules/@typescript/typescript-linux-x64/lib/tsc'
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
if args.samples<3:raise SystemExit('At least three samples are required')
with tempfile.TemporaryDirectory(prefix='archguard-semantic-benchmark-') as directory:
    corpus=pathlib.Path(directory);(corpus/'node_modules').symlink_to(root/'research/t3code/semantic/node_modules',target_is_directory=True)
    (corpus/'archguard.json').write_text(json.dumps({'schemaVersion':1,'include':['main.ts'],'rules':[]}))
    (corpus/'tsconfig.json').write_text(json.dumps({'compilerOptions':{'strict':True,'noEmit':True,'skipLibCheck':True,'target':'ESNext','module':'NodeNext','moduleResolution':'NodeNext'},'files':['main.ts']}))
    (corpus/'semantic.json').write_text(json.dumps({'schemaVersion':1,'provider':{'name':'typescript7','command':['node',str(provider)],'timeoutMs':30000},'contexts':[{'id':'history','tsconfig':'tsconfig.json','files':['main.ts']}],'rules':[{'id':'missing-member','kind':'missingMember','files':['**']}]}))
    cases={name:(root/f'research/t3code/semantic/fixtures/{name}.ts').read_text() for name in ['before','fixed']}
    cases['clean-control']='export const unrelated={auth:{webSocketTicket:()=>"clean"}}; unrelated.auth.webSocketTicket();\n'
    report={'sourceRevision':subprocess.check_output(['git','-C',str(root),'rev-parse','HEAD'],text=True).strip(),'sourceStatus':subprocess.check_output(['git','-C',str(root),'status','--porcelain'],text=True),'requestBinarySha256':digest(request_binary),'binarySha256':digest(binary),'providerSha256':digest(provider),'nativeSha256':digest(native),'npmLockSha256':digest(root/'providers/typescript7/package-lock.json'),'reproductionNpmLockSha256':digest(root/'research/t3code/semantic/package-lock.json'),'graphConfigSha256':digest(corpus/'archguard.json'),'compilerConfigSha256':digest(corpus/'tsconfig.json'),'semanticConfigSha256':digest(corpus/'semantic.json'),'nodeSha256':digest(pathlib.Path(shutil.which('node'))),'nodeVersion':subprocess.check_output(['node','--version'],text=True).strip(),'machine':{'platform':platform.platform(),'architecture':platform.machine(),'cpuCount':os.cpu_count()},'backend':{'version':'7.0.2','api':'typescript/unstable/sync'},'samples':args.samples,'cases':{}}
    for name,source in cases.items():
        (corpus/'main.ts').write_text(source)
        data={'sourceSha256':digest(corpus/'main.ts'),'sourceBytes':len(source.encode()),'runs':{}}
        graph=subprocess.check_output([str(binary),'facts','--root',str(corpus),'--config',str(corpus/'archguard.json')])
        provider_request=subprocess.check_output([str(request_binary),str(corpus/'semantic.json')],input=graph)
        data['requestSha256']=hashlib.sha256(provider_request).hexdigest();data['request']=json.loads(provider_request)
        for mode in ['lexical','semantic','provider-only']:
            command=[str(binary),'check','--root',str(corpus),'--config',str(corpus/'archguard.json'),'--json']
            if mode=='semantic':command+=['--semantic-config',str(corpus/'semantic.json')]
            if mode=='provider-only':command=['node',str(provider)]
            rows=[]
            for iteration in range(args.samples):
                started=time.perf_counter();result=subprocess.run(command,capture_output=True,input=provider_request if mode=='provider-only' else None);wall=(time.perf_counter()-started)*1000
                value=json.loads(result.stdout);expected=2 if mode=='semantic' and name=='before' else 0
                if result.returncode!=expected:raise SystemExit(f'{name}/{mode} unexpected exit {result.returncode}: {result.stderr.decode()} {result.stdout.decode()}')
                if name=='before' and mode=='semantic' and not any('auth' in d['message'] for d in value['diagnostics']):raise SystemExit('Historical semantic violation missing')
                if mode=='provider-only':
                    if iteration==0:data['providerFacts']=value
                    if value['complete'] != (name!='before'):raise SystemExit('Unexpected provider completeness')
                    value={'elapsedMs':None,'complete':value['complete'],'diagnostics':[d for c in value['contexts'] for d in c['diagnostics']],'problems':[p for c in value['contexts'] for p in c['problems']],'contexts':value['contexts']}
                normalized={key:item for key,item in value.items() if key!='elapsedMs'}
                rows.append({'iteration':iteration,'wallMs':wall,'reportedMs':value['elapsedMs'],'exit':result.returncode,'complete':value['complete'],'diagnostics':value['diagnostics'],'problems':value['problems'],'normalizedSha256':hashlib.sha256(json.dumps(normalized,sort_keys=True).encode()).hexdigest()})
            if len({row['normalizedSha256'] for row in rows})!=1:raise SystemExit(f'Non-deterministic {name}/{mode} output')
            values=[row['wallMs'] for row in rows];data['runs'][mode]={'raw':rows,'medianMs':statistics.median(values),'minMs':min(values),'maxMs':max(values)}
        data['semanticOverLexical']=data['runs']['semantic']['medianMs']/data['runs']['lexical']['medianMs'];report['cases'][name]=data
    args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({name:{mode:{key:value for key,value in run.items() if key!='raw'} for mode,run in data['runs'].items()} for name,data in report['cases'].items()},indent=2))
