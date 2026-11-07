"""Checkpoint reports must reject incomparable or unstable measurements."""
import importlib.util
import pathlib
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("checkpoint", pathlib.Path(__file__).with_name("benchmark_checkpoint.py"))
checkpoint = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checkpoint)


class CheckpointTests(unittest.TestCase):
    def test_medians_preserve_samples_and_input_identity(self):
        row = checkpoint.summarize("valid", "abc", [{"elapsed_ns": 30, "fingerprint": "a"}, {"elapsed_ns": 10, "fingerprint": "a"}, {"elapsed_ns": 20, "fingerprint": "a"}])
        self.assertEqual(row["medians"]["elapsed_ns"], 20)
        self.assertEqual(row["input_sha256"], "abc")
        self.assertEqual(len(row["samples"]), 3)

    def test_fingerprint_drift_is_an_error(self):
        with self.assertRaisesRegex(ValueError, "fingerprint"):
            checkpoint.summarize("bad", "abc", [{"elapsed_ns": 1, "fingerprint": "a"}, {"elapsed_ns": 2, "fingerprint": "b"}])

    def test_comparison_rejects_different_inputs(self):
        with self.assertRaisesRegex(ValueError, "input"):
            checkpoint.compare({"case": "valid", "input_sha256": "a", "medians": {"elapsed_ns": 10}},
                               {"case": "valid", "input_sha256": "b", "medians": {"elapsed_ns": 5}})

    def test_comparison_reports_ratio(self):
        result = checkpoint.compare({"case": "valid", "input_sha256": "a", "medians": {"elapsed_ns": 10}},
                                    {"case": "valid", "input_sha256": "a", "medians": {"elapsed_ns": 8}})
        self.assertEqual(result["ratios"]["elapsed_ns"], 0.8)

    def test_source_identity_includes_untracked_code(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)
            import subprocess
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            (root / "new.rs").write_text("one")
            before = checkpoint.source_identity(root)["diff_sha256"]
            (root / "new.rs").write_text("two")
            self.assertNotEqual(checkpoint.source_identity(root)["diff_sha256"], before)


if __name__ == "__main__":
    unittest.main()
