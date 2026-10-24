"""Compare Rust preparation with the live .NET exporter, never a frozen fallback."""
import json
import pathlib
import subprocess
import tempfile
from test_reference import ROOT, run


def compare(project, tmp, index):
    reference = tmp / f"reference-{index}.json"
    result = run("export-project", project, reference)
    assert result.returncode == 0, result.stderr
    native = subprocess.run([ROOT / "target/release/nitride", "prepare", project], capture_output=True, text=True)
    assert native.returncode == 0, native.stderr
    actual, expected = json.loads(native.stdout), json.loads(reference.read_text())
    assert actual == expected, f"preparation mismatch for {project}"


def main():
    with tempfile.TemporaryDirectory() as folder:
        tmp = pathlib.Path(folder)
        for index, profile in enumerate(["portable", "power", "composition", "xml", "namespaces", "inference", "calls", "functions", "dates", "rewrites", "grammar"]):
            compare(ROOT / "languages" / profile / "project.json", tmp, index)
        for index in range(48):
            project = tmp / f"case-{index}"
            project.mkdir()
            # Deliberately exercise default .NET hashing escapes and UTF-16 pairs.
            comment = '// " & < > + ` café 😀 \u007f\u0085\u2028\n'
            char = [r"'a'", r"'\u0061'", r"'\n'", r"'\''"][index % 4]
            base = f'''{comment}syntax module Base{index} {{
 token N = ['0'..'9']+;
 token C = [{char} 'b'..'z'];
 syntax Pair = "(" N ")";
 extensible syntax E {{ | Num = N | Group = "(" E ")" }}
}}'''
            extension = f'''syntax module Uses{index} {{
 using Base{index};
 syntax Start = Entries:(Pair / E)*;
 extend syntax Base{index}.E {{
 | Neg = "-" E precedence 30
 | Add = Left:E "+" Right:E precedence {10 + index % 20} {"right" if index % 2 else "left"}
 }}
 token Comment = "#" (!"!" .)* "!";
}}'''
            (project / "Base.ngr").write_text(base)
            (project / "Uses.ngr").write_text(extension)
            (project / "catalog.json").write_text('{"types":[],"operations":[],"statements":{}}')
            manifest = {"mode":"syntax", "start":f"Uses{index}.Start", "grammars":["Base.ngr", "Uses.ngr"], "catalog":"catalog.json"}
            (project / "project.json").write_text(json.dumps(manifest))
            compare(project / "project.json", tmp, index + 6)
        for index in range(16):
            project = tmp / f"typed-{index}"
            project.mkdir()
            source = (ROOT / "languages/portable/Portable.ngr").read_text()
            source = (f"// typed variation {index} café 😀\n" + source).replace("Number", "Digits")
            source = source.replace("precedence 10 left", f"precedence {index + 6} {'right' if index % 2 else 'left'}")
            (project / "Portable.ngr").write_text(source)
            (project / "catalog.json").write_bytes((ROOT / "languages/portable/catalog.json").read_bytes())
            (project / "project.json").write_text(json.dumps({"mode":"typed","start":"Portable.Program","grammars":["Portable.ngr"],"catalog":"catalog.json"}))
            compare(project / "project.json", tmp, index + 63)
        for index in range(4):
            project = tmp / f"separated-{index}";project.mkdir()
            separator = [",", "|"][index % 2]; suffix = "*" if index < 2 else "+"
            source = f'syntax module List {{ syntax Start = Items:(Item; "{separator}"){suffix}; syntax Item = "a"; }}'
            (project / "List.ngr").write_text(source)
            (project / "catalog.json").write_text('{"types":[],"operations":[],"statements":{}}')
            (project / "project.json").write_text(json.dumps(dict(mode="syntax",start="List.Start",grammars=["List.ngr"],catalog="catalog.json")))
            compare(project / "project.json",tmp,index+100)
        for index, label in enumerate(["Value", "Spelling", "text", "texts"]):
            project = tmp / f"dates-{index}";project.mkdir()
            source = (ROOT / "languages/dates/Dates.ngr").read_text().replace("Value:Date lowers Dates.ParseDate(text Value)", f"{label}:Date lowers Dates.ParseDate(text {label})")
            catalog = json.loads((ROOT / "languages/dates/catalog.json").read_text())
            catalog["checks"]["Dates.Day"][0]["field"] = label
            (project / "Dates.ngr").write_text(source)
            (project / "catalog.json").write_text(json.dumps(catalog))
            (project / "project.json").write_text(json.dumps(dict(mode="typed",start="Dates.Program",grammars=["Dates.ngr"],catalog="catalog.json")))
            compare(project / "project.json",tmp,index+110)
        rewrite_variants = [
            ("FortnightDuration(Count)", "Rewrites.FortnightDuration(Count)"),
            ("<[ $Count * 14 days ]>", "<[$Count*14 days]>"),
            ("| _ => <[ $Value ]>;", "| <[ 0 ]> => <[ 1 ]>;\n    | _ => <[ $Value ]>;"),
            ("<[ today + $Amount days ]>", "<[\n      today + $Amount days // comment-like text is template text\n    ]>"),
        ]
        for index, (old, replacement) in enumerate(rewrite_variants):
            project = tmp / f"rewrites-{index}";project.mkdir()
            (project / "Rewrites.ngr").write_text((ROOT / "languages/rewrites/Rewrites.ngr").read_text().replace(old, replacement, 1))
            (project / "Dates.ngr").write_bytes((ROOT / "languages/dates/Dates.ngr").read_bytes())
            (project / "catalog.json").write_bytes((ROOT / "languages/dates/catalog.json").read_bytes())
            (project / "project.json").write_text(json.dumps(dict(mode="typed",start="Dates.Program",grammars=["Dates.ngr","Rewrites.ngr"],catalog="catalog.json")))
            compare(project / "project.json",tmp,index+120)
        project = tmp / "rewrites-syntax";project.mkdir()
        for name in ["Dates.ngr", "catalog.json"]:
            (project / name).write_bytes((ROOT / "languages/dates" / name).read_bytes())
        (project / "Rewrites.ngr").write_bytes((ROOT / "languages/rewrites/Rewrites.ngr").read_bytes())
        (project / "catalog.json").write_text('{"types":[],"operations":[],"statements":{}}')
        (project / "project.json").write_text(json.dumps(dict(mode="syntax",start="Dates.Program",grammars=["Dates.ngr","Rewrites.ngr"],catalog="catalog.json")))
        compare(project / "project.json",tmp,130)
        project = tmp / "transparent";project.mkdir()
        (project / "T.ngr").write_text('syntax module T { syntax Start = Items:S*; extensible syntax S { | transparent Plain = Body:(W / ";") | transparent W2 = W | Other = transparent | Block = "{" Items:S* "}" } syntax W = "w"; syntax transparent = "t"; }')
        (project / "catalog.json").write_text('{"types":[],"operations":[],"statements":{}}')
        (project / "project.json").write_text(json.dumps(dict(mode="syntax",start="T.Start",grammars=["T.ngr"],catalog="catalog.json")))
        compare(project / "project.json",tmp,140)
        project = tmp / "reference-operator";project.mkdir()
        (project / "R.ngr").write_text('syntax module R { syntax Start = E; token MultiplyOp = "*" / "/"; syntax Arguments = "(" Value:E ")"; extensible syntax E { | Num = Value:Digits | Invoke = Target:E Args:Arguments precedence 180 left | Multiply = Left:E MultiplyOp Right:E precedence 160 left | Add = Left:E "+" Right:E precedence 150 left } token Digits = [\'0\'..\'9\']+; }')
        (project / "catalog.json").write_text('{"types":[],"operations":[],"statements":{}}')
        (project / "project.json").write_text(json.dumps(dict(mode="syntax",start="R.Start",grammars=["R.ngr"],catalog="catalog.json")))
        compare(project / "project.json",tmp,141)
        project = tmp / "surrogates";project.mkdir()
        (project / "S.ngr").write_text("syntax module S { syntax Start = Items:Id*; token Id = (['a'..'z'] / ['\\ud835'] ['\\udc00'..'\\udfff'] / ['\\ud800']) ([^ ' ' '\\ud800'..'\\udbff'])*; }")
        (project / "catalog.json").write_text('{"types":[],"operations":[],"statements":{}}')
        (project / "project.json").write_text(json.dumps(dict(mode="syntax",start="S.Start",grammars=["S.ngr"],catalog="catalog.json")))
        compare(project / "project.json",tmp,142)
        project = tmp / "shared-operator";project.mkdir()
        (project / "M.ngr").write_text('syntax module M { syntax Start = E; token Id = [\'a\'..\'z\']+; extensible syntax E { | Name = Value:Id | Member = Target:E "." Name:Id precedence 180 left | GenericMember = Target:E "." Name:Id "<" Argument:Id ">" precedence 180 left } }')
        (project / "catalog.json").write_text('{"types":[],"operations":[],"statements":{}}')
        (project / "project.json").write_text(json.dumps(dict(mode="syntax",start="M.Start",grammars=["M.ngr"],catalog="catalog.json")))
        compare(project / "project.json",tmp,143)
        # Grammar functions: CSNitrogen's real Lexical.ngr, then generated coverage.
        project = tmp / "csharp-lexical";project.mkdir()
        (project / "Lexical.ngr").write_bytes((ROOT / "languages/csharp4/Grammars/Lexical.ngr").read_bytes())
        (project / "catalog.json").write_text('{"types":[],"operations":[],"statements":{}}')
        (project / "project.json").write_text(json.dumps(dict(mode="syntax",start="CSharp.Lexical.IdentifierFragment",grammars=["Lexical.ngr"],catalog="catalog.json")))
        compare(project / "project.json",tmp,150)
        functions = [
            'function F(n: Grammar.Int) : Grammar.Rule = [char(n + 65) .. char(n * 2 + 70)]; token T = F(3);',
            'function F(n: Grammar.Int) : Grammar.Rule = if n % 3 == 0 and not (n / 2 > 7) or n ^ 2 <= 4 then "a" else "b"; token T = F(6) F(16) F(2);',
            "function F(c: Grammar.Class) : Grammar.Rule = let x = (c | ['x']) - ['b'..'c'] + 1; x; token T = F(['a'..'d']);",
            "function F(c: Grammar.Class) : Grammar.Rule = let x = ~(c & ['a'..'m']) & ['a'..'z']; x; token T = F(['f'..'q']);",
            'function F(w: Grammar.Text) : Grammar.Rule = seq c in w { [upper(c) lower(c)] }; token T = F("aBc");',
            'function F(w: Grammar.Text) : Grammar.Rule = choice i in 0 .. length(w) - 1 { let k = i * 2; where k != 2 : [char(code(\'a\') + k)] }; token T = F("abcd");',
            "function F(r: Grammar.Rule, c: Grammar.Char) : Grammar.Rule = r r? [c]; token D = ['0'..'9']; token T = F(D, 'x');",
            "function F(n: Grammar.Int) : Grammar.Rule = choice d in 0 .. n { [char(48 + d)] \"!\" }; token T = F(3);",
            'function G(r: Grammar.Rule) : Grammar.Rule = r r; function F(n: Grammar.Int) : Grammar.Rule = G({ let x = n + 1; where x > 1 : [char(x + 64)] }); token T = F(1) F(2);',
            "function G(n: Grammar.Int) : Grammar.Rule = [char(n)]; function F(n: Grammar.Int) : Grammar.Rule = G(n) G(n + 1); syntax U = Value:F(97);",
            "token D = ['0'..'9']; function F(c: Grammar.Class) : Grammar.Rule = let x = c - ['5']; x; token T = F(D);",
            "function F(t: Grammar.Text) : Grammar.Rule = \"<\" t \">\"; token T = F(\"ab\") F(\"\");",
        ]
        for index, body in enumerate(functions):
            project = tmp / f"functions-{index}";project.mkdir()
            source = f"syntax module F {{ {body} syntax Start = Value:{'U' if 'syntax U' in body else 'T'}; }}"
            (project / "F.ngr").write_text(source)
            (project / "catalog.json").write_text('{"types":[],"operations":[],"statements":{}}')
            (project / "project.json").write_text(json.dumps(dict(mode="syntax",start="F.Start",grammars=["F.ngr"],catalog="catalog.json")))
            compare(project / "project.json",tmp,160 + index)
        rejected = [
            'syntax module Bad { syntax Start = (Item; Separator)*; syntax Item = "a"; token Separator = ","; }',
            'syntax module Bad { syntax Start = (Item; "")*; syntax Item = "a"; }',
            'syntax module Bad { syntax Start = Missing; }',
            'syntax module Bad { using Missing; syntax Start = "x"; }',
            'syntax module Bad { syntax Start = Start "x" / "y"; }',
            'syntax module Bad { token T = Start; syntax Start = "x"; }',
            'syntax module Bad { syntax Start = [\'z\'..\'a\']; }',
            'syntax module Bad { syntax Start = "x"; syntax Start = "y"; }',
            'syntax module Bad { syntax Start = E; extensible syntax E { | N = "x" | Add = E "+" E precedence 256 } }',
            'syntax module Bad { syntax Start = E; extensible syntax E { | N = "x"? } }',
            'syntax module Bad { syntax Start = X:"a" X:"b"; }',
            'syntax module Bad { symbols { foo } syntax Start = "a" references foo X; }',
            'syntax module Bad { symbols { foo } syntax Start = "a" declares foo X sequential type Y; }',
            'syntax module Bad { syntax Start = "a" / "a"; }',
            'syntax module Bad { token T = "a" / "ab"; syntax Start = T; }',
            'syntax module Bad { syntax Start = E; extensible syntax E { | N = "a" } } syntax module Extra { extend syntax Bad.E { | N = "b" } }',
            'syntax module Bad { syntax function F(X: Start*) : Start = <[ a ]>; syntax Start = "a"; }',
            'syntax module Bad { syntax function F(X: Start) : Start = match X { | <[ a ]> => <[ a ]>; }; syntax Start = "a"; }',
            'syntax module Bad { syntax function F(X: Start) : Start = match X { | _ => <[ a ]>; | <[ a ]> => <[ a ]>; }; syntax Start = "a"; }',
            'syntax module Bad { syntax function F(X: T) : Start = <[ a ]>; token T = "t"; syntax Start = "a"; }',
            'syntax module Bad { syntax Start = S; extensible syntax S { | transparent P = W W | X = "x" } syntax W = "w"; }',
            'syntax module Bad { function L(n: Grammar.Int) : Grammar.Rule = L(n + 1); token T = L(0); syntax Start = T; }',
            'syntax module Bad { function B(n: Grammar.Int) : Grammar.Rule = choice i in 0 .. n { "a" }; token T = B(1099511627776); syntax Start = T; }',
            'syntax module Bad { token T = Missing(1); syntax Start = T; }',
            'syntax module Bad { function F(n: Grammar.Int) : Grammar.Rule = [n] (if n then "a" else "b"); syntax Start = "t"; }',
            'syntax module Bad { function F(n: Grammar.Int) : Grammar.Rule = [char(n)]; token T = F(70000); syntax Start = T; }',
            'syntax module Bad { function F(n: Grammar.Int) : Grammar.Rule = [char(1 / n)]; token T = F(0); syntax Start = T; }',
            'syntax module Bad { function F(n: Grammar.Int, n: Grammar.Int) : Grammar.Rule = "a"; syntax Start = "t"; }',
            'syntax module Bad { function F(n: Grammar.Int) : Grammar.Rule = [] ; syntax Start = "t"; }',
        ]
        bad = tmp / "bad"
        bad.mkdir()
        (bad / "catalog.json").write_text('{"types":[],"operations":[],"statements":{}}')
        (bad / "project.json").write_text(json.dumps({"mode":"syntax","start":"Bad.Start","grammars":["Bad.ngr"],"catalog":"catalog.json"}))
        for source in rejected:
            (bad / "Bad.ngr").write_text(source)
            reference = run("export-project", bad / "project.json", bad / "output.json")
            native = subprocess.run([ROOT / "target/release/nitride", "prepare", bad / "project.json"], capture_output=True)
            assert reference.returncode != 0 and native.returncode != 0, source
    print("105 prepared profiles match live .NET export; 29 invalid grammars are rejected by both")


if __name__ == "__main__":
    main()
