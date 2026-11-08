"""The packaged-editor harness must reject unsafe payloads and isolate fixtures."""
import tempfile
import re
import json
import os
import subprocess
import sys
import unittest
import zipfile
from pathlib import Path

from scripts import editor_acceptance as acceptance


class AcceptanceTests(unittest.TestCase):
    def test_timed_out_host_process_is_reaped(self):
        from unittest.mock import patch
        original = subprocess.Popen
        children = []

        def launch(*args, **kwargs):
            child = original(*args, **kwargs)
            children.append(child)
            return child

        with patch.object(acceptance.subprocess, 'Popen', side_effect=launch):
            with self.assertRaises(subprocess.TimeoutExpired):
                acceptance.run_host([sys.executable, '-c', 'import time; time.sleep(60)'],
                                    os.environ.copy(), timeout=0.1)
        self.assertIsNotNone(children[0].poll())

    def test_missing_vsix_invalidates_old_complete_report(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / 'result.json'
            output.write_text('{"complete": true}')
            result = subprocess.run([sys.executable, acceptance.__file__, '--code', sys.executable,
                                     '--vsix', str(root / 'missing.vsix'), '--output', str(output)],
                                    capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            report = json.loads(output.read_text())
            self.assertFalse(report['complete'])
            self.assertIn('error', report)

    def test_interrupted_host_is_terminated_and_reaped(self):
        from unittest.mock import Mock, patch
        process = Mock(pid=12345)
        process.communicate.side_effect = [KeyboardInterrupt(), (b'', None)]
        process.poll.return_value = None
        with patch.object(acceptance.subprocess, 'Popen', return_value=process), \
             patch.object(acceptance.os, 'killpg', create=True) as kill_group, \
             patch.object(acceptance.subprocess, 'run') as terminate_windows:
            with self.assertRaises(KeyboardInterrupt):
                acceptance.run_host(['code'], {}, timeout=1)
        self.assertEqual(process.communicate.call_count, 2)
        self.assertIn('timeout', process.communicate.call_args.kwargs)
        if acceptance.os.name == 'nt':
            terminate_windows.assert_called_once()
        else:
            kill_group.assert_called_once()

    def test_archive_cannot_escape_its_disposable_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / 'bad.vsix'
            with zipfile.ZipFile(archive, 'w') as stream:
                stream.writestr('extension/../../escape', 'bad')
            with self.assertRaisesRegex(ValueError, 'path'):
                acceptance.extract(archive, root / 'unpacked')
            self.assertFalse((root / 'escape').exists())

    def test_payload_must_have_a_bundled_native_server(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / 'bad.vsix'
            with zipfile.ZipFile(archive, 'w') as stream:
                stream.writestr('extension/package.json', '{}')
            with self.assertRaisesRegex(ValueError, 'server'):
                acceptance.extract(archive, root / 'unpacked')

    def test_fixture_changes_never_touch_repository_sources(self):
        with tempfile.TemporaryDirectory() as directory:
            source = acceptance.ROOT / 'languages/portable/Portable.ngr'
            before = source.read_bytes()
            workspace = acceptance.fixture(Path(directory))
            (workspace / 'portable/Portable.ngr').write_text('broken')
            self.assertEqual(source.read_bytes(), before)
            self.assertTrue((workspace / 'nitride.nproj').is_file())

    def test_full_language_fixture_preserves_every_workspace_include(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace = acceptance.fixture(Path(directory), repository_languages=True)
            includes = re.findall(r'\binclude\s+([^;\s]+)\s*;',
                                  (workspace / 'nitride.nproj').read_text())
            self.assertGreater(len(includes), 1)
            for include in includes:
                self.assertEqual(len(list((workspace / include).glob('*.nproj'))), 1)


if __name__ == '__main__':
    unittest.main()
