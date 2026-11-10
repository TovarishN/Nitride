"""New measurement output must not overwrite frozen evidence."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from report_paths import report_path, write_report
from check_evidence_archive import check_archive


class ReportPaths(unittest.TestCase):
    def test_default_output_leaves_archived_measurements_unchanged(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            archived = root / "docs/archive/stages/stage14-recovery-quality.json"
            archived.parent.mkdir(parents=True)
            archived.write_bytes(b'{"old": true}\n')
            output = write_report("stage14-recovery-quality.json", {"new": [1, 2]}, root=root)
            self.assertEqual(output, root / "target/reports/stage14-recovery-quality.json")
            self.assertEqual(json.loads(output.read_text()), {"new": [1, 2]})
            self.assertEqual(archived.read_bytes(), b'{"old": true}\n')

    def test_report_names_cannot_escape_the_output_directory(self):
        for name in ["../docs/archive/file.json", "/tmp/file.json", r"..\file.json", "C:file.json"]:
            with self.assertRaises(ValueError):
                report_path(name)


class ArchiveHashes(unittest.TestCase):
    def fixture(self, root):
        path = root / "docs/archive/stages/stage2-phase-memory.json"
        path.parent.mkdir(parents=True)
        data = b'{"measurements": [1, 2]}\n'
        path.write_bytes(data)
        digest = hashlib.sha256(data).hexdigest()
        manifest = {"schema": 1, "source_commit": "frozen", "files": [{
            "original": "docs/stage2-phase-memory.json", "archived": path.relative_to(root).as_posix(),
            "original_sha256": digest, "sha256": digest, "bytes": len(data),
        }]}
        (path.parent / "manifest.json").write_text(json.dumps(manifest))
        return path, manifest

    def test_checker_detects_modified_evidence(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            path, _ = self.fixture(root)
            self.assertEqual(check_archive(root), [])
            path.write_bytes(b'{"measurements": [3]}\n')
            self.assertTrue(check_archive(root))

    def test_updating_checksum_does_not_hide_changed_raw_json(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            path, manifest = self.fixture(root)
            data = b'{"measurements": [3]}\n'
            path.write_bytes(data)
            manifest["files"][0].update(sha256=hashlib.sha256(data).hexdigest(), bytes=len(data))
            (path.parent / "manifest.json").write_text(json.dumps(manifest))
            self.assertTrue(check_archive(root))

    def test_checker_accepts_an_explicit_nonstage_evidence_manifest(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            path, manifest = self.fixture(root)
            destination = root / "docs/benchmarks/compiler/compiler-optimization-audit.json"
            destination.parent.mkdir(parents=True)
            path.rename(destination)
            manifest["files"][0]["archived"] = destination.relative_to(root).as_posix()
            index = root / "docs/archive/layout-manifest.json"
            index.write_text(json.dumps(manifest))
            self.assertEqual(check_archive(root, manifest=Path("docs/archive/layout-manifest.json")), [])
            destination.write_bytes(b"changed")
            self.assertTrue(check_archive(root, manifest=Path("docs/archive/layout-manifest.json")))


if __name__ == "__main__":
    unittest.main()
