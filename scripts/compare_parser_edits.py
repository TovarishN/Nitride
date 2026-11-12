"""Compare two `nitride-profile` builds: retained storage and snapshot edit latency.

Build each revision's profiler first, for example:

    cargo build --release -p nitride-cli --bin nitride-profile --target-dir target/a
    python3 -I scripts/compare_parser_edits.py target/a/release/nitride-profile \
        target/b/release/nitride-profile --report target/reports/edits.json

Inputs come from target/parser-compare/ (see compare_parser_builds.py). Each edit
case alternates one digit near the middle of the file 60 times; the median edit
phase is reported with the median reparsed bytes. Storage is a cold strict
snapshot of the 4 MiB valid input with allocation-instrumented requested bytes.
"""
import argparse
import json
import pathlib
import re
import statistics
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from compare_parser_builds import inputs  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
EDITS = [("256k-valid", False), ("4m-valid", False), ("4m-one", True)]


def script(folder, name, recover, count=60):
    text = (folder / f"{name}.cs").read_text()
    middle = len(text) // 2
    at = len(text[: next(m.start() for m in re.finditer(r"\d", text[middle:])) + middle].encode())
    edits = [{"range": [at, at + 1], "replacement": "12"[i % 2]} for i in range(count)]
    path = folder / f"edits-{name}.json"
    path.write_text(json.dumps({"recover": recover, "validate": False, "retain": 0, "edits": edits}))
    return path


def measure(profile, folder, scripts):
    snap = json.loads(subprocess.check_output(
        [profile, "--native-syntax", "--snapshot", "csharp4", str(folder / "4m-valid.cs")]))
    row = {"index_bytes": snap["storage"]["index_bytes"],
           "retained_bytes": snap["storage"]["physical_retained_bytes"],
           "peak_bytes": snap["parse"]["peak_bytes"], "edits": {}}
    for name, _ in EDITS:
        out = json.loads(subprocess.check_output(
            [profile, "--native-syntax", "--edits", "csharp4", str(folder / f"{name}.cs"), str(scripts[name])]))
        row["edits"][name] = {
            "median_ms": statistics.median(e["phase"]["elapsed_ns"] / 1e6 for e in out["edits"]),
            "median_reparsed_bytes": statistics.median(e["snapshot"]["stats"]["reparsed_bytes"] for e in out["edits"]),
            "samples_ms": [e["phase"]["elapsed_ns"] / 1e6 for e in out["edits"]],
        }
    return row


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("baseline")
    parser.add_argument("candidate")
    parser.add_argument("--report", type=pathlib.Path)
    args = parser.parse_args()
    folder = ROOT / "target/parser-compare"
    folder.mkdir(parents=True, exist_ok=True)
    inputs(folder)
    scripts = {name: script(folder, name, recover) for name, recover in EDITS}
    rows = {"baseline": measure(args.baseline, folder, scripts),
            "candidate": measure(args.candidate, folder, scripts)}
    mib = 2 ** 20
    for key in ("index_bytes", "retained_bytes", "peak_bytes"):
        a, b = rows["baseline"][key], rows["candidate"][key]
        print(f"{key:15s} {a / mib:8.1f} MiB -> {b / mib:8.1f} MiB ({100 * (b / a - 1):+.1f}%)")
    for name, _ in EDITS:
        a, b = rows["baseline"]["edits"][name], rows["candidate"]["edits"][name]
        print(f"edit {name:10s} {a['median_ms']:8.2f} ms -> {b['median_ms']:8.2f} ms "
              f"({100 * (b['median_ms'] / a['median_ms'] - 1):+.1f}%), reparsed "
              f"{a['median_reparsed_bytes']:.0f} -> {b['median_reparsed_bytes']:.0f} B")
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(rows, indent=1) + "\n")


if __name__ == "__main__":
    main()
