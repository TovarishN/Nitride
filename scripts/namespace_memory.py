"""Record requested engine allocations for paired independent namespaces."""
import json
import pathlib
import platform
import subprocess
import tempfile
from test_reference import ROOT


def main():
    rows=[]
    with tempfile.TemporaryDirectory() as folder:
        path=pathlib.Path(folder)/"source.namespaces"
        for count in [10,100,1000]:
            source="\n".join(f"let x{i}:Scalar={i}; rate x{i}:Scalar={i+1}; either(x{i}); reverse(x{i});" for i in range(count))
            path.write_text(source)
            runs=[]
            for _ in range(3):
                report=json.loads(subprocess.check_output([ROOT/"target/release/nitride-profile","namespaces",path],text=True))
                assert report["values"]==[v for i in range(count) for v in [i,i+1]]
                report.pop("values")
                runs.append(report)
            rows.append({"pairs":count,"statements":count*4,"declarations":count*2,"references":count*2,"runs":runs})
    data={"host":platform.platform(),"native_executable_bytes":(ROOT/"target/release/nitride").stat().st_size,
          "profiler_executable_bytes":(ROOT/"target/release/nitride-profile").stat().st_size,"measurements":rows,
          "metric":"Requested System allocation bytes; independent phase baselines; no JSON in measured regions; excludes caller source/static data/allocator overhead/RSS"}
    (ROOT/"docs/namespace-memory.json").write_text(json.dumps(data,indent=2)+"\n")
    print("Recorded namespace workloads with 10, 100, and 1000 declaration pairs")


if __name__=="__main__":main()
