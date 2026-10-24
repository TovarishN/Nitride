"""Measure engine-owned phase allocations without observation JSON in the region."""
import json
import pathlib
import platform
import subprocess
import tempfile
from reference import ROOT


def main():
    profiler = ROOT / "target/release/nitride-profile"
    measurements = []
    with tempfile.TemporaryDirectory() as tmp:
        for mode in ["portable", "power", "xml"]:
            for count in [10, 100, 1000, 5000]:
                if mode == "xml":
                    source = "<configuration>\n" + "\n".join(f'<item key="k{i}" enabled="true">value{i}</item>' for i in range(count)) + "\n</configuration>"
                else:
                    expression = "2 ^ 3" if mode == "power" else "2 * 3"
                    source = "\n".join(f"{i} + {expression};" for i in range(count))
                path = pathlib.Path(tmp) / "input.txt"
                path.write_text(source)
                runs = []
                for _ in range(3):
                    output = subprocess.check_output([profiler, mode, path], text=True)
                    run = json.loads(output)
                    run.pop("values", None)
                    runs.append(run)
                measurements.append({"language":mode,"items":count,"runs":runs})
    report = {"host":platform.platform(),"rustc":subprocess.check_output(["rustc","--version"],text=True).strip(),
        "profiler_executable_bytes":profiler.stat().st_size,"native_executable_bytes":(ROOT / "target/release/nitride").stat().st_size,
        "measurements":measurements,"limitations":[
            "Counts requested live allocation bytes through System, not allocator overhead, RSS or OS-internal realloc copies.",
            "Each phase peak is incremental above its own baseline; do not sum independent peak values as a simultaneous process peak.",
            "Caller source and pre-created host bindings are in the baseline; syntax's owned source copy is included in parse storage.",
            "Static language tables and executable mappings are excluded from allocation counts.",
            "JSON is formatted only after all phase snapshots. XML has parsing admission only.",
            "Profiler atomic accounting affects timings; these are not uninstrumented throughput benchmarks.",
            "Single-threaded fixture handlers; no concurrent allocator activity is expected."]}
    (ROOT / "docs/phase-memory.json").write_text(json.dumps(report,indent=2)+"\n")
    for row in measurements:
        run=row["runs"][0]
        print(row["language"],row["items"],"source",run["source_bytes"],"parse retained",run["parse"]["retained_bytes"],"parse peak",run["parse"]["peak_bytes"],"HIR",run["hir"] and run["hir"]["retained_bytes"])


if __name__ == "__main__":
    main()
