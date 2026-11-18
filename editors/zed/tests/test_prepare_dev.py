"""Exercise the installable artifact, including Zed's local Git grammar checkout."""
import pathlib
import subprocess
import sys
import tempfile
import tomllib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]


class PrepareDevTests(unittest.TestCase):
    def test_prepared_extension_can_checkout_its_exact_grammar_revision(self):
        with tempfile.TemporaryDirectory(prefix="nitride zed ") as tmp:
            output = pathlib.Path(tmp) / "dev extension"
            command = [sys.executable, str(ROOT / "prepare-dev.py"), "--output", str(output)]
            first = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(first.returncode, 0, first.stderr)
            manifest = tomllib.loads((output / "extension.toml").read_text())
            grammar = manifest["grammars"]["nitride"]
            clone = pathlib.Path(tmp) / "zed grammar checkout"
            subprocess.run(["git", "clone", "--quiet", grammar["repository"], str(clone)], check=True)
            subprocess.run(["git", "checkout", "--quiet", grammar["rev"]], cwd=clone, check=True)
            self.assertTrue((clone / "src/parser.c").is_file())
            self.assertTrue((output / "Cargo.lock").is_file())
            self.assertTrue((output / "languages/ngr/highlights.scm").is_file())
            second = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(second.returncode, 0, second.stderr)
            updated = tomllib.loads((output / "extension.toml").read_text())
            self.assertEqual(updated["grammars"]["nitride"]["rev"], grammar["rev"])

    def test_existing_unmanaged_directory_is_preserved(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = pathlib.Path(tmp) / "user files"
            output.mkdir()
            sentinel = output / "keep.txt"
            sentinel.write_text("keep me")
            result = subprocess.run([sys.executable, str(ROOT / "prepare-dev.py"), "--output", str(output)], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unmanaged", result.stderr)
            self.assertEqual(sentinel.read_text(), "keep me")

    def test_source_directory_cannot_be_used_as_output(self):
        result = subprocess.run([sys.executable, str(ROOT / "prepare-dev.py"), "--output", str(ROOT)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("source", result.stderr.lower())


if __name__ == "__main__":
    unittest.main()
