"""Reproducible native parser checkpoint; build harnesses before running.

Timings belong to the named executable, never to shipping PGO artifacts.
"""
import argparse
import hashlib
import importlib.util
import json
import os
import pathlib
import platform
import re
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def command(args, cwd=ROOT):
    return subprocess.check_output([str(a) for a in args], cwd=cwd, stderr=subprocess.PIPE)


def source_identity(root=ROOT):
    def git(*args):
        return command(["git", *args], root)
    try:
        revision = git("rev-parse", "HEAD").decode().strip()
    except subprocess.CalledProcessError:
        revision = None
    diff = git("diff", "HEAD") if revision else git("diff")
    untracked = git("ls-files", "--others", "--exclude-standard", "-z")
    for name in sorted(filter(None, untracked.split(b"\0"))):
        path = root / os.fsdecode(name)
        if path.is_file():
            diff += name + b"\0" + path.read_bytes()
    return {"revision": revision, "diff_sha256": sha(diff), "dirty": bool(diff)}


def summarize(case, input_hash, samples):
    for key in ("fingerprint", "source_bytes", "nodes", "diagnostics"):
        observed = {s[key] for s in samples if key in s}
        if len(observed) > 1:
            raise ValueError(f"{case}: inconsistent {key}: {observed}")
    keys = set.intersection(*(set(s) for s in samples))
    medians = {key: statistics.median(s[key] for s in samples)
               for key in sorted(keys)
               if all(type(s[key]) in (int, float) for s in samples)}
    return {"case": case, "input_sha256": input_hash, "medians": medians, "samples": samples}


def compare(before, after):
    if before["case"] != after["case"] or before["input_sha256"] != after["input_sha256"]:
        raise ValueError("cannot compare different cases or input hashes")
    edit_samples = any(type(s.get("edits")) is int
                       for row in (before, after) for s in row.get("samples", []))
    if before.get("requires_equivalence") or after.get("requires_equivalence") or edit_samples:
        for row in (before, after):
            samples = row.get("samples", [])
            if not samples or any(not s.get("equivalence_checked") or not s.get("fingerprint") for s in samples):
                raise ValueError("missing fresh-parse equivalence evidence")
    for key in ("fingerprint",):
        a = {s[key] for s in before.get("samples", []) if key in s}
        b = {s[key] for s in after.get("samples", []) if key in s}
        if a and b and a != b:
            raise ValueError(f"cannot compare different {key}")
    return {"case": before["case"], "ratios": {
        key: after["medians"][key] / value
        for key, value in before["medians"].items()
        if value and key in after["medians"] and key.endswith(("_ns", "_bytes"))
    }}


def measured(args):
    started = time.perf_counter_ns()
    if sys.platform == "darwin":
        result = subprocess.run(["/usr/bin/time", "-l", *map(str, args)], cwd=ROOT,
                                capture_output=True, text=True, check=True)
        match = re.search(r"(\d+)\s+maximum resident set size", result.stderr)
        rss = int(match[1]) if match else None
    else:
        result = subprocess.run(list(map(str, args)), cwd=ROOT, capture_output=True,
                                text=True, check=True)
        rss = None
    sample = json.loads(result.stdout)
    sample["process_wall_ns"] = time.perf_counter_ns() - started
    sample["peak_rss_bytes"] = rss
    return sample


