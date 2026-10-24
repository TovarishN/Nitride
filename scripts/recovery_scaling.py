"""Native parse-only recovery timing/storage/work; serialization and .NET excluded."""
import argparse
import hashlib
import json
import pathlib
import platform
import subprocess
import tempfile
from csharp4_scaling import generated
from recovery_corpus import ROOT


def damaged(source, pattern):
    middle = len(source)//2
    if pattern == "one":
        at = source.index(";",middle)
        return source[:at]+source[at+1:]
    if pattern == "dense":
        return source.replace("return x;", "return x")
    if pattern == "junk":
        at = source.index("return",middle)
        return source[:at]+("@ "*64)+source[at:]
    if pattern == "truncated":
        return source[:len(source)*3//4]
    raise ValueError(pattern)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--sizes",type=int,nargs="+",default=[256*1024,4*1024*1024,16*1024*1024])
    parser.add_argument("--patterns",nargs="+",default=["one","dense","junk","truncated"])
    args = parser.parse_args()
    rows = []
    with tempfile.TemporaryDirectory() as folder:
        path = pathlib.Path(folder)/"input.cs"
        for size in args.sizes:
            base = generated(size)
            for pattern in args.patterns:
                source = damaged(base,pattern)
                path.write_text(source)
                result = subprocess.run([str(ROOT/"target/release/nitride-profile"),"--recover","csharp4",str(path)],capture_output=True,text=True,check=True)
                row = {"size_target":size,"pattern":pattern,**json.loads(result.stdout)}
                rows.append(row)
                print(json.dumps({"bytes":len(source),"pattern":pattern,"seconds":row["parse"]["elapsed_ns"]/1e9,"peak_bytes":row["parse"]["peak_bytes"],"work":row["recovery"]["work"],"fallbacks":row["recovery"]["fallback_count"]}),flush=True)
    failures = []
    for pattern in args.patterns:
        cases = [row for row in rows if row["pattern"] == pattern]
        first = cases[0]
        for row in cases[1:]:
            a = first["recovery"]["work"] / first["source_bytes"]
            b = row["recovery"]["work"] / row["source_bytes"]
            if a and b > 2*a:
                failures.append(f"{pattern}: work/byte more than doubles at {row['source_bytes']} bytes")
    report = {"profiler_sha256":hashlib.sha256((ROOT/"target/release/nitride-profile").read_bytes()).hexdigest(),"counter_scope":"recovery VM transitions and strict probes; initial failed strict parse and retry attempts excluded; elapsed includes the entire recovery API","allocator_scope":"requested System bytes; excludes thread stacks, allocator overhead, OS mappings, JSON serialization and caller input","host":platform.platform(),"machine":platform.machine(),"runs_per_case":1,"failures":failures,"measurements":rows}
    (ROOT/"docs/stage14-recovery-scaling.json").write_text(json.dumps(report,indent=2)+"\n")
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
