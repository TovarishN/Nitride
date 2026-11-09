"""Paired reports must identify new source files without hiding bad revisions."""
import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchmark_lsp as paired


class SourceHashTests(unittest.TestCase):
    def test_new_module_has_explicit_absent_baseline_and_current_hash(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(['git', 'init', '-q', str(root)], check=True, capture_output=True)
            (root / 'existing.rs').write_bytes(b'before')
            subprocess.run(['git', 'add', 'existing.rs'], cwd=root, check=True, capture_output=True)
            subprocess.run(['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                            'commit', '-qm', 'fixture'], cwd=root, check=True, capture_output=True)
            (root / 'existing.rs').write_bytes(b'after')
            (root / 'projection.rs').write_bytes(b'new module')
            result = paired.source_hashes('HEAD', ['existing.rs', 'projection.rs'], root)
            digest = lambda data: hashlib.sha256(data).hexdigest()
            self.assertEqual(result['existing.rs'], {'before': digest(b'before'), 'after': digest(b'after')})
            self.assertEqual(result['projection.rs'], {'before': None, 'after': digest(b'new module')})
            with self.assertRaises(subprocess.CalledProcessError):
                paired.source_hashes('missing-revision', ['projection.rs'], root)

    def test_owning_files_include_projection_module(self):
        self.assertIn('crates/nitride-lsp/src/analysis/projection.rs', paired.SOURCES)


if __name__ == '__main__':
    unittest.main()
