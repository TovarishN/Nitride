"""Behavioral contracts for ordered multi-module language preparation."""
import json
import subprocess
import pathlib
import shutil
import tempfile
import unittest
from test_reference import ROOT, run


def export(project, output):
    return run("export-project", project, output)


def references(expr):
    if expr.get("kind") == "ref":
        yield expr["name"]
    for child in expr.get("items", []):
        yield from references(child)
    if "inner" in expr:
        yield from references(expr["inner"])


class CompositionTests(unittest.TestCase):
    def test_imported_tokens_and_extension_references_resolve(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = pathlib.Path(tmp) / "language.json"
            result = export(ROOT / "languages/composition/project.json", out)
            self.assertEqual(result.returncode, 0, result.stderr)
            data = json.loads(out.read_text())
            self.assertEqual(data["start"], "Uses.Doc")
            by_name = {r["name"]: r for r in data["rules"]}
            self.assertIn("Base.Word", list(references(by_name["Uses.Assign"]["expr"])))
            self.assertEqual(list(references(by_name["Uses.Neg"]["expr"])), ["Base.Value"])
            self.assertIn("Uses.Neg", by_name["Base.Value"]["expr"]["prefixes"])

    def test_typed_extension_retains_identity_and_implicit_target_module(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = pathlib.Path(tmp) / "language.json"
            result = export(ROOT / "languages/power/project.json", out)
            self.assertEqual(result.returncode, 0, result.stderr)
            by_name = {r["name"]: r for r in json.loads(out.read_text())["rules"]}
            power = by_name["Portable.Power.Pow"]
            self.assertEqual(list(references(power["expr"])), ["Portable.Expr", "Portable.Expr"])
            self.assertEqual(power["owner"], "Portable.Expr")
            self.assertIn("Portable.Power.Pow", by_name["Portable.Expr"]["expr"]["infixes"])

    def test_xml_syntax_profile_records_omitted_semantics(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = pathlib.Path(tmp) / "xml.json"
            result = export(ROOT / "languages/xml/project.json", out)
            self.assertEqual(result.returncode, 0, result.stderr)
            data = json.loads(out.read_text())
            self.assertEqual(data["mode"], "syntax")
            self.assertIn("Xml.Paired", [item["rule"] for item in data["omitted_semantics"]])

    def test_missing_and_ambiguous_imports_fail_without_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "catalog.json").write_text('{"types":[],"operations":[],"statements":{}}')
            (root / "Base.ngr").write_text('syntax module Base { token Word = [\'a\'..\'z\']+; }')
            (root / "Extra.ngr").write_text('syntax module Extra { token Word = [\'a\'..\'z\']+; }')
            for imports in ["using Missing;", "using Base; using Extra;"]:
                (root / "Main.ngr").write_text('syntax module Main { ' + imports + ' syntax Start = Word; }')
                manifest = {"mode":"syntax","start":"Main.Start","grammars":["Base.ngr","Extra.ngr","Main.ngr"],"catalog":"catalog.json"}
                (root / "project.json").write_text(json.dumps(manifest))
                output = root / "out.json"
                result = export(root / "project.json", output)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(output.exists())

    def test_manifest_is_relocatable_and_repeatable(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            shutil.copytree(ROOT / "languages/composition", root / "relocated")
            a,b = root / "a.json", root / "b.json"
            self.assertEqual(export(ROOT / "languages/composition/project.json", a).returncode, 0)
            self.assertEqual(export(root / "relocated/project.json", b).returncode, 0)
            self.assertEqual(a.read_bytes(), b.read_bytes())

    def test_shared_extension_operators_are_admitted(self):
        # Alternatives may share an operator, as in Nitrogen: the parser ranks
        # them, and an exact tie is its ambiguity diagnostic at parse time.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "Base.ngr").write_text('syntax module Base { token N = [\'0\'..\'9\']+; syntax Start = E; extensible syntax E { | Num = N } }')
            for name in ["One", "Two"]:
                (root / f"{name}.ngr").write_text(f'syntax module {name} {{ extend syntax Base.E {{ | Add{name} = E "+" E precedence 10 left }} }}')
            (root / "catalog.json").write_text('{"types":[],"operations":[],"statements":{}}')
            (root / "project.json").write_text(json.dumps({"mode":"syntax","start":"Base.Start","grammars":["Base.ngr","One.ngr","Two.ngr"],"catalog":"catalog.json"}))
            output = root / "out.json"
            self.assertEqual(export(root / "project.json", output).returncode, 0)
            native = subprocess.run([ROOT / "target/release/nitride", "prepare", root / "project.json"], capture_output=True, text=True)
            self.assertEqual(native.returncode, 0, native.stderr)
            self.assertEqual(json.loads(native.stdout), json.loads(output.read_text()))


if __name__ == "__main__":
    unittest.main()
