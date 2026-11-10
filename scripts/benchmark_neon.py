"""Compare matched default/scalar/NEON compiler-audit binaries on Apple Silicon."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess

from report_paths import ROOT, write_report


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def generated(size):
    source = 'using System;\nusing System.Linq;\n'
    parts = [source]
    i = 0
    length = len(source)
    while length < size:
        part = (
            f'namespace N{i} {{ public class C{i}<T> where T : class {{\n'
            f'int f{i} = {i};\n'
            f'public string Name {{ get {{ return "n{i}"; }} set {{ }} }}\n'
            f'int F(int x) {{ if (x > {i}) {{ return x * 2 + f{i}; }} else {{ for (int i = 0; i < x; i++) {{ x += i; }} return x; }} }}\n'
            f'void Q() {{ var xs = from a in new[] {{ {i}, 2, 3 }} where a > 1 select a * 2; Func<int, int> g = y => y + {i}; g(1); }}\n'
            '} }\n'
        )
        parts.append(part)
        length += len(part)
        i += 1
    return ''.join(parts)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--rounds', type=int, default=7)
    parser.add_argument('--report', default='neon-scanner.json', help='JSON basename under target/reports/')
    parser.add_argument('--binaries', type=Path, default=ROOT / 'target/neon-audit')
    args = parser.parse_args()
    if args.rounds < 3:
        parser.error('--rounds must be at least 3')
    os.chdir(ROOT)
    names = ['default', 'scalar', 'neon']
    binaries = {n: args.binaries / n / 'aarch64-apple-darwin/release/nitride-compiler-audit' for n in names}
    variants = {n: {'file': str(p.relative_to(ROOT) if p.is_relative_to(ROOT) else p),
                    'sha256': sha(p), 'bytes': p.stat().st_size,
                    'features': [] if n == 'default' else [f'nitride-core/{"ascii-runs" if n == "scalar" else "simd-ascii-runs"}']}
                for n, p in binaries.items()}
    for name, binary in binaries.items():
        fingerprints = list((binary.parent / '.fingerprint').glob('nitride-core-*/lib-nitride_core.json'))
        if len(fingerprints) != 1:
            raise RuntimeError(f'Expected one core fingerprint for {name}: {fingerprints}')
        fingerprint = json.loads(fingerprints[0].read_text())
        variants[name]['actual_rustflags'] = fingerprint['rustflags']
        variants[name]['actual_core_features'] = json.loads(fingerprint['features'])
        expected = [] if name == 'default' else ['ascii-runs'] if name == 'scalar' else ['ascii-runs', 'simd-ascii-runs']
        if fingerprint['rustflags'] or variants[name]['actual_core_features'] != expected:
            raise RuntimeError(f'Unexpected flags/features for {name}: {variants[name]}')
    fixtures = ROOT / 'target/neon-audit/inputs'
    fixtures.mkdir(parents=True, exist_ok=True)
    mixed = generated(256 << 10)
    one = mixed.index(';', len(mixed) // 2)
    cases = [
        ('256k-valid', mixed, 'strict'),
        ('4m-valid', generated(4 << 20), 'strict'),
        ('256k-one', mixed[:one] + mixed[one + 1:], 'recover'),
        ('256k-dense', mixed.replace('return x;', 'return x'), 'recover'),
        ('string-1m', 'class C { string s = "' + 'a' * (1 << 20) + '"; }', 'strict'),
        ('comment-1m', '/*' + 'a' * (1 << 20) + '*/ class C {}', 'strict'),
        ('identifier-64k', 'class ' + 'a' * (64 << 10) + ' {}', 'strict'),
        ('unicode-escapes', 'class C { string s = "' + ('a' * 63 + 'é😀\\n') * 4096 + '"; }', 'strict'),
    ]
    def command(*cmd):
        return subprocess.check_output(cmd, text=True).strip()
    sources = ['crates/nitride-core/Cargo.toml', 'crates/nitride-core/src/lexical.rs',
               'crates/nitride-core/src/lexical/runs.rs', 'scripts/benchmark_neon.py',
               'scripts/compiler-audit/Cargo.toml', 'scripts/compiler-audit/Cargo.lock',
               'scripts/compiler-audit/build.rs', 'scripts/compiler-audit/src/main.rs']
    report = {
        'date': datetime.now(timezone.utc).isoformat(),
        'revision': command('git', 'rev-parse', 'HEAD'),
        'tracked_diff': command('git', 'diff', '--stat'),
        'source_sha256': {p: sha(p) for p in sources},
        'host': platform.platform(), 'cpu': command('sysctl', '-n', 'machdep.cpu.brand_string'),
        'toolchain': command('rustc', '-vV'), 'variants': variants,
        'method': f'{args.rounds} rotating/reversing fresh-process rounds plus one untimed warmup per case/variant. Portable release, LTO, ordinary allocator, no PGO/native flags or affinity pinning. Parse timing excludes I/O, export, fingerprinting and process launch. All samples retained.',
        'cases': [], 'mismatches': [],
    }
    for case_id, source, mode in cases:
        file = fixtures / f'{case_id}.cs'
        file.write_bytes(source.encode())
        row = {'id': case_id, 'mode': mode, 'file': str(file.relative_to(ROOT)),
               'bytes': file.stat().st_size, 'sha256': sha(file),
               'samples': {n: [] for n in names}}
        expected = None
        for round_id in range(-1, args.rounds):
            order = names[::-1] if round_id % 2 == 0 else names[:]
            rotate = max(round_id, 0) % len(order)
            order = order[rotate:] + order[:rotate]
            for name in order:
                sample = json.loads(command(str(binaries[name]), mode, str(file)))
                signature = (sample['fingerprint'], sample['nodes'], sample['diagnostics'], sample['work'])
                if expected is None:
                    expected = signature
                if signature != expected or (mode == 'strict' and sample['diagnostics'] != 0):
                    report['mismatches'].append({'case': case_id, 'variant': name, 'round': round_id,
                                                 'expected': expected, 'actual': signature})
                if round_id >= 0:
                    row['samples'][name].append(sample)
        row['median_ns'] = {n: statistics.median(s['elapsed_ns'] for s in samples)
                            for n, samples in row['samples'].items()}
        report['cases'].append(row)
        write_report(args.report, report)
        print(case_id, row['median_ns'], flush=True)
    if report['mismatches']:
        raise SystemExit(f"{len(report['mismatches'])} fingerprint/diagnostic/work mismatches")


if __name__ == '__main__':
    main()