def load_generator(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--suite", choices=["csharp", "json", "edits", "all"], default="all")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--smoke", action="store_true")
    parser.add_argument("--audit", type=pathlib.Path, default=ROOT / "scripts/compiler-audit/target/release/nitride-compiler-audit")
    parser.add_argument("--json", type=pathlib.Path, default=ROOT / "scripts/json-bench/target/release/nitride-json-bench")
    parser.add_argument("--heap", type=pathlib.Path, default=ROOT / "scripts/json-bench/target/heap/release/nitride-json-bench")
    parser.add_argument("--profile", type=pathlib.Path, default=ROOT / "target/release/nitride-profile")
    args = parser.parse_args()
    identity = source_identity()
    csharp = load_generator(ROOT / "scripts/csharp4_scaling.py", "csharp")
    recovery = load_generator(ROOT / "scripts/recovery_scaling.py", "recovery")
    # The corpus generator reads argv at import; give it its output path below.
    original = sys.argv
    with tempfile.TemporaryDirectory(prefix="nitride-checkpoint-") as folder:
        folder = pathlib.Path(folder)
        sys.argv = ["corpus", str(folder)]
        corpus = load_generator(ROOT / "scripts/json-bench/corpus.py", "json_corpus")
        sys.argv = original
        corpus.write_inputs([("64k", 64 << 10), ("256k", 256 << 10)] if args.smoke else
                            [("64k", 64 << 10), ("256k", 256 << 10), ("1m", 1 << 20), ("4m", 4 << 20), ("16m", 16 << 20)])
        rows = []
        repeats = 1 if args.smoke else 5

        def collect(case, path, executable, tail):
            if not executable.is_file():
                raise FileNotFoundError(f"build benchmark first: {executable}")
            run = [executable, *tail]
            oracle = None
            if tail[0] == "edit":
                oracle = measured([executable, "edit-check", *tail[1:]])
                if not oracle.get("oracle_checked") or len(oracle.get("edit_fingerprints", [])) != int(tail[-1]):
                    raise ValueError("missing JSON edit equivalence oracle")
            measured(run)  # Explicit unrecorded fresh process.
            samples = [measured(run) for _ in range(repeats)]
            if oracle:
                for sample in samples:
                    if sample.get("edit_fingerprints") != oracle["edit_fingerprints"]:
                        raise ValueError("timed JSON edit differs from fresh-parse oracle")
                    sample["equivalence_checked"] = True
            row = summarize(case, sha(path.read_bytes()), samples)
            if oracle:
                row["equivalence_oracle"] = oracle
            row["executable"] = {"path": str(executable), "sha256": sha(executable.read_bytes())}
            rows.append(row)
            print(f"{case}: {row['medians']}", file=sys.stderr, flush=True)

        if args.suite in ("csharp", "all"):
            for size in ([64 << 10] if args.smoke else [64 << 10, 256 << 10, 4 << 20]):
                base = csharp.generated(size)
                for pattern in ("valid", "one", "dense"):
                    path = folder / f"csharp-{size}-{pattern}.cs"
                    path.write_text(base if pattern == "valid" else recovery.damaged(base, pattern))
                    collect(path.stem, path, args.audit, ["strict" if pattern == "valid" else "recover", path])
                    if args.profile.is_file():
                        collect(path.stem + "-heap", path, args.profile,
                                ["--native-syntax", "--snapshot" if pattern == "valid" else "--snapshot-recover", "csharp4", path])
        if args.suite in ("json", "all"):
            for shape in ("records", "numbers", "strings"):
                for size in (["64k"] if args.smoke else ["64k", "1m", "16m"]):
                    path = folder / f"{shape}-{size}.json"
                    collect(path.stem, path, args.json, ["bench", path, "10"])
                    if args.heap.is_file():
                        collect(path.stem + "-heap", path, args.heap, ["heap", path])
        if args.suite in ("edits", "all"):
            for size in ([256 << 10] if args.smoke else [256 << 10, 4 << 20]):
                base = csharp.generated(size)
                for pattern in ("valid", "one"):
                    path = folder / f"csharp-edit-{size}-{pattern}.cs"
                    source = base if pattern == "valid" else recovery.damaged(base, "one")
                    path.write_text(source)
                    at = source.index("2", len(source) // 2)
                    edits = [{"range": [at, at + 1], "replacement": "3" if i % 2 == 0 else "2"} for i in range(10)]
                    config = folder / "edits.json"
                    config.write_text(json.dumps({"recover": pattern != "valid", "retain": 2, "validate": True, "edits": edits}))
                    collect(path.stem, path, args.profile, ["--native-syntax", "--edits", "csharp4", path, config])
                    for sample in rows[-1]["samples"]:
                        if not sample["cold"]["equivalent"] or not all(e["equivalent"] for e in sample["edits"]):
                            raise ValueError("edit differs from fresh parse")
                for shape in ("records", "numbers"):
                    label = "256k" if size == 256 << 10 else "4m"
                    path = folder / f"{shape}-{label}.json"
                    collect(path.stem + "-edits", path, args.json, ["edit", path, "10"])
        report = {
            "schema": 1, "source": identity, "variant": "portable-release-no-pgo",
            "complete_matrix": not args.smoke and args.suite == "all",
            "smoke": args.smoke, "suite": args.suite, "runs_per_case": repeats,
            "untimed_runs_per_case": 1, "host": platform.platform(), "machine": platform.machine(),
            "rustc": command(["rustc", "-Vv"]).decode(), "rows": rows,
            "cpu": command(["sysctl", "-n", "machdep.cpu.brand_string"]).decode().strip() if sys.platform == "darwin" else platform.processor(),
            "build_environment": {key: os.environ.get(key) for key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET")},
            "metric_definitions": {
                "process_wall_ns": "process launch, input reading, parsing, validation/export, reporting and destruction",
                "elapsed_ns": "uninstrumented harness snapshot parse only; validation/export excluded",
                "cold_ns": "JSON first parse in a fresh process",
                "warm_median_ns": "JSON ten later full parses in the same process",
                "peak_rss_bytes": "whole-process maximum RSS via macOS time -l; unavailable elsewhere",
                "heap": "requested System bytes; instrumented separately from parse timing",
                "language_analysis": "shared process storage; profiler excludes initialization from snapshot phase",
            },
            "unavailable": ["shipping PGO binaries", "fresh external engine comparisons",
                            "separate shared-language table byte count"],
        }
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
