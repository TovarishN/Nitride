"""Freeze/check actual Nitrogen CSharp4 syntax trees against Nitride's native parser.

Cases come from CSNitrogen's corpus (wrapped as its own oracle tests wrap them),
the vendored Mono corpus files, supplementary-plane identifiers, and 64
deterministic generated programs.
"""
import json
import pathlib
import subprocess
import sys
import tempfile
from test_reference import ROOT, run

CSNITROGEN = pathlib.Path.home() / "work/CSNitrogen/tests/Corpus"
NATIVE = ROOT / "target/release/nitride"


def wrap(mode, source):
    return {
        "type": "class C { " + source + " field; }",
        "expression": "class C { object a, b; object nameof(object x){return x;} object M(){return " + source + ";} }",
        "statement": "class C {void M(){" + source + "}}",
        "attribute": source + " class C{}",
        "parameter": "class C {void M" + source + "{}}",
    }.get(mode, source)


def cases():
    """Build the case list from CSNitrogen's corpus (run with --cases to refresh)."""
    rows = []
    for row in json.loads((CSNITROGEN / "source-grammar-cases.json").read_text()):
        rows.append(("grammar", wrap(row["mode"], row["source"])))
    for name in ["source-review-cases.json", "context-conformance-cases.json"]:
        rows += [(name.split("-")[0], row["source"]) for row in json.loads((CSNITROGEN / name).read_text())]
    for row in json.loads((CSNITROGEN / "attribute-conformance-cases.json").read_text()):
        source = row["source"]
        full = source if source.startswith("class") or " using " in source else source + " class C{}"
        rows.append(("attribute", full))
    specific = json.loads((CSNITROGEN / "CSharp4/source-specific-cases.json").read_text())
    for row in specific["cases"] if isinstance(specific, dict) else specific:
        rows += [("csharp4", row["source"]), ("csharp4-negative", row["negative"])]
    rows += [
        ("supplementary", "class C { int \U0001D465; int \\U0001D465x; }"),
        ("supplementary", "class \U0001D465 { }"),
    ]
    # Shapes from the final review: nesting, long chains, postfix composition.
    chain = "".join(f"if (x == {i}) return {i}; else " for i in range(200))
    rows += [
        ("nesting", "class C { object M() { return " + "(" * 30 + "1" + ")" * 30 + "; } }"),
        ("nesting", "class C { object M() { return " + "(int)(" * 20 + "1" + ")" * 20 + "; } }"),
        ("nesting", "class C { int M(int x) { " + chain + "return -1; } }"),
        ("nesting", "class C { void M() { " + "{ " * 60 + "M();" + " }" * 60 + " } }"),
        ("postfix", "class C { object M() { return f(x)(y)[z].w; } }"),
        ("postfix", "class C { object M() { return -a.b(c)[d] + e(f)[g].h * 2; } }"),
    ]
    seen, unique = set(), []
    for kind, source in rows:
        if source not in seen:
            seen.add(source)
            unique.append({"name": f"{kind}-{len(unique)}", "source": source})
    return unique


def generated(index):
    members = [
        "int F(int x) { return x * {i} + 1; }",
        "public string Name {{ get {{ return \"n{i}\"; }} set {{ }} }}",
        "static T Id<T>(T v) where T : class {{ return v; }}",
        "void Q() {{ var xs = from a in new[] {{ {i}, 2 }} where a > 1 select a * 2; }}",
        "void L() {{ System.Func<int, int> f = x => x + {i}; f({i}); }}",
        "[System.Obsolete(\"o{i}\")] void A() {{ }}",
        "unsafe void P(int* p) {{ *p = {i}; }}",
        "void S() {{ switch ({i}) {{ case 1: break; default: goto case 1; }} }}",
        "void T() {{ try {{ throw new System.Exception(); }} catch (System.Exception e) {{ }} finally {{ }} }}",
        "event System.EventHandler E{i};",
        "int this[int k] {{ get {{ return k + {i}; }} }}",
        "void N() {{ var o = new {{ A = {i}, B = \"b\" }}; int? n = null; object d = default(int); }}",
    ]
    body = " ".join(members[(index + k) % len(members)].replace("{i}", str(index)).replace("{{", "{").replace("}}", "}")
                    for k in range(1 + index % 4))
    kind = ["class", "struct", "class", "class"][index % 4]
    namespace = f"namespace N{index} {{ " if index % 3 == 0 else ""
    end = " }" if index % 3 == 0 else ""
    return f"using System; {namespace}{kind} C{index}<T> {{ {body} }}{end}"


def observe(source, folder, metadata):
    path = pathlib.Path(folder) / "input.cs"
    path.write_text(source)
    result = run("oracle-syntax", "csharp4", path, metadata)
    assert result.returncode == 0, result.stderr
    return json.loads(result.stdout)


def native(source, folder):
    path = pathlib.Path(folder) / "input.cs"
    path.write_text(source)
    result = subprocess.run([NATIVE, "--parse-csharp4", path], capture_output=True, text=True)
    return json.loads(result.stdout)


def same(actual, expected):
    # Diagnostic positions differ by design (generated vs Nitrogen recovery); acceptance must agree.
    if not expected["ok"] or not actual["ok"]:
        return actual["ok"] == expected["ok"]
    return actual["nodes"] == expected["nodes"]


def main():
    if "--cases" in sys.argv:
        corpus = sorted((ROOT / "languages/csharp4/corpus").rglob("*.cs"))
        rows = cases() + [{"name": f"corpus-{p.parent.name}-{p.stem}", "source": p.read_text()} for p in corpus]
        (ROOT / "fixtures/csharp4-cases.json").write_text(json.dumps(rows, indent=2, ensure_ascii=False) + "\n")
        print(f"Wrote {len(rows)} CSharp4 cases")
        return
    rows = json.loads((ROOT / "fixtures/csharp4-cases.json").read_text())
    with tempfile.TemporaryDirectory() as folder:
        metadata = pathlib.Path(folder) / "csharp4.json"
        prepared = subprocess.run([NATIVE, "prepare", ROOT / "languages/csharp4/project.json", "--output", metadata], capture_output=True, text=True)
        assert prepared.returncode == 0, prepared.stderr
        frozen = [{"name": r["name"], "source": r["source"], "observation": observe(r["source"], folder, metadata)} for r in rows]
        path = ROOT / "fixtures/csharp4-reference.json"
        if "--freeze" in sys.argv:
            path.write_text(json.dumps(frozen, indent=1, ensure_ascii=False) + "\n")
            print(f"Frozen {len(frozen)} CSharp4 observations ({sum(c['observation']['ok'] for c in frozen)} accepted)")
            return
        assert json.loads(path.read_text()) == frozen, "frozen CSharp4 reference changed"
        mismatches = [c["name"] for c in frozen if not same(native(c["source"], folder), c["observation"])]
        for index in range(64):
            source = generated(index)
            if not same(native(source, folder), observe(source, folder, metadata)):
                mismatches.append(f"generated-{index}")
        if mismatches:
            print(f"{len(mismatches)} mismatches: {mismatches[:20]}")
            sys.exit(1)
        print(f"{len(frozen)} CSharp4 cases and 64 generated programs match live Nitrogen")


if __name__ == "__main__":
    main()
