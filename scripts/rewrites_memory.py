"""Measure quote rewrite expansion, re-emission and execution."""
import json
import platform
import pathlib
import subprocess
import tempfile
from test_reference import ROOT


def main():
    measurements=[]
    with tempfile.TemporaryDirectory() as folder:
        path=pathlib.Path(folder)/"input.rewrites"
        for count in [10,100,1000]:
            source='\n'.join(f'let n{i} = {i} + 1 swapped; twice(n{i} fortnights) in days;' for i in range(count))
            path.write_text(source);runs=[]
            for _ in range(3):
                report=json.loads(subprocess.check_output([ROOT/'target/release/nitride-profile','rewrites',path],text=True))
                assert report['values']==[(i+1)*28 for i in range(count)]
                report.pop('values');runs.append(report)
            measurements.append(dict(pairs=count,statements=count*2,declarations=count,references=count,swapped_uses=count,fortnight_uses=count,twice_uses=count,runs=runs))
    report=dict(host=platform.platform(),native_executable_bytes=(ROOT/'target/release/nitride').stat().st_size,
                profiler_executable_bytes=(ROOT/'target/release/nitride-profile').stat().st_size,measurements=measurements,
                metric='Requested System bytes; separate phase baselines; excludes JSON, caller input, static tables, allocator overhead and RSS')
    (ROOT/'docs/stage11-rewrites-memory.json').write_text(json.dumps(report,indent=2)+'\n')
    print('Recorded 10/100/1000 rewrites declaration-pair workloads')


if __name__=='__main__':main()
