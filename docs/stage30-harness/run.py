import json,subprocess,re,statistics as st,sys,os,platform
S='/private/tmp/claude-501/-Users-dmitrikamenetski-work-Nitride/f9903ec3-9ffe-4715-a983-b1da6aaa16db/scratchpad'
B=S+'/cmp'
tools={
 'nitride':      lambda mode,f,extra: [B+'/nbench/target/release/nbench',mode,'recover',f]+extra,
 'nitride-strict':lambda mode,f,extra: [B+'/nbench/target/release/nbench',mode,'strict',f]+extra,
 'roslyn':       lambda mode,f,extra: ['dotnet',B+'/rcmp/bin/Release/net10.0/rcmp.dll',mode,f]+extra,
 'nitrogen':     lambda mode,f,extra: ['dotnet',B+'/ncmp/bin/Release/net10.0/ncmp.dll',mode,f]+extra,
}
def run(cmd):
    out=subprocess.run(['/usr/bin/time','-l']+cmd,capture_output=True,text=True)
    if out.returncode: return {'error':out.stderr[-400:]}
    d=json.loads(out.stdout.strip().splitlines()[-1])
    d['peak_rss_mib']=int(re.search(r'(\d+)\s+maximum resident set size',out.stderr).group(1))/2**20
    return d
cases=[(f'{S}/paired_{n}_{p}.cs',n,p) for n in (262144,4194304) for p in ('strict','one','dense','junk')]+[(f'{S}/paired_16777216_{p}.cs',16777216,p) for p in ('strict','one','junk')]
rows=[]
baseline={t:run(tools[t]('cold',B+'/tiny.cs',[])) for t in ('nitride','roslyn','nitrogen')}
print(json.dumps({'baseline':baseline}),flush=True)
for f,n,p in cases:
    for t in tools:
        if t=='nitride-strict' and p!='strict': continue
        slow = t.startswith('nitride') and ((p=='dense' and n>262144) or (n==16777216 and p!='strict'))
        cold=[run(tools[t]('cold',f,[])) for _ in range(3)]
        warm=run(tools[t]('warm',f,['1','2'] if slow else ['2','5']))
        ok=[c for c in cold if 'seconds' in c]
        row=dict(tool=t,bytes=n,pattern=p,
            cold_s=st.median([c['seconds'] for c in ok]) if ok else None,
            cold_peak_rss_mib=st.median([c['peak_rss_mib'] for c in ok]) if ok else None,
            warm_s=warm.get('seconds'),warm_peak_rss_mib=warm.get('peak_rss_mib'),
            retained_bytes=(ok[0] if ok else warm).get('retained_bytes'),
            diagnostics=(ok[0] if ok else warm).get('diagnostics'),
            nodes=(ok[0] if ok else warm).get('nodes'),success=(ok[0] if ok else warm).get('success'),
            cold=cold,warm=warm)
        rows.append(row)
        print(json.dumps({k:row[k] for k in ('tool','bytes','pattern','cold_s','warm_s','cold_peak_rss_mib','retained_bytes','diagnostics','nodes')}),flush=True)
json.dump(dict(host=platform.platform(),baseline=baseline,rows=rows),open(B+'/results.json','w'),indent=1)
print('done',flush=True)
