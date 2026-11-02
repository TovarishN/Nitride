"""Run each engine on each input in fresh processes; print a JSON report."""
import json
import pathlib
import platform
import re
import statistics
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
INPUTS = HERE / "target/json-bench"
ENGINES = {
    "nitride": [str(HERE / "target/release/nitride-json-bench"), "bench"],
    # Built by pgo.py; skipped when absent.
    "nitride-pgo": [str(HERE / "target/pgo/release/nitride-json-bench"),
                    "bench"],
    "stj-document": [str(HERE / "dotnet/bin/Release/net10.0/JsonBench"), "document"],
    "stj-reader": [str(HERE / "dotnet/bin/Release/net10.0/JsonBench"), "reader"],
}
HEAP = HERE / "target/heap/release/nitride-json-bench"
PROCESSES, WARM = 5, 10


def run(command):
    # /usr/bin/time -l reports the process's peak resident set on macOS.
    done = subprocess.run(["/usr/bin/time", "-l", *command], capture_output=True, text=True, check=True)
    sample = json.loads(done.stdout)
    sample["peak_rss_bytes"] = int(re.search(r"(\d+)\s+maximum resident set size", done.stderr)[1])
    return sample


def extras():
    """Edit timings and heap use; `heap` needs `cargo build --release --features heap --target-dir target/heap`."""
    edits = [{"input": name, **json.loads(subprocess.run([ENGINES["nitride"][0], "edit", str(INPUTS / f"{name}.json"), "20"],
                                                          capture_output=True, text=True, check=True).stdout)}
             for name in ("numbers-1m", "records-16m")]
    heap = [{"input": name, **json.loads(subprocess.run([str(HEAP), "heap", str(INPUTS / f"{name}.json")],
                                                         capture_output=True, text=True, check=True).stdout)}
            for name in ("records-16m", "numbers-16m", "strings-16m")]
    return {"edit": edits, "heap": heap}


def main():
    rows = []
    for shape in ("records", "numbers", "strings"):
        for size in ("64k", "1m", "16m"):
            path = INPUTS / f"{shape}-{size}.json"
            for engine, command in ENGINES.items():
                if not pathlib.Path(command[0]).exists():
                    continue
                samples = [run([*command, str(path), str(WARM)]) for _ in range(PROCESSES)]
                med = lambda key: statistics.median(s[key] for s in samples)
                rows.append({"shape": shape, "size": size, "engine": engine, "source_bytes": samples[0]["source_bytes"],
                             "cold_ns": med("cold_ns"), "warm_ns": med("warm_median_ns"), "peak_rss_bytes": med("peak_rss_bytes"),
                             "samples": samples})
                print(f"{shape}-{size} {engine}: cold {rows[-1]['cold_ns']/1e6:.2f} ms warm {rows[-1]['warm_ns']/1e6:.2f} ms", file=sys.stderr)
    cpu = subprocess.run(["sysctl", "-n", "machdep.cpu.brand_string"], capture_output=True, text=True).stdout.strip()
    dotnet = subprocess.run(["dotnet", "--version"], capture_output=True, text=True).stdout.strip()
    rustc = subprocess.run(["rustc", "--version"], capture_output=True, text=True).stdout.strip()
    print(json.dumps({"cpu": cpu, "os": platform.platform(), "dotnet_sdk": dotnet, "rustc": rustc,
                      "processes": PROCESSES, "warm_iterations": WARM, "rows": rows,
                      **({"extras": extras()} if "--extras" in sys.argv else {})}, indent=1))


main()
