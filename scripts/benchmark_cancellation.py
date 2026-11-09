"""Measure gated cancellation return latency with the prebuilt native example."""
import argparse
from collections import Counter
import json
from pathlib import Path
import platform
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
from benchmark_checkpoint import ROOT, command, sha, source_identity
from benchmark_lsp import SOURCES
from csharp4_scaling import generated
from recovery_scaling import damaged


def validate(row):
    try:
        size = row['source_bytes']; total = row['calibration_polls']; samples = row['samples']
        if type(size) is not int or size <= 0 or type(total) is not int or total <= 20:
            raise ValueError('invalid source/calibration')
        if len(samples) != 9 or Counter(s['gate_percent_of_calibrated_polls'] for s in samples) != {10: 3, 50: 3, 90: 3}:
            raise ValueError('expected three samples at each calibrated gate')
        for sample in samples:
            span = sample['stop_span']
            if (sample['outcome'] != 'cancelled'
                or sample['gate_poll'] != total * sample['gate_percent_of_calibrated_polls'] // 100
                or sample['observed_polls'] <= sample['gate_poll']
                or type(sample['signal_to_return_ns']) is not int or sample['signal_to_return_ns'] < 0
                or not isinstance(span, list) or len(span) != 2
                or any(type(v) is not int for v in span) or not 0 <= span[0] <= span[1] <= size):
                raise ValueError('missing cancellation, next-poll observation or valid latency/span')
    except (KeyError, TypeError) as error:
        raise ValueError('incomplete cancellation evidence') from error
    return row


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    report = {'complete': False, 'rows': []}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    try:
        binary = args.binary.resolve()
        report.update({'source': source_identity(), 'binary_sha256': sha(binary.read_bytes()),
                       'host': platform.platform(), 'rustc': command(['rustc', '-Vv']).decode(),
                       'source_hashes': {name: sha((ROOT / name).read_bytes()) for name in
                                         (*SOURCES, 'crates/nitride-lsp/examples/cancellation.rs')},
                       'definition': 'Native RuntimeSnapshot parse, valid/broken 4 MiB source. '
                       'Signal precedes release of an intentionally false gated poll; includes next polling batch, '
                       'local cleanup and join. Checkpoint ordinals are not elapsed-time percentages or phase tags. '
                       'Preparation excluded; no hard deadline or GUI/whole-LSP latency claim.'})
        with tempfile.TemporaryDirectory(prefix='nitride-cancel-bench-') as directory:
            base = generated(4 << 20)
            for case, source in [('valid', base), ('missing-semicolon', damaged(base, 'one'))]:
                path = Path(directory) / f'{case}.csharp4'; path.write_text(source)
                result = subprocess.run([str(binary), str(ROOT), str(path)], cwd=ROOT,
                                        capture_output=True, text=True, check=True, timeout=300)
                data = validate(json.loads(result.stdout))
                if data['source_bytes'] != len(source.encode()): raise ValueError('input length mismatch')
                report['rows'].append({'case': case, 'input_sha256': sha(source.encode()), **data})
                args.output.write_text(json.dumps(report, indent=2) + '\n')
                print(case, [round(s['signal_to_return_ns'] / 1e6, 3) for s in data['samples']], flush=True)
        report['complete'] = True
    except BaseException as error:
        report['error'] = f'{type(error).__name__}: {error}'
        raise
    finally:
        args.output.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__': main()
