"""Shared-subtree evidence; allocator peaks stay phase-local, never summed."""
import argparse
import hashlib
import json
import math
import pathlib
import platform
import subprocess
import tempfile
from csharp4_scaling import generated
from recovery_scaling import damaged

ROOT = pathlib.Path(__file__).resolve().parents[1]

def source_digest():
    names=subprocess.check_output(["rg","--files","crates","scripts","languages"],cwd=ROOT,text=True).splitlines()
    digest=hashlib.sha256()
    for name in sorted(names+["Cargo.toml","Cargo.lock"]):
        digest.update(name.encode());digest.update(b"\0");digest.update((ROOT/name).read_bytes())
    return digest.hexdigest()

def unique_storage_bytes(readers):
    chunks = {}
    for reader in readers:
        for identity, size in reader:
            if size < 0 or (identity in chunks and chunks[identity] != size):
                raise ValueError("inconsistent physical chunk size")
            chunks[identity] = size
    return sum(chunks.values())

def percentile(samples, percent):
    if not samples or not 0 < percent <= 100:
        raise ValueError("percentile requires samples and 0 < p <= 100")
    ordered = sorted(samples)
    return ordered[max(0, math.ceil(len(ordered) * percent / 100) - 1)]

def summarize_edits(report, warmups):
    all_rows = report["edits"]
    rows = all_rows[warmups:]
    times = [row["phase"]["elapsed_ns"] for row in rows]
    if not times:
        raise ValueError("no timed edits after warmup")
    stats = [row["snapshot"]["stats"] for row in all_rows]
    copies = [s for s in stats if s["syntax_bytes_copied"] is not None and s["source_bytes_copied"] is not None]
    return {"samples": len(times), "validated_samples":sum(row.get("equivalent") is not None for row in rows), "p50_ns": percentile(times, 50),
            "p95_ns": percentile(times, 95), "max_ns": max(times),
            "warmup_max_ns":max((row["phase"]["elapsed_ns"] for row in all_rows[:warmups]),default=0),
            "max_all_ns":max(row["phase"]["elapsed_ns"] for row in all_rows),
            "validated_all_samples":sum(row.get("equivalent") is not None for row in all_rows),
            "fallbacks": sum(s["full_parse_fallbacks"] for s in stats),
            "unknown_copy_samples": len(stats)-len(copies),
            "max_copied_bytes": max((s["syntax_bytes_copied"]+s["source_bytes_copied"] for s in copies), default=None),
            "equivalence_failures": sum(row.get("equivalent") is False for row in all_rows),
            "max_reader_union_bytes": max((row.get("shared_reader_bytes", 0) for row in all_rows), default=0)}

def cold_gate_failures(old, new):
    bounds = {"elapsed_ns": 1.25, "peak_bytes": 1.50, "retained_bytes": 1.35}
    return [{"metric": key, "actual": new[key], "baseline": old[key], "limit_ratio": ratio}
            for key, ratio in bounds.items() if new[key] > old[key]*ratio]

