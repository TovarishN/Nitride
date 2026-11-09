"""Compare LSP analysis phases in matched, prebuilt executables.

Each executable performs one warm edit and five checked, measured updates.
The large valid case additionally uses five fresh processes per executable.
Do not compile or run tests alongside this script.
"""
import argparse
import json
import pathlib
import platform
import statistics
import tempfile

from benchmark_checkpoint import ROOT, command, sha, source_identity
from csharp4_scaling import generated
from recovery_scaling import damaged


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', type=pathlib.Path, required=True)
    parser.add_argument('--after', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--baseline-revision', default='2d31168')
    parser.add_argument('--build-description', default='unspecified; executable hashes identify the supplied binaries')
    args = parser.parse_args()
    binaries = [args.before.resolve(), args.after.resolve()]
    report = {
        'source': source_identity(), 'host': platform.platform(),
        'rustc': command(['rustc', '-Vv']).decode(),
        'cpu': command(['sysctl', '-n', 'machdep.cpu.brand_string']).decode().strip()
        if platform.system() == 'Darwin' else platform.processor(),
        'executables': [{'path': str(p), 'sha256': sha(p.read_bytes())} for p in binaries],
        'declared_baseline_revision': args.baseline_revision,
        'source_hashes': {name: {
            'before': sha(command(['git', 'show', f'{args.baseline_revision}:{name}'])),
            'after': sha((ROOT / name).read_bytes())}
            for name in ('crates/nitride-core/src/syntax/shared.rs', 'crates/nitride-core/src/syntax/arena.rs',
                         'crates/nitride-codegen/src/runtime_snapshot.rs',
                         'crates/nitride-lsp/src/analysis.rs', 'crates/nitride-lsp/examples/benchmark.rs')},
        'declared_build': args.build_description,
        'definitions': 'In-process analysis, excluding registry load/debounce/stdio/GUI. One warm edit then five samples per process, always update before fresh analysis. Each update compares tree shape/edges, values and problems to fresh analysis outside timing. Alternating executable order across fresh processes; medians below are medians of process medians.',
        'complete': False, 'rows': [],
    }
    with tempfile.TemporaryDirectory(prefix='nitride-lsp-paired-') as folder:
        for size in (256 << 10, 4 << 20):
            base = generated(size)
            for pattern in ('valid', 'one'):
                path = pathlib.Path(folder) / f'{size}-{pattern}.csharp4'
                path.write_text(base if pattern == 'valid' else damaged(base, 'one'))
                row = {'case': path.stem, 'input_sha256': sha(path.read_bytes()),
                       'before': [], 'after': []}
                report['rows'].append(row)
                repetitions = 5 if size == 4 << 20 and pattern == 'valid' else 1
                for run in range(repetitions):
                    for variant in ([0, 1] if run % 2 == 0 else [1, 0]):
                        data = json.loads(command([binaries[variant], ROOT, path]))
                        if len(data['samples']) != 5:
                            raise ValueError('expected five measured edits after one warm edit')
                        data['medians'] = {key: statistics.median(s[key] for s in data['samples'])
                                           for key in data['samples'][0]}
                        row['before' if variant == 0 else 'after'].append(data)
                    print(row['case'], 'paired process', run + 1, flush=True)
                row['medians'] = {
                    label: {key: statistics.median(p['medians'][key] for p in row[label])
                            for key in row[label][0]['medians']}
                    for label in ('before', 'after')}
                row['ratios'] = {key: row['medians']['after'][key] / value
                                 for key, value in row['medians']['before'].items() if value}
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_text(json.dumps(report, indent=2) + '\n')
    report['complete'] = True
    args.output.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
