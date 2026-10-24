"""Measure typed template binding, substitution origins and execution."""
import json
import pathlib
import platform
import subprocess
import tempfile
from test_reference import ROOT


def main():
    measurements=[]
    with tempfile.TemporaryDirectory() as folder:
        path=pathlib.Path(folder)/"input.functions"
        for count in [10,100,1000]:
            source='\n'.join(f'def f{i}(x: Scalar)=add(x,x); call f{i}(neg({i}));' for i in range(count))
            path.write_text(source);runs=[]
            for _ in range(3):
                report=json.loads(subprocess.check_output([ROOT/'target/release/nitride-profile','functions',path],text=True))
                assert report['values']==[-2*i for i in range(count)]
                report.pop('values');runs.append(report)
            measurements.append(dict(pairs=count,statements=count,declarations=count*2,references=count*3,runs=runs))
    report=dict(host=platform.platform(),native_executable_bytes=(ROOT/'target/release/nitride').stat().st_size,
                profiler_executable_bytes=(ROOT/'target/release/nitride-profile').stat().st_size,measurements=measurements,
                metric='Requested System bytes; separate phase baselines; excludes JSON, caller input, static tables, allocator overhead and RSS')
    (ROOT/'docs/functions-memory.json').write_text(json.dumps(report,indent=2)+'\n')
    print('Recorded 10/100/1000 functions declaration-pair workloads')


if __name__=='__main__':main()
