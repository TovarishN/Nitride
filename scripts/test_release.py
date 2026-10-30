"""Behavioral checks for release boundaries; no network or mocked build tools."""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class VersionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for name in ['Cargo.toml', 'Cargo.lock', 'editors/vscode/package.json',
                     'editors/vscode/package-lock.json', 'scripts/version.py']:
            destination = self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            if (ROOT / name).exists():
                shutil.copy2(ROOT / name, destination)
        for manifest in (ROOT / 'crates').glob('*/Cargo.toml'):
            destination = self.root / manifest.relative_to(ROOT)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(manifest, destination)

    def run_version(self, *args):
        return subprocess.run([sys.executable, str(self.root / 'scripts/version.py'), *args],
                              capture_output=True, text=True)

    def test_set_keeps_manifests_locks_and_tag_in_sync(self):
        result = self.run_version('set', '1.2.3')
        self.assertEqual(result.returncode, 0, result.stderr)
        result = self.run_version('check', '--tag', 'v1.2.3')
        self.assertEqual(result.returncode, 0, result.stderr)
        import tomllib
        locked = tomllib.loads((self.root / 'Cargo.lock').read_text())
        self.assertEqual({p['version'] for p in locked['package']
                          if p['name'].startswith('nitride-')}, {'1.2.3'})
        lock = json.loads((self.root / 'editors/vscode/package-lock.json').read_text())
        self.assertEqual(lock['version'], '1.2.3')
        self.assertEqual(lock['packages']['']['version'], '1.2.3')

    def test_invalid_version_never_changes_files(self):
        before = (self.root / 'Cargo.toml').read_bytes()
        for version in ['01.2.3', 'v1.2.3', '1.2', '1.2.3;echo bad']:
            self.assertNotEqual(self.run_version('set', version).returncode, 0)
        self.assertEqual((self.root / 'Cargo.toml').read_bytes(), before)

    def test_mismatch_and_wrong_tag_are_rejected(self):
        self.assertNotEqual(self.run_version('check', '--tag', 'v9.9.9').returncode, 0)
        package = self.root / 'editors/vscode/package.json'
        data = json.loads(package.read_text())
        data['version'] = '9.9.9'
        package.write_text(json.dumps(data))
        self.assertNotEqual(self.run_version('check').returncode, 0)

    def test_set_repairs_an_out_of_sync_extension_version(self):
        package = self.root / 'editors/vscode/package.json'
        data = json.loads(package.read_text())
        data['version'] = '9.9.9'
        package.write_text(json.dumps(data))
        result = self.run_version('set', '1.2.3')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.run_version('check', '--tag', 'v1.2.3').returncode, 0)




class LauncherTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name)
        cls.suffix = '.exe' if sys.platform == 'win32' else ''
        cls.launcher = cls.root / ('nitride' + cls.suffix)
        subprocess.run(['rustc', '--edition=2024', '-O', str(ROOT / 'scripts/release/launcher.rs'),
                        '-o', str(cls.launcher)], check=True, capture_output=True)
        fixture = cls.root / 'fixture.rs'
        fixture.write_text('fn main() { println!("{:?}", std::env::args().skip(1).collect::<Vec<_>>()); std::process::exit(7); }')
        cls.baseline = cls.root / ('nitride-baseline' + cls.suffix)
        subprocess.run(['rustc', str(fixture), '-o', str(cls.baseline)], check=True, capture_output=True)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def test_arguments_stdio_and_exit_status_survive_dispatch(self):
        result = subprocess.run([str(self.launcher), 'lsp', 'with space', 'ü'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 7, result.stderr)
        self.assertEqual(json.loads(result.stdout), ['lsp', 'with space', 'ü'])

    def test_missing_payload_fails_clearly(self):
        hidden = self.baseline.with_suffix('.hidden')
        self.baseline.rename(hidden)
        try:
            result = subprocess.run([str(self.launcher)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn('nitride-baseline', result.stderr)
        finally:
            hidden.rename(self.baseline)


class PackageTests(unittest.TestCase):
    @unittest.skipIf(sys.platform == 'win32', 'Unix directory permissions')
    def test_readonly_container_payload_packages_and_tampering_is_rejected(self):
        from unittest.mock import patch
        sys.path.insert(0, str(ROOT / 'scripts'))
        from release import package as packaging
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            parent = root / 'target/native-release/darwin-arm64'
            payload = parent / 'payload'
            payload.mkdir(parents=True)
            (payload / 'nitride').write_bytes(b'release executable fixture')
            (payload / 'release.json').write_text(json.dumps({
                'version': packaging.version.check(), 'platform': 'darwin-arm64', 'wasmtime': False,
                'variants': {'baseline': {'pgo': True, 'bolt': False}}}))
            (root / 'editors').mkdir()
            (root / 'editors/vscode').symlink_to(ROOT / 'editors/vscode')
            shutil.copy2(ROOT / 'README.md', root / 'README.md')
            parent.chmod(0o555)
            try:
                with patch.object(packaging, 'ROOT', root):
                    packaging.package('darwin-arm64', root / 'dist')
                    packaging.verify_assets(root / 'dist', ['darwin-arm64'])
                    archive = root / f'dist/nitride-{packaging.version.check()}-darwin-arm64.tar.gz'
                    archive.write_bytes(b'corrupted after upload')
                    with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
                        packaging.verify_assets(root / 'dist', ['darwin-arm64'])
            finally:
                parent.chmod(0o755)


if __name__ == '__main__':
    unittest.main()
