"""Compare frozen damage against each parser's own original tree; never rewrite inputs implicitly."""
import argparse
from collections import defaultdict
from dataclasses import asdict
import hashlib
import json
import pathlib
import subprocess
import sys
from recovery_corpus import ROOT, SEED, Edit, damages, frozen_baselines, generated_baselines, map_span, preservation, token_spans, semantic_items


class Runner:
    def __init__(self, command):
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, encoding="utf-8", bufsize=1)

    def query(self, profile, source):
        self.process.stdin.write(json.dumps({"profile":profile,"source":source}, ensure_ascii=False) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError(f"oracle stopped: {self.process.poll()}")
        return json.loads(line)

    def close(self):
        self.process.stdin.close()
        self.process.wait(timeout=60)
        if self.process.returncode:
            raise RuntimeError(f"oracle exited {self.process.returncode}")


def native_items(source, observation):
    raw = source.encode("utf-8")
    shapes = {}
    result = []
    # All native trees are in postorder. Hash labels, structure and lexical
    # spellings, not positions or parser-specific identity numbers.
    for node in observation["nodes"]:
        value = [node["rule"], node["missing"], raw[node["span"][0]:node["span"][1]].decode("utf-8") if node["lexical"] else [(e["label"], shapes[e["node"]]) for e in node["children"]]]
        shapes[node["id"]] = hashlib.sha256(json.dumps(value, ensure_ascii=False).encode()).hexdigest()
        csharp_unit = node["rule"].rsplit(".",1)[-1] in {"MemberDeclaration","StandardStatement","ClassTypeDeclaration","StructTypeDeclaration","InterfaceTypeDeclaration","EnumTypeDeclaration","ClassDeclaration","StructDeclaration","InterfaceDeclaration","EnumDeclaration","NamespaceDeclaration"}
        syntax_unit = node["rule"].rsplit(".",1)[-1] in {"Element","Empty"}
        if node.get("item") or csharp_unit or syntax_unit:
            result.append({"span":node["span"],"shape":shapes[node["id"]],"kind":node["rule"]})
    return result


def coverage_complete(observation, source):
    raw = source.encode("utf-8")
    at = 0
    for part in observation["coverage"]:
        a, b = part["span"]
        if a != at or b < a or b > len(raw):
            return False
        try:
            raw[a:b].decode("utf-8")
        except UnicodeDecodeError:
            return False
        at = b
    return at == len(raw)


def freeze(path, native, roslyn):
    baselines, variants, skipped, excluded = [], [], [], []
    for baseline in [*frozen_baselines(),*generated_baselines()]:
        observation = native.query(baseline["profile"],baseline["source"])
        baseline["native_baseline_diagnostics"] = observation["diagnostics"]
        if ":generated:" in baseline["id"] and not observation["ok"]:
            excluded.append({"id":baseline["id"],"profile":baseline["profile"],"source":baseline["source"],"diagnostics":observation["diagnostics"],"reason":"existing generator intentionally includes invalid programs"})
            continue
        baseline["sha256"] = hashlib.sha256(baseline["source"].encode()).hexdigest()
        baselines.append(baseline)
        if baseline["profile"] == "csharp4":
            reference = roslyn.query("csharp4",baseline["source"])
            tokens = [t["span"] for t in reference["tokens"]]
        else:
            tokens = token_spans(baseline["source"])
        for kind, edit in damages(baseline["source"],tokens,baseline["id"]):
            if edit is None:
                skipped.append({"baseline":baseline["id"],"operation":kind,"reason":"no eligible source token/boundary"})
            else:
                variants.append({"baseline":baseline["id"],"operation":kind,"edit":asdict(edit),"source":edit.apply(baseline["source"])})
    manifest = {"seed":SEED,"baselines":baselines,"variants":variants,"inapplicable":skipped,"invalid_generated":excluded}
    path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+"\n")
    print(f"froze {len(baselines)} baselines, {len(variants)} edits, {len(skipped)} inapplicable operations",flush=True)


