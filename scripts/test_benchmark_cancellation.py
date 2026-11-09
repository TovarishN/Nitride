"""Cancellation evidence must prove abort, not successful partial parsing."""
import unittest
from scripts import benchmark_cancellation as benchmark


class CancellationReportTests(unittest.TestCase):
    def row(self):
        return {'source_bytes': 100, 'calibration_polls': 1000, 'samples': [
            {'gate_percent_of_calibrated_polls': percent, 'gate_poll': percent * 10,
             'observed_polls': percent * 10 + 1, 'signal_to_return_ns': 100,
             'outcome': 'cancelled', 'stop_span': [10, 10]}
            for percent in [10, 50, 90] for _ in range(3)]}

    def test_complete_cancelled_samples_are_preserved(self):
        row = self.row()
        self.assertEqual(benchmark.validate(row)['samples'], row['samples'])

    def test_partial_success_or_unobserved_signal_is_rejected(self):
        for key, value in [('outcome', 'success'), ('observed_polls', 0),
                           ('signal_to_return_ns', -1), ('stop_span', [0, 101])]:
            row = self.row()
            row['samples'][0][key] = value
            with self.assertRaises(ValueError): benchmark.validate(row)
        row = self.row(); row['samples'].pop()
        with self.assertRaises(ValueError): benchmark.validate(row)


if __name__ == '__main__': unittest.main()
