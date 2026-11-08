"""Record a checked native analysis baseline; build first, run without other work."""
import argparse
import json
import math
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
from benchmark_checkpoint import ROOT, command, sha, source_identity
from csharp4_scaling import generated
from recovery_scaling import damaged

PHASES = ('incremental_ns', 'fresh_ns', 'parse_update_ns', 'materialize_ns',
          'semantic_ns', 'editor_ns')


def portable(size):
    blocks = []
    length = index = 0
    while length < size:
        block = f'let v{index} : Scalar = {index};\nv{index} + 2;\n'
        blocks.append(block)
        length += len(block)
        index += 1
    return ''.join(blocks)


def summarize(data):
    samples = data.get('samples', [])
    if len(samples) != 5 or any(type(s.get(key)) not in (int, float)
                               or not math.isfinite(s[key]) or s[key] < 0
                               for s in samples for key in PHASES):
        raise ValueError('expected five complete nonnegative phase samples')
    return {**data, 'medians': {key: statistics.median(s[key] for s in samples)
                               for key in PHASES}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--build-description', required=True)
    parser.add_argument('--processes', type=int, default=3)
    args = parser.parse_args()
    if args.processes < 1:
        parser.error('--processes must be positive')
    report = {'complete': False}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    try:
        measure(args, report)
    except BaseException as error:
        report['complete'] = False
        report['error'] = f'{type(error).__name__}: {error}'
        raise
    finally:
        args.output.write_text(json.dumps(report, indent=2) + '\n')


def measure(args, report):
    binary = args.binary.resolve()
    report.update({'source': source_identity(),
              'host': platform.platform(), 'machine': platform.machine(),
              'rustc': command(['rustc', '-Vv']).decode(),
              'binary_sha256': sha(binary.read_bytes()),
              'declared_build': args.build_description,
              'definitions': 'In-process native analyze API, no GUI/stdio/debounce. '
              'Each process performs one warm edit then five measured edits, each checked '
              'against fresh analysis (problems, values, tree shape and edges) outside timing. '
              'Update always precedes fresh analysis. Preparation and initial analysis are '
              'separate. Medians are medians of process medians; no improvement claim.',
              'rows': []})
    with tempfile.TemporaryDirectory(prefix='nitride-editor-baseline-') as directory:
        for language, sizes, make in [('csharp4', [256 << 10, 4 << 20], generated),
                                     ('portable', [16 << 10, 64 << 10], portable)]:
            for size in sizes:
                for state in ['valid', 'missing-semicolon']:
                    source = make(size)
                    if state != 'valid':
                        source = damaged(source, 'one')
                    path = Path(directory) / f'input.{language}'
                    path.write_text(source)
                    row = {'case': f'{language}-{size}-{state}',
                           'mode': 'typed' if language == 'portable' else 'syntax-only',
                           'source_bytes': len(source.encode()), 'input_sha256': sha(source.encode()),
                           'processes': []}
                    for _ in range(args.processes):
                        result = subprocess.run([str(binary), str(ROOT), str(path)], cwd=ROOT,
                                                capture_output=True, text=True, check=True, timeout=180)
                        row['processes'].append(summarize(json.loads(result.stdout)))
                    row['medians'] = {key: statistics.median(p['medians'][key] for p in row['processes'])
                                      for key in PHASES}
                    report['rows'].append(row)
                    args.output.write_text(json.dumps(report, indent=2) + '\n')
                    print(row['case'], row['medians'], flush=True)
    report['complete'] = True


if __name__ == '__main__':
    main()
