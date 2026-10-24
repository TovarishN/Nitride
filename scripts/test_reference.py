"""Behavioral contract tests for the development-only reference exporter."""
import json
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
DLL = ROOT / "tools/Nitride.Reference/bin/Debug/net10.0/Nitride.Reference.dll"


def run(*args):
    return subprocess.run(["dotnet", str(DLL), *map(str, args)], capture_output=True, text=True)


class ExportTests(unittest.TestCase):
    def test_export_is_path_independent_and_has_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            grammar = ROOT / "languages/portable/Portable.ngr"
            catalog = ROOT / "languages/portable/catalog.json"
            copy = pathlib.Path(tmp) / "other.ngr"
            copy.write_bytes(grammar.read_bytes())
            a, b = pathlib.Path(tmp) / "a.json", pathlib.Path(tmp) / "b.json"
            for source, output in [(grammar, a), (copy, b)]:
                result = run("export", source, catalog, output)
                self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(a.read_bytes(), b.read_bytes())
            data = json.loads(a.read_text())
            self.assertEqual(data["version"], 1)
            self.assertEqual(len(data["source_hash"]), 64)
            self.assertEqual(data["start"], "Portable.Program")

    def test_unscoped_builtins_export_kind_and_dotted_names(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp); grammar=root/'test.ngr';output=root/'out.json'
            grammar.write_text('syntax module B { symbols { function } builtin function { neg sys.neg } syntax Start = "x"; }')
            result=run('export',grammar,ROOT/'languages/portable/catalog.json',output)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(json.loads(output.read_text())['builtins'],[dict(kind='function',name='neg'),dict(kind='function',name='sys.neg')])

    def test_duplicate_and_scoped_builtins_are_rejected(self):
        for declaration in ['builtin value in Start { x }','builtin missing { x }','builtin value { x x }']:
            self.reject('syntax module Bad { symbols { value } '+declaration+' syntax Start = "x"; }')

    def test_csharp_blocks_are_rejected(self):
        self.reject('syntax module Bad { syntax Start = "x" { check BAD false : "bad"; } }')

    def test_unknown_rule_is_rejected(self):
        self.reject('syntax module Bad { syntax Start = Missing; }')

    def test_left_recursion_is_rejected(self):
        self.reject('syntax module Bad { syntax Start = Start "x" / "y"; }')

    def test_declared_symbol_namespace_inventory_is_preserved(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = pathlib.Path(tmp) / "out.json"
            result = run("export-project", ROOT / "languages/namespaces/project.json", output)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(output.read_text())["symbol_kinds"], ["value", "rate"])

    def test_undeclared_symbol_namespace_is_rejected(self):
        source = (ROOT / "languages/portable/Portable.ngr").read_text().replace("references value Name", "references missing Name")
        self.reject(source)

    def reject(self, source):
        with tempfile.TemporaryDirectory() as tmp:
            grammar, output = pathlib.Path(tmp) / "bad.ngr", pathlib.Path(tmp) / "out.json"
            grammar.write_text(source)
            result = run("export", grammar, ROOT / "languages/portable/catalog.json", output)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unsupported", result.stderr.lower())
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
