"""Measure iterative type inference and selected concrete operation execution."""
import json
import pathlib
import platform
import subprocess
import tempfile
from test_reference import ROOT


def main():
    measurements=[]
    with tempfile.TemporaryDirectory() as folder:
        path=pathlib.Path(folder)/"input.inferred"
        for count in [10,100,1000]:
            source='\n'.join(f'let x{i}=duration({i}); rate x{i}={i+1}; either(x{i})+duration(2); reverse(x{i})-1;' for i in range(count))
            path.write_text(source);runs=[]
            for _ in range(3):
                report=json.loads(subprocess.check_output([ROOT/'target/release/nitride-profile','inference',path],text=True))
                assert report['values']==[v for i in range(count) for v in [i+2,i]]
                report.pop('values');runs.append(report)
            measurements.append(dict(pairs=count,statements=count*4,declarations=count*2,references=count*2,runs=runs))
    report=dict(host=platform.platform(),native_executable_bytes=(ROOT/'target/release/nitride').stat().st_size,
                profiler_executable_bytes=(ROOT/'target/release/nitride-profile').stat().st_size,measurements=measurements,
                metric='Requested System bytes; separate phase baselines; excludes JSON, caller input, static tables, allocator overhead and RSS')
    (ROOT/'docs/inference-memory.json').write_text(json.dumps(report,indent=2)+'\n')
    print('Recorded 10/100/1000 inferred declaration-pair workloads')


if __name__=='__main__':main()
