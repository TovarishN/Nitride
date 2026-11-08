"""Paired portable-release measurements; never build while this runs."""
import argparse
import json
import pathlib
import platform
import re
import statistics
import subprocess
import sys
import tempfile
from benchmark_checkpoint import ROOT, command, compare, measured, sha, source_identity, summarize
from csharp4_scaling import generated
from recovery_scaling import damaged
import importlib.util


def stats(executable, path):
    result = subprocess.run([str(executable), "stats", str(path)], capture_output=True, text=True, check=True)
    block = re.search(r"StorageStats\s*\{([^}]+)", result.stderr)[1]
    return {key: int(value) for key, value in re.findall(r"(\w+): (\d+)", block)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=pathlib.Path, required=True)
    parser.add_argument("--after", type=pathlib.Path, required=True)
    parser.add_argument("--before-heap", type=pathlib.Path, required=True)
    parser.add_argument("--after-heap", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--smoke", action="store_true")
    args = parser.parse_args()
    state = source_identity()
    report = {"source": state, "host": platform.platform(), "variant": "portable-release-no-pgo",
              "rustc": command(["rustc", "-Vv"]).decode(),
              "cpu": command(["sysctl", "-n", "machdep.cpu.brand_string"]).decode().strip() if sys.platform == "darwin" else platform.processor(),
              "build_flags": "opt-level=3, LTO, one codegen unit, panic=abort; inherited Rust flags cleared; no PGO/native CPU targeting",
              "baseline_layout_revision": "2e0211b",
              "source_control": "same isolated source copy and flags for both variants; before replaces only dependency-index source with the named revision", "runs": 1 if args.smoke else 5, "smoke": args.smoke, "rows": [],
              "definitions": {"timing": "parse/edit phase only; process wall includes launch/export/destruction",
                              "heap": "requested System bytes in a separate instrumented build; live_after includes first-use language analysis",
                              "storage": "retained snapshot storage; excludes owner-scoped language analysis",
                              "paired_order": "one untimed process per variant; alternate variant order across five fresh-process repetitions"}}
    with tempfile.TemporaryDirectory(prefix="nitride-storage-pair-") as folder:
        folder = pathlib.Path(folder)
        old_argv = sys.argv
        sys.argv = ["corpus", str(folder)]
        spec = importlib.util.spec_from_file_location("json_corpus", ROOT / "scripts/json-bench/corpus.py")
        corpus = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(corpus)
        sys.argv = old_argv
        sizes = [("64k", 64 << 10), ("256k", 256 << 10), ("1m", 1 << 20), ("4m", 4 << 20), ("16m", 16 << 20)]
        corpus.write_inputs(sizes if not args.smoke else sizes[:2])

        def pair(case, path, name, tail, heap=False):
            dirs = [args.before_heap, args.after_heap] if heap else [args.before, args.after]
            bins = [directory.resolve() / name for directory in dirs]
            verify_edits = name == "nitride-json-bench" and tail[0] == "edit"
            oracles = []
            if verify_edits:
                for binary in bins:
                    oracle = measured([binary, "edit-check", *tail[1:]])
                    if not oracle.get("oracle_checked") or len(oracle.get("edit_fingerprints", [])) != int(tail[-1]):
                        raise ValueError("missing JSON edit equivalence oracle")
                    oracles.append(oracle)
            for binary in bins:
                measured([binary, *tail])
            samples = [[], []]
            for run in range(report["runs"]):
                for variant in ([0, 1] if run % 2 == 0 else [1, 0]):
                    sample = measured([bins[variant], *tail])
                    if verify_edits:
                        if sample.get("edit_fingerprints") != oracles[variant]["edit_fingerprints"]:
                            raise ValueError("timed JSON edit differs from fresh-parse oracle")
                        sample["equivalence_checked"] = True
                    if "edits" in sample and isinstance(sample["edits"], list):
                        sample["median_edit_ns"] = statistics.median(e["phase"]["elapsed_ns"] for e in sample["edits"])
                        sample["initial_parse_ns"] = sample["cold"]["phase"]["elapsed_ns"]
                    samples[variant].append(sample)
            rows = [summarize(case, sha(path.read_bytes()), group) for group in samples]
            if verify_edits:
                for row in rows:
                    row["requires_equivalence"] = True
            row = {"case": case, "before": rows[0], "after": rows[1], "comparison": compare(*rows),
                   "executables": [{"path": str(binary), "sha256": sha(binary.read_bytes())} for binary in bins]}
            if verify_edits:
                row["equivalence_oracles"] = oracles
            if name == "nitride-json-bench" and not heap and tail[0] == "bench":
                row["storage"] = {"before": stats(bins[0], path), "after": stats(bins[1], path)}
            report["rows"].append(row)
            # Keep a recoverable report after each case, explicitly incomplete.
            report["complete"] = False
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(report, indent=2) + "\n")
            print(case, row["comparison"]["ratios"], file=sys.stderr, flush=True)

        for size in ([64 << 10] if args.smoke else [64 << 10, 256 << 10, 4 << 20]):
            base = generated(size)
            for pattern in ("valid", "one", "dense"):
                path = folder / f"csharp-{size}-{pattern}.cs"
                path.write_text(base if pattern == "valid" else damaged(base, pattern))
                pair(path.stem, path, "nitride-compiler-audit", ["strict" if pattern == "valid" else "recover", path])
        for shape in ("records", "numbers", "strings"):
            for size in (["64k"] if args.smoke else ["64k", "1m", "16m"]):
                path = folder / f"{shape}-{size}.json"
                pair(path.stem, path, "nitride-json-bench", ["bench", path, "10"])
                if size != "64k":
                    pair(path.stem + "-heap", path, "nitride-json-bench", ["heap", path], heap=True)
        for size in ([256 << 10] if args.smoke else [256 << 10, 4 << 20]):
            base = generated(size)
            for pattern in ("valid", "one"):
                path = folder / f"csharp-edit-{size}-{pattern}.cs"
                source = base if pattern == "valid" else damaged(base, "one")
                path.write_text(source)
                at = source.index("2", len(source) // 2)
                edits = [{"range": [at, at + 1], "replacement": "3" if i % 2 == 0 else "2"} for i in range(10)]
                config = folder / "edits.json"
                config.write_text(json.dumps({"recover": pattern != "valid", "retain": 2, "validate": False, "edits": edits}))
                pair(path.stem, path, "nitride-profile", ["--native-syntax", "--edits", "csharp4", path, config])
                # Independent full-parse oracle outside all measured samples.
                config.write_text(json.dumps({"recover": pattern != "valid", "retain": 2, "validate": True, "edits": edits[:2]}))
                checks = []
                for directory in (args.before, args.after):
                    sample = measured([directory.resolve() / "nitride-profile", "--native-syntax", "--edits", "csharp4", path, config])
                    assert sample["cold"]["equivalent"] and all(e["equivalent"] for e in sample["edits"])
                    checks.append({"cold": True, "edits": len(sample["edits"])})
                report["rows"][-1]["oracle_checks"] = checks
            for shape in ("records", "numbers"):
                path = folder / f"{shape}-{'256k' if size == 256 << 10 else '4m'}.json"
                pair(path.stem + "-edits", path, "nitride-json-bench", ["edit", path, "10"])
        report["complete"] = not args.smoke
        args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
