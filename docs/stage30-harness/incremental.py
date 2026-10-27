# Per-edit Roslyn (WithChangedText + GetRoot) against Nitride (--edits) on the same edit scripts.
# usage: incremental.py <nitride-profile> <rinc binary> <out.json>, run from the directory holding the inputs.
# nitride-profile is built with --features uninstrumented-profiler.
import json,subprocess,sys,statistics as st
nit,rinc,out=sys.argv[1:4]
cases=[('in_256k.cs','edits_256k_strict_middle.json'),('in_4m.cs','edits_4m_strict_early.json'),('in_4m.cs','edits_4m_strict_middle.json'),('in_4m.cs','edits_4m_strict_late.json'),
       ('one_4m.cs','edits_4m_one_early.json'),('one_4m.cs','edits_4m_one_middle.json'),('one_4m.cs','edits_4m_one_late.json'),('in_16m.cs','edits_16m_strict_middle.json'),('one_16m.cs','edits_16m_one_middle.json')]
rows=[]
for src,ed in cases:
    r=json.loads(subprocess.run([rinc,src,ed,'3'],capture_output=True,text=True,check=True).stdout)
    ts=[]
    for _ in range(2):
        d=json.loads(subprocess.run([nit,'--native-syntax','--edits','csharp4',src,ed],capture_output=True,text=True,check=True).stdout)
        ts+= [e['phase']['elapsed_ns']/1e9 for e in d['edits']]
    ts.sort(); n=dict(median=st.median(ts),p90=ts[len(ts)*9//10],max=ts[-1])
    row=dict(source=src,edits=ed,roslyn=r,nitride=n); rows.append(row)
    print(f"{ed:32} roslyn med {r['median']*1e3:7.3f}ms p90 {r['p90']*1e3:7.3f}  nitride med {n['median']*1e3:7.3f}ms p90 {n['p90']*1e3:7.3f}  ratio {n['median']/r['median']:.2f}",flush=True)
json.dump(rows,open(out,'w'),indent=1)