def measure(args):
    profiler = args.profiler.resolve() if args.profiler else ROOT / "target/release/nitride-profile"
    latency_only = getattr(args,"latency_only",False)
    previous = None if latency_only else json.loads(args.baseline_report.read_text())
    destination = args.report.resolve()
    if destination.exists():
        raise ValueError("measurement report already exists; preserve captured evidence")
    rows = []
    report = {"host": platform.platform(), "machine": platform.machine(),
              "source_sha256":source_digest(), "profiler_sha256": hashlib.sha256(profiler.read_bytes()).hexdigest(),
              "warmups": args.warmups, "timed_edits": args.edits, "latency_only":latency_only,
              "measurements": rows, "failures": []}
    def save():
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(json.dumps(report, indent=2)+"\n")
    def invoke(command):
        result = subprocess.run(command, capture_output=True, text=True)
        if result.returncode:
            return {"process_failure": result.returncode, "stderr": result.stderr}
        return json.loads(result.stdout)
    with tempfile.TemporaryDirectory() as folder:
        source_path = pathlib.Path(folder)/"input.cs"
        config_path = pathlib.Path(folder)/"edits.json"
        for size in args.sizes:
            healthy = generated(size)
            for pattern in ["strict"] + args.patterns:
                source = healthy if pattern == "strict" else damaged(healthy, pattern)
                source_path.write_text(source)
                cold = [invoke([str(profiler), "--snapshot" if pattern == "strict" else "--snapshot-recover", "csharp4", str(source_path)]) for _ in range(args.runs)]
                row = {"size_target": size, "pattern": pattern,
                       "source_sha256": hashlib.sha256(source.encode()).hexdigest(), "cold": cold, "edits": []}
                rows.append(row)
                if any("process_failure" in sample for sample in cold):
                    report["failures"].append({"size": size, "pattern": pattern, "kind": "cold process failure"})
                    save()
                    continue
                if latency_only:
                    legacy = [invoke([str(profiler)] + ([] if pattern=="strict" else ["--recover"]) + ["csharp4", str(source_path)]) for _ in range(args.runs)]
                    row["legacy_cold"] = legacy
                    if any("process_failure" in sample for sample in legacy):
                        report["failures"].append({"size":size,"pattern":pattern,"kind":"legacy process failure"})
                        save()
                        continue
                    old_phases = [s["parse"] for s in legacy]
                    metrics = ("elapsed_ns",)
                    if any(s["parse"].get("allocation_instrumented") is not False for s in cold+legacy):
                        raise ValueError("latency-only requires an uninstrumented-profiler binary")
                else:
                    old = next(r for r in previous["measurements"] if r["profile"] == "csharp4" and r["size_target"] == size and r["pattern"] == pattern)
                    old_phases = [s["result"]["parse"] for s in old["samples"]]
                    metrics = ("elapsed_ns","peak_bytes","retained_bytes")
                new_phases = [s["parse"] for s in cold]
                old_medians = {k: percentile([s[k] for s in old_phases],50) for k in metrics}
                new_medians = {k: percentile([s[k] for s in new_phases],50) for k in metrics}
                row["cold_gate_failures"] = ([{"metric":"elapsed_ns","actual":new_medians["elapsed_ns"],"baseline":old_medians["elapsed_ns"],"limit_ratio":1.25}]
                    if new_medians["elapsed_ns"] > old_medians["elapsed_ns"]*1.25 else []) if latency_only else cold_gate_failures(old_medians,new_medians)
                report["failures"] += [{"size":size,"pattern":pattern,"kind":"cold",**failure} for failure in row["cold_gate_failures"]]
                save()
                # Local strict/one-error latency gates; other damage patterns have cold scaling only.
                if pattern not in ("strict", "one"):
                    continue
                encoded = source.encode()
                candidates = []
                offset = encoded.find(b"return x * 2")
                while offset >= 0:
                    candidates.append(offset+11)
                    offset = encoded.find(b"return x * 2",offset+1)
                if not candidates:
                    raise ValueError("generated method edit target missing")
                for location in args.locations:
                    fraction = {"early":0.1,"middle":0.5,"late":0.9,"alternating":0.1}[location]
                    at = candidates[min(len(candidates)-1,int(len(candidates)*fraction))]
                    for readers in args.readers:
                        config = {"recover":pattern != "strict", "validate":args.validate, "retain":readers,
                                  "edits":[{"range":[position,position+1],"replacement":"3" if (i//2 if location=="alternating" else i)%2 == 0 else "2"}
                                           for i in range(args.edits+args.warmups)
                                           for position in [candidates[min(len(candidates)-1,int(len(candidates)*(0.1 if i%2==0 else 0.9)))] if location=="alternating" else at]]}
                        config_path.write_text(json.dumps(config))
                        result = invoke([str(profiler),"--edits","csharp4",str(source_path),str(config_path)])
                        case = {"location":location,"readers":readers,"result":result}
                        row["edits"].append(case)
                        if "process_failure" in result:
                            report["failures"].append({"size":size,"pattern":pattern,"kind":"edit process failure"})
                        else:
                            summary = summarize_edits(result,args.warmups)
                            case["summary"] = summary
                            if summary["equivalence_failures"]:
                                report["failures"].append({"size":size,"pattern":pattern,"location":location,"readers":readers,"kind":"equivalence","mismatches":summary["equivalence_failures"]})
                            limit = {262144:50_000_000,4194304:100_000_000,16777216:200_000_000}.get(size,200_000_000)
                            if summary["p95_ns"] > min(limit,new_medians["elapsed_ns"]*0.2):
                                report["failures"].append({"size":size,"pattern":pattern,"location":location,"readers":readers,"kind":"edit latency","actual_ns":summary["p95_ns"]})
                            if summary["unknown_copy_samples"] or (summary["max_copied_bytes"] or 0) > 262144:
                                report["failures"].append({"size":size,"pattern":pattern,"kind":"copy gate unproved or exceeded"})
                        save()
                        print(json.dumps({"size":size,"pattern":pattern,"location":location,"readers":readers,"summary":case.get("summary")}),flush=True)
    return int(bool(report["failures"]))

def roundtrip_edits(source, edits):
    raw=source.encode()
    forward=[]
    inverse=[]
    for edit in edits:
        a,b=edit["start"],edit["end"]
        replacement=edit["replacement"]
        original=raw[a:b].decode()
        forward.append({"range":[a,b],"replacement":replacement})
        inverse.append({"range":[a,a+len(replacement.encode())],"replacement":original})
        raw=raw[:a]+replacement.encode()+raw[b:]
    return forward+list(reversed(inverse)),raw.decode()

def differential(args):
    profiler = args.profiler.resolve() if args.profiler else ROOT/"target/release/nitride-profile"
    manifest = json.loads(args.manifest.read_text())
    baselines = {b["id"]:b for b in manifest["baselines"]}
    report = {"source_sha256":source_digest(),"profiler_sha256":hashlib.sha256(profiler.read_bytes()).hexdigest(),
              "manifest_sha256":hashlib.sha256(args.manifest.read_bytes()).hexdigest(),
              "seed":manifest["seed"],"compiled_lexers":args.native_lexers or args.native_syntax,"compiled_syntax":args.native_syntax,"cases":[],"failures":[]}
    if args.report.exists():
        raise ValueError("differential report already exists; preserve captured evidence")
    with tempfile.TemporaryDirectory() as folder:
        source_path=pathlib.Path(folder)/"input.txt"
        config_path=pathlib.Path(folder)/"edits.json"
        for number,variant in enumerate(manifest["variants"][:args.limit]):
            baseline=baselines[variant["baseline"]]
            source_path.write_text(baseline["source"])
            edits,changed=roundtrip_edits(baseline["source"],variant.get("edits") or [variant["edit"]])
            assert changed==variant["source"], "manifest edit trace differs from frozen source"
            config={"recover":True,"validate":True,"retain":8,"edits":edits}
            config_path.write_text(json.dumps(config))
            outcome=subprocess.run([str(profiler)]+(["--native-syntax"] if args.native_syntax else ["--native-lexers"] if args.native_lexers else [])+["--edits",baseline["profile"],str(source_path),str(config_path)],capture_output=True,text=True)
            row={"index":number,"baseline":baseline["id"],"operation":variant.get("operation",variant.get("group","multiple")),"returncode":outcome.returncode}
            if outcome.returncode:
                row["stderr"]=outcome.stderr
            else:
                result=json.loads(outcome.stdout)
                row["equivalent"]=[r["equivalent"] for r in result["edits"]]
                row["fallbacks"]=[r["snapshot"]["stats"]["full_parse_fallbacks"] for r in result["edits"]]
            report["cases"].append(row)
            if outcome.returncode or not all(row["equivalent"]):
                report["failures"].append(row)
            if number%100==0 or number+1==min(len(manifest["variants"]),args.limit):
                args.report.parent.mkdir(parents=True,exist_ok=True)
                args.report.write_text(json.dumps(report,indent=2)+"\n")
                print(json.dumps({"cases":number+1,"failures":len(report["failures"])}),flush=True)
    return int(bool(report["failures"]))

def baseline(args):
    profiler = args.profiler.resolve() if args.profiler else ROOT / "target/release/nitride-profile"
    destination = args.report.resolve()
    if destination.exists():
        raise ValueError("baseline report already exists; preserve captured evidence")
    rows = []
    report = {"host": platform.platform(), "machine": platform.machine(),
              "source_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
              "profiler_sha256": hashlib.sha256(profiler.read_bytes()).hexdigest(),
              "runs_per_case": args.runs, "measurements": rows,
              "scope": "Release process-cold runs, parse phase only; requested allocator bytes exclude caller input, allocator overhead and OS/thread mappings; independent peaks are not summed"}
    with tempfile.TemporaryDirectory() as folder:
        source_path = pathlib.Path(folder) / "input.txt"
        cases = []
        for size in args.sizes:
            source = generated(size)
            cases.append(("csharp4", size, "strict", source))
            for pattern in args.patterns:
                cases.append(("csharp4", size, pattern, damaged(source, pattern)))
        cases += [("portable", 0, "strict", "1 + 2 * 3;\n" * 100),
                  ("power", 0, "strict", "let x : Scalar = 2; x ^ 3;"),
                  ("xml", 0, "strict", "<root><item/></root>")]
        for profile, size, pattern, source in cases:
            source_path.write_text(source)
            samples = []
            for run in range(args.runs):
                command = [str(profiler)] + (["--recover"] if pattern != "strict" else []) + [profile, str(source_path)]
                outcome = subprocess.run(command, capture_output=True, text=True)
                sample = {"run": run, "returncode": outcome.returncode}
                if outcome.returncode:
                    sample["stderr"] = outcome.stderr
                else:
                    sample["result"] = json.loads(outcome.stdout)
                samples.append(sample)
                print(json.dumps({"profile": profile, "size": size, "pattern": pattern, **sample}), flush=True)
            rows.append({"profile": profile, "size_target": size, "pattern": pattern,
                         "source_sha256": hashlib.sha256(source.encode()).hexdigest(), "samples": samples})
            destination.parent.mkdir(parents=True, exist_ok=True)
            # Flush each completed case: a later failure must not erase earlier evidence.
            destination.write_text(json.dumps(report, indent=2) + "\n")
    return 1 if any(s["returncode"] for row in rows for s in row["samples"]) else 0

def main():
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--baseline", action="store_true")
    mode.add_argument("--measure", action="store_true")
    mode.add_argument("--differential", action="store_true")
    parser.add_argument("--manifest", type=pathlib.Path, default=ROOT/"fixtures/recovery/manifest.json")
    parser.add_argument("--limit", type=int, default=4334)
    parser.add_argument("--baseline-report", type=pathlib.Path, default=ROOT/"docs/stage15-baseline.json")
    parser.add_argument("--edits", type=int, default=100)
    parser.add_argument("--warmups", type=int, default=10)
    parser.add_argument("--locations", nargs="+", choices=["early","middle","late","alternating"], default=["early","middle","late","alternating"])
    parser.add_argument("--readers", nargs="+", type=int, default=[0,8])
    parser.add_argument("--validate", action="store_true")
    parser.add_argument("--latency-only", action="store_true", help="use uninstrumented same-build legacy cold timing; allocator admission is separate")
    parser.add_argument("--report", type=pathlib.Path, required=True)
    parser.add_argument("--sizes", type=int, nargs="+", default=[256*1024, 4*1024*1024, 16*1024*1024])
    parser.add_argument("--patterns", nargs="+", default=["one", "dense", "junk", "truncated"])
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--profiler", type=pathlib.Path)
    parser.add_argument("--native-lexers", action="store_true", help="opt into generated scanners for differential cold snapshots")
    parser.add_argument("--native-syntax", action="store_true", help="opt into generated syntax and lexers for differential cold snapshots")
    args = parser.parse_args()
    if (args.native_lexers or args.native_syntax) and not args.differential:
        parser.error("native generation currently supports --differential only")
    if args.runs < 1 or args.edits<1 or args.warmups<0 or any(r<0 or r>1024 for r in args.readers) or any(size < 1024 for size in args.sizes):
        parser.error("positive runs and sizes >=1024 are required")
    return baseline(args) if args.baseline else differential(args) if args.differential else measure(args)

if __name__ == "__main__":
    raise SystemExit(main())
