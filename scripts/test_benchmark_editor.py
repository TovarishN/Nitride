"""Baseline reports must distinguish typed work and reject missing phase data."""
import unittest
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from scripts import benchmark_editor as benchmark


class EditorBenchmarkTests(unittest.TestCase):
    def test_missing_binary_invalidates_old_complete_report(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / 'report.json'
            output.write_text('{"complete": true}')
            result = subprocess.run([sys.executable, benchmark.__file__, '--binary', str(root / 'missing'),
                                     '--output', str(output), '--build-description', 'missing test'],
                                    capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            report = json.loads(output.read_text())
            self.assertFalse(report['complete'])
            self.assertIn('error', report)

    def test_failed_run_replaces_an_old_complete_report(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'report.json'
            output.write_text('{"complete": true}')
            # Python itself is an executable, but cannot execute this benchmark's
            # workspace/source arguments, so the first measurement fails.
            result = subprocess.run([sys.executable, benchmark.__file__, '--binary', sys.executable,
                                     '--output', str(output), '--build-description', 'failure test'],
                                    capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(json.loads(output.read_text())['complete'])

    def test_typed_fixture_has_unique_bindings_and_references(self):
        source = benchmark.portable(1024)
        self.assertGreaterEqual(len(source), 1024)
        self.assertEqual(source.count('let v0 : Scalar'), 1)
        self.assertIn('v0 + 2;', source)
        self.assertTrue(source.endswith(';\n'))

    def test_partial_or_negative_phase_data_cannot_form_complete_report(self):
        sample = {key: 1 for key in benchmark.PHASES}
        for samples in [[], [sample] * 4, [{**sample, 'semantic_ns': -1}] * 5,
                        [{key: 1 for key in benchmark.PHASES if key != 'editor_ns'}] * 5]:
            with self.assertRaises(ValueError):
                benchmark.summarize({'samples': samples})

    def test_phase_medians_preserve_raw_samples(self):
        samples = [{key: value for key in benchmark.PHASES} for value in [1, 4, 3, 2, 5]]
        result = benchmark.summarize({'samples': samples})
        self.assertEqual(result['medians']['incremental_ns'], 3)
        self.assertEqual(result['samples'], samples)


if __name__ == '__main__':
    unittest.main()