def measure(manifest, native, roslyn):
    originals, reference_originals, semantic_originals = {}, {}, {}
    cases = []
    for baseline in manifest["baselines"]:
        profile, source = baseline["profile"], baseline["source"]
        obs = native.query(profile,source)
        originals[baseline["id"]] = native_items(source,obs)
        semantic_originals[baseline["id"]] = semantic_items(obs)
        if profile == "csharp4":
            reference_originals[baseline["id"]] = roslyn.query(profile,source)
    by_id = {b["id"]:b for b in manifest["baselines"]}
    for index, variant in enumerate(manifest["variants"]):
        baseline = by_id[variant["baseline"]]
        profile, source = baseline["profile"], variant["source"]
        edit = Edit(**variant["edit"])
        obs = native.query(profile,source)
        before = originals[baseline["id"]]
        after = native_items(source,obs)
        native_score = preservation(before,after,edit)
        diagnostic_spans = [d["span"] for d in obs["diagnostics"] if d["category"] == "syntax"]
        error_end = edit.start + len(edit.replacement.encode("utf-8"))
        distance = min((max(edit.start-b,a-error_end,0) for a,b in diagnostic_spans),default=None)
        row = {"baseline":baseline["id"],"profile":profile,"operation":variant["operation"],"edit":variant["edit"],"native":native_score,"semantic":preservation(semantic_originals[baseline["id"]],semantic_items(obs,edit),edit),"native_diagnostics":len(diagnostic_spans),"native_total_diagnostics":len(obs["diagnostics"]),"diagnostic_distance":distance,"coverage":coverage_complete(obs,source),"parse_ns":obs["parse_ns"],"stats":obs["stats"]}
        if profile == "csharp4":
            reference = roslyn.query(profile,source)
            original_ref = reference_originals[baseline["id"]]
            common = {tuple(n["span"]) for n in before} & {tuple(n["span"]) for n in original_ref["items"]}
            paired_native = preservation([n for n in before if tuple(n["span"]) in common],after,edit)
            paired_ref = preservation([n for n in original_ref["items"] if tuple(n["span"]) in common],reference["items"],edit)
            row.update({"roslyn":preservation(original_ref["items"],reference["items"],edit),"paired_native":paired_native,"paired_roslyn":paired_ref,"roslyn_diagnostics":len(reference["diagnostics"]),"roslyn_baseline_diagnostics":len(original_ref["diagnostics"]),"roslyn_parse_ns":reference["parse_ns"],"roslyn_version":reference["version"]})
        skipped = [r["span"] for r in obs["repairs"] if r["kind"] == "skipped"]
        row["skipped_bytes"] = sum(b-a for a,b in skipped)
        row["skipped_extent"] = [min(a for a,b in skipped),max(b for a,b in skipped)] if skipped else None
        cases.append(row)
        if index % 100 == 0:
            print(f"measured {index+1}/{len(manifest['variants'])}",flush=True)
    totals = defaultdict(lambda:defaultdict(int))
    for row in cases:
        group = totals[f"{row['profile']}:{row['operation']}"]
        group["cases"] += 1
        group["coverage_failures"] += not row["coverage"]
        group["fallbacks"] += row["stats"]["fallback_count"]
        group["native_diagnostics"] += row["native_diagnostics"]
        group["native_total_diagnostics"] += row["native_total_diagnostics"]
        for engine in ["native","semantic","roslyn","paired_native","paired_roslyn"]:
            if engine in row:
                group[engine+"_eligible"] += row[engine]["eligible"]
                group[engine+"_preserved"] += row[engine]["preserved"]
        if "roslyn_diagnostics" in row:
            group["roslyn_diagnostics"] += row["roslyn_diagnostics"]
    failures = quality_failures(totals)
    return {"native_sha256":hashlib.sha256((ROOT/"target/release/nitride").read_bytes()).hexdigest(),"seed":manifest["seed"],"baseline_diagnostics":[{"id":b["id"],"diagnostics":b["native_baseline_diagnostics"]} for b in manifest["baselines"] if b["native_baseline_diagnostics"]],"invalid_generated":len(manifest.get("invalid_generated",[])),"baselines":len(manifest["baselines"]),"variants":len(cases),"groups":dict(totals),"failures":failures,"cases":cases}


def quality_failures(totals):
    failures = []
    for name, group in totals.items():
        if group["coverage_failures"]:
            failures.append(f"{name}: source coverage failure")
        if name.startswith("csharp4:"):
            if group["paired_native_preserved"] < group["paired_roslyn_preserved"]:
                failures.append(f"{name}: preserved {group['paired_native_preserved']}/{group['paired_native_eligible']}, Roslyn {group['paired_roslyn_preserved']}/{group['paired_roslyn_eligible']}")
            if group["native_total_diagnostics"] > group["roslyn_diagnostics"]:
                failures.append(f"{name}: {group['native_total_diagnostics']} diagnostics, Roslyn {group['roslyn_diagnostics']}")
    return failures


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--freeze",action="store_true")
    parser.add_argument("--manifest",type=pathlib.Path,default=ROOT/"fixtures/recovery/manifest.json")
    parser.add_argument("--report",type=pathlib.Path,default=ROOT/"docs/stage14-recovery-quality.json")
    args = parser.parse_args()
    native = Runner([str(ROOT/"target/release/nitride"),"--recover-batch"])
    roslyn = Runner(["dotnet",str(ROOT/"tools/Nitride.RecoveryReference/bin/Debug/net10.0/Nitride.RecoveryReference.dll")])
    try:
        if args.freeze:
            freeze(args.manifest,native,roslyn)
        report = measure(json.loads(args.manifest.read_text()),native,roslyn)
        args.report.write_text(json.dumps(report,indent=2)+"\n")
        print(json.dumps({"variants":report["variants"],"failures":report["failures"]},indent=2))
        return bool(report["failures"])
    finally:
        native.close()
        roslyn.close()


if __name__ == "__main__":
    sys.exit(main())
