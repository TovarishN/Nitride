"""Fast strict path milestone 0: aggregate parser decision census and decide go/no-go.

Build a census-enabled profiler first:

    cargo build --release -p nitride-cli --bin nitride-profile --features census
    python3 -I scripts/parser_census.py --profile target/release/nitride-profile \
        --report target/reports/decision-census.json

CSharp4 runs the 4 MiB generated valid input and every Mono corpus file and is
judged against the thresholds below. Every other profiler language runs its
`languages/<dir>/sample.*` file and is reported only. Every parse is strict
and fully interpreted, the language server's path.
"""
import argparse
import json
import pathlib
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from compare_parser_builds import inputs  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Go/no-go thresholds (the census plan's Global Constraints).
# A decision counts as decided when at most one alternative or candidate ran.
G1_CHOICE_SINGLE = 0.90
G2_PRATT_SINGLE = 0.90
G3_CONSUMED_PER_KB = 20.0
PREDICATES_PER_KB = 5.0
SAMPLES = {
    "portable": "portable/sample.portable",
    "power": "power/sample.powercalc",
    "composition": "composition/sample.composed",
    "xml": "xml/sample.xml",
    "namespaces": "namespaces/sample.namespaces",
    "inference": "inference/sample.inferred",
    "calls": "calls/sample.calls",
    "functions": "functions/sample.functions",
    "dates": "dates/sample.dates",
    "rewrites": "rewrites/sample.rewrites",
    "context": "context/sample.context",
    "grammar": "grammar/sample.nitridegrammar",
}


def aggregate(reports):
    """Totals, shares and per-1,000-byte rates over census reports."""
    source = sum(r["source_bytes"] for r in reports)
    kb = max(source, 1) / 1000
    decisions = [d for r in reports for d in r["decisions"]]

    def share(kinds):
        chosen = [d for d in decisions if d["kind"] in kinds]
        entries = sum(d["entries"] for d in chosen)
        # Decided by lookahead: at most one alternative or candidate ran.
        decided = sum(d["single"] + d.get("empty", 0) for d in chosen)
        return entries, (decided / entries if entries else 1.0)

    choice_entries, choice_share = share({"choice"})
    pratt_entries, pratt_share = share({"pratt-prefix", "pratt-infix"})
    consumed = sum(d["consumed_failures"] for d in decisions) + sum(
        r["repeat_consumed_exits"] for r in reports)
    merged = {}
    for d in decisions:
        key = (d["rule_name"], d["kind"], d["ordinal"])
        row = merged.setdefault(key, {"rule_name": d["rule_name"], "kind": d["kind"], "ordinal": d["ordinal"],
                                      "entries": 0, "single": 0, "attempts": 0, "consumed_failures": 0})
        for field in ("entries", "single", "attempts", "consumed_failures"):
            row[field] += d[field]
    top = sorted(merged.values(), key=lambda row: row["attempts"] - row["single"], reverse=True)[:20]
    return {
        "source_bytes": source,
        "choice_entries": choice_entries,
        "choice_single_share": choice_share,
        "pratt_entries": pratt_entries,
        "pratt_single_share": pratt_share,
        "consumed_failures_per_kb": consumed / kb,
        "predicates_per_kb": sum(r["predicate_evaluations"] for r in reports) / kb,
        "contexts_per_kb": sum(r["context_entries"] for r in reports) / kb,
        "top": top,
    }


def verdict(summary):
    """The milestone-0 go/no-go for one language's summary."""
    g1 = summary["choice_single_share"] >= G1_CHOICE_SINGLE
    g2 = summary["pratt_single_share"] >= G2_PRATT_SINGLE
    g3 = summary["consumed_failures_per_kb"] <= G3_CONSUMED_PER_KB
    return {"go": g1 and g2 and g3, "g1": g1, "g2": g2, "g3": g3,
            "predicate_support": summary["predicates_per_kb"] > PREDICATES_PER_KB}


def census(profile, language, path):
    out = subprocess.run([str(profile), "--census", language, str(path)], capture_output=True, text=True)
    if out.returncode != 0:
        return {"language": language, "path": str(path), "error": out.stderr.strip()[-300:]}
    report = json.loads(out.stdout)
    report["path"] = str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else path.name
    return report


def describe(name, summary):
    print(f"{name}: {summary['source_bytes']} bytes; choices {summary['choice_entries']} "
          f"single {summary['choice_single_share']:.3f}; Pratt {summary['pratt_entries']} "
          f"single {summary['pratt_single_share']:.3f}; consumed failures "
          f"{summary['consumed_failures_per_kb']:.2f}/KB; predicates {summary['predicates_per_kb']:.2f}/KB; "
          f"contexts {summary['contexts_per_kb']:.2f}/KB")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--profile", type=pathlib.Path, required=True)
    parser.add_argument("--report", type=pathlib.Path)
    args = parser.parse_args()
    folder = ROOT / "target/parser-compare"
    folder.mkdir(parents=True, exist_ok=True)
    inputs(folder)
    csharp = [folder / "4m-valid.cs"] + sorted((ROOT / "languages/csharp4/corpus").rglob("*.cs"))
    runs = {"csharp4": [census(args.profile, "csharp4", path) for path in csharp]}
    for language, sample in SAMPLES.items():
        runs[language] = [census(args.profile, language, ROOT / "languages" / sample)]
    result = {"languages": {}, "errors": []}
    for language, reports in runs.items():
        good = [r for r in reports if "error" not in r and r.get("diagnostics") == 0]
        result["errors"] += [r for r in reports if r not in good]
        if not good:
            continue
        summary = aggregate(good)
        result["languages"][language] = {"summary": summary, "files": len(good)}
        describe(language, summary)
    csharp_summary = result["languages"]["csharp4"]["summary"]
    result["verdict"] = verdict(csharp_summary)
    result["thresholds"] = {"g1_choice_single": G1_CHOICE_SINGLE, "g2_pratt_single": G2_PRATT_SINGLE,
                            "g3_consumed_per_kb": G3_CONSUMED_PER_KB, "predicates_per_kb": PREDICATES_PER_KB}
    result["reports"] = runs
    print("verdict:", json.dumps(result["verdict"]))
    for error in result["errors"]:
        print("not counted:", error.get("language"), error.get("path"), error.get("error", "diagnostics"))
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(result, indent=1) + "\n")


if __name__ == "__main__":
    main()
