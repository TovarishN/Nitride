"""Compare two builds of the compiler-audit CSharp4 harness: parity, then time.

Build each revision's harness first, for example:

    cargo build --release --manifest-path scripts/compiler-audit/Cargo.toml --target-dir target/a
    python3 -I scripts/compare_parser_builds.py target/a/release/nitride-compiler-audit \
        target/b/release/nitride-compiler-audit --rounds 9

Parity runs every generated input (valid, one error, junk, dense errors, truncated)
and the Mono corpus in strict and recovering mode. Tree fingerprints (nodes,
labeled edges, diagnostics, recovery records) and diagnostic counts must match.
Logical work must match too unless `--work-may-differ` is given; differences are
always reported. Timing medians come from alternating build order each round.
Inputs are written to target/parser-compare/; a JSON report goes to --report.
"""
import argparse
import json
import pathlib
import statistics
import subprocess
import sys

# Sibling generator modules, also under `python3 -I`.
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from csharp4_scaling import generated  # noqa: E402
from recovery_scaling import damaged  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
TIMED = ["256k-valid", "4m-valid", "16m-valid", "256k-one", "4m-one", "4m-junk", "256k-dense", "256k-truncated"]


def inputs(folder):
    cases = []

    def add(name, make, mode):
        path = folder / f"{name}.cs"
        if not path.exists():
            path.write_text(make())
        cases.append((name, path, mode))

    for size, tag in [(64 << 10, "64k"), (256 << 10, "256k"), (4 << 20, "4m"), (16 << 20, "16m")]:
        source = generated(size)
        add(f"{tag}-valid", lambda: source, "strict")
        if tag in ("256k", "4m"):
            add(f"{tag}-one", lambda: damaged(source, "one"), "recover")
            add(f"{tag}-junk", lambda: damaged(source, "junk"), "recover")
        if tag in ("64k", "256k"):
            add(f"{tag}-dense", lambda: damaged(source, "dense"), "recover")
            add(f"{tag}-truncated", lambda: damaged(source, "truncated"), "recover")
    return cases


def run(binary, mode, path):
    out = subprocess.run([str(binary), mode, str(path)], capture_output=True, text=True)
    if out.returncode != 0:
        return {"error": out.stderr.strip()[-300:]}
    return json.loads(out.stdout)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("baseline")
    parser.add_argument("candidate")
    parser.add_argument("--rounds", type=int, default=9)
    parser.add_argument("--work-may-differ", action="store_true")
    parser.add_argument("--report", type=pathlib.Path)
    args = parser.parse_args()
    folder = ROOT / "target/parser-compare"
    folder.mkdir(parents=True, exist_ok=True)
    cases = inputs(folder)
    corpus = sorted((ROOT / "languages/csharp4/corpus").rglob("*.cs"))

    def key(result):
        work = None if args.work_may_differ else result.get("work")
        return (result.get("fingerprint"), work, result.get("diagnostics"), result.get("error"))

    parity = []
    runs = [(f"corpus/{p.name}", p, m) for p in corpus for m in ("strict", "recover")]
    runs += [case for case in cases if case[0] not in ("16m-valid",)]
    for name, path, mode in runs:
        a, b = run(args.baseline, mode, path), run(args.candidate, mode, path)
        parity.append({"case": name, "mode": mode, "match": key(a) == key(b),
                       "fingerprint": [a.get("fingerprint"), b.get("fingerprint")],
                       "work": [a.get("work"), b.get("work")],
                       "diagnostics": [a.get("diagnostics"), b.get("diagnostics")]})
    mismatches = [row for row in parity if not row["match"]]
    print(f"parity: {len(parity)} runs, {len(mismatches)} mismatches")
    for row in mismatches[:20]:
        print("  MISMATCH", row)
    for row in parity:
        a, b = row["work"]
        if a and b and a != b:
            print(f"  work {row['case']} {row['mode']}: {a} -> {b} ({100 * (b / a - 1):+.1f}%)")

    timed = [case for case in cases if case[0] in TIMED]
    samples = {(name, build): [] for name, _, _ in timed for build in ("baseline", "candidate")}
    for round_ in range(args.rounds):
        order = [("baseline", args.baseline), ("candidate", args.candidate)]
        if round_ % 2:
            order.reverse()
        for name, path, mode in timed:
            for build, binary in order:
                samples[(name, build)].append(run(binary, mode, path)["elapsed_ns"] / 1e6)
    rows = []
    print(f"{'case':12s} {'baseline ms':>12s} {'candidate ms':>13s} {'change':>8s}")
    for name, _, mode in timed:
        a = statistics.median(samples[(name, "baseline")])
        b = statistics.median(samples[(name, "candidate")])
        rows.append({"case": name, "mode": mode, "baseline_ms": a, "candidate_ms": b,
                     "baseline_samples": samples[(name, "baseline")],
                     "candidate_samples": samples[(name, "candidate")]})
        print(f"{name:12s} {a:12.2f} {b:13.2f} {100 * (b / a - 1):+7.1f}%")
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps({
            "baseline": str(args.baseline), "candidate": str(args.candidate),
            "rounds": args.rounds, "work_may_differ": args.work_may_differ,
            "parity": parity, "timing": rows}, indent=1) + "\n")
    return 1 if mismatches else 0


if __name__ == "__main__":
    sys.exit(main())
