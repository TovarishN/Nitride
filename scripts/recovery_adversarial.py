"""Frozen reviewed errors, every-token small inputs, and seeded multi-error edits."""
import argparse
from dataclasses import asdict
import json
import random
from recovery_corpus import ROOT, SEED, Edit, map_span, token_spans
from recovery_compare import Runner, native_items, coverage_complete

SOURCES = {
    "csharp4": "class C { int A() { return 1; } int B() { return 2; } }",
    "portable": "let x : Scalar = 1; x + 1; 2 + 3;",
    "power": "1 + 2; 3 * 4;",
    "composition": "doc let x = -1; (name) end",
    "xml": '<root><a x="1"/><b/></root>',
    "namespaces": "let x : Scalar = 1; either(x) + 1; 2 + 3;",
    "inference": "let x = 1; value(x) + 1; 2 + 3;",
    "calls": "let x = 1; sum(x,1); neg(2);",
    "functions": "def bad(x: Scalar) = add(x, 1); def good(x: Scalar) = add(x, 2); call good(3);",
    "dates": "let x = 2026-01-01; x + 1 days; today;",
    "rewrites": "let x = 1 days; twice(x); 2 fortnights;",
    "grammar": "syntax module M { token N = ['0'..'9']+; syntax P = N; }",
}


def freeze(path, native, roslyn):
    baselines, variants, inapplicable = [], [], []
    for profile, source in SOURCES.items():
        assert native.query(profile, source)["ok"], profile
        baselines.append({"id": profile, "profile": profile, "source": source})
        raw = source.encode()
        tokens = ([t["span"] for t in roslyn.query(profile,source)["tokens"]]
                  if profile == "csharp4" else token_spans(source))
        for i, (a,b) in enumerate(tokens):
            edits = {"delete":Edit(a,b,""),"insert":Edit(a,a,"@ "),"duplicate":Edit(a,b,raw[a:b].decode()*2),"truncate":Edit(a,len(raw),"")}
            if i+1 < len(tokens):
                c,d = tokens[i+1]
                edits["swap"] = Edit(a,d,(raw[c:d]+raw[b:c]+raw[a:b]).decode())
            else:
                inapplicable.append({"profile":profile,"token":i,"operation":"swap"})
            if raw[a:b] in [b')',b']',b'}']:
                edits["closer"] = Edit(a,b,"")
            else:
                inapplicable.append({"profile":profile,"token":i,"operation":"closer"})
            for operation, edit in edits.items():
                variants.append({"baseline":profile,"group":"every-token","token":i,"operation":operation,"edits":[asdict(edit)],"source":edit.apply(source)})
        rng = random.Random(f"{SEED}:{profile}:multiple")
        for index in range(16):
            current, edits = source, []
            for _ in range(3):
                spans = token_spans(current)
                if not spans:
                    break
                a,b = rng.choice(spans)
                edit = rng.choice([Edit(a,b,""),Edit(a,a,"@ "),Edit(a,b,current.encode()[a:b].decode()*2)])
                current = edit.apply(current)
                edits.append(asdict(edit))
            variants.append({"baseline":profile,"group":"multiple","index":index,"edits":edits,"source":current})
    # Reviewed isolated errors: one missing delimiter/value, with independent siblings.
    for profile, needle, replacement in [
        ("csharp4","return 1;","return 1"), ("portable","= 1;","= ;"),
        ("inference","= 1;","= ;"), ("namespaces","= 1;","= ;"),
        ("calls","= 1;","= ;"), ("functions","add(x, 1)","add(x, )"),
        ("composition","-1;","-1"), ("grammar","N;","N"),
    ]:
        source = SOURCES[profile]
        # Store only the differing bytes so unaffected adjacent syntax stays eligible.
        a = source.index(needle)
        left = 0
        while left < min(len(needle),len(replacement)) and needle[left] == replacement[left]: left += 1
        right = 0
        while right < min(len(needle),len(replacement))-left and needle[-right-1] == replacement[-right-1]: right += 1
        edit = Edit(a+left,a+len(needle)-right,replacement[left:len(replacement)-right if right else None])
        variants.append({"baseline":profile,"group":"reviewed","edits":[asdict(edit)],"source":edit.apply(source)})
    path.write_text(json.dumps({"seed":SEED,"baselines":baselines,"variants":variants,"inapplicable":inapplicable},ensure_ascii=False,indent=2)+"\n")


def measure(manifest, native):
    originals = {b["id"]:native_items(b["source"],native.query(b["profile"],b["source"])) for b in manifest["baselines"]}
    rows, failures = [], []
    for index, variant in enumerate(manifest["variants"]):
        profile, source = variant["baseline"], variant["source"]
        a,b = native.query(profile,source),native.query(profile,source)
        a.pop("parse_ns"); b.pop("parse_ns")
        after = {(tuple(n["span"]),n["shape"]) for n in native_items(source,a)}
        eligible = preserved = 0
        for item in originals[profile]:
            span = item["span"]
            for edit in variant["edits"]:
                span = map_span(span,Edit(**edit))
                if span is None: break
            if span is not None:
                eligible += 1
                preserved += (tuple(span),item["shape"]) in after
        syntax = sum(d["category"] == "syntax" for d in a["diagnostics"])
        complete, deterministic = coverage_complete(a,source), a == b
        row = {"index":index,"profile":profile,"group":variant["group"],"eligible":eligible,"preserved":preserved,"syntax_diagnostics":syntax,"coverage":complete,"deterministic":deterministic,"stats":a["stats"]}
        rows.append(row)
        if not complete or not deterministic:
            failures.append(f"{index}: coverage/determinism")
        if variant["group"] == "reviewed" and (syntax != 1 or preserved != eligible):
            failures.append(f"{index}: reviewed isolated error: {syntax} diagnostics, {preserved}/{eligible} siblings")
    return {"seed":manifest["seed"],"cases":rows,"failures":failures}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--freeze",action="store_true")
    args = parser.parse_args()
    path = ROOT/"fixtures/recovery/adversarial.json"
    native = Runner([str(ROOT/"target/release/nitride"),"--recover-batch"])
    roslyn = Runner(["dotnet",str(ROOT/"tools/Nitride.RecoveryReference/bin/Debug/net10.0/Nitride.RecoveryReference.dll")])
    try:
        if args.freeze: freeze(path,native,roslyn)
        report = measure(json.loads(path.read_text()),native)
        (ROOT/"docs/stage14-recovery-adversarial.json").write_text(json.dumps(report,indent=2)+"\n")
        print(json.dumps({"cases":len(report["cases"]),"failures":report["failures"]},indent=2))
        return bool(report["failures"])
    finally:
        native.close(); roslyn.close()


if __name__ == "__main__":
    raise SystemExit(main())
