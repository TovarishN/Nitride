"""Compare existing .NET harnesses with a completed native audit (macOS).

Build rcmp/ncmp into target/reassessment/dotnet first; see README.md.
Run after all builds finish to avoid compilation during measurement.
"""
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
os.chdir(ROOT)
audit_path = Path(sys.argv[1] if len(sys.argv) > 1 else 'docs/compiler-optimization-macos.json')
output = Path(sys.argv[2] if len(sys.argv) > 2 else 'docs/compiler-comparison-macos.json')
audit = json.loads(audit_path.read_text())
artifacts = ROOT / 'target/reassessment/dotnet/bin'
tools = {'roslyn': artifacts / 'rcmp/release/rcmp.dll',
         'nitrogen': artifacts / 'ncmp-binaries/release/ncmp-binaries.dll'}


def command(args):
    return subprocess.check_output(args, text=True).strip()


def sample(name, mode, file):
    return json.loads(command(['dotnet', str(tools[name]), mode, str(file)]).splitlines()[-1])


report = {
    'date': datetime.now(timezone.utc).isoformat(),
    'native_audit': str(audit_path),
    'host': platform.platform(),
    'dotnet_info': command(['dotnet', '--info']),
    'nitrogen_checkout_revision': command(['git', '-C', '../Nitrogen', 'rev-parse', 'HEAD']),
    'method': 'Same source files as native audit. Five fresh-process first parses per case/tool; '
              'tool order alternates. One warm process per case/tool, two warmups and five timed parses. '
              'Existing stage30 harnesses; source I/O, process launch and Nitrogen language construction '
              'excluded. Managed memory accounting retained as observations, not compared with native audit. '
              'Native audit runs separately before this comparison. No affinity pinning.',
    'binaries': {name: {'path': str(file.relative_to(ROOT)),
                        'sha256': hashlib.sha256(file.read_bytes()).hexdigest()}
                 for name, file in tools.items()},
    'assemblies': {name: {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                           for p in file.parent.glob('*.dll')}
                   for name, file in tools.items()},
    'harness_sources': {str(p): hashlib.sha256(p.read_bytes()).hexdigest()
                        for p in [Path('docs/stage30-harness/rcmp/Program.cs'),
                                  Path('docs/stage30-harness/ncmp/Program.cs')]},
    'rows': [],
}
for case in audit['cases']:
    if case['id'] in ('unicode', 'long-trivia'):
        continue
    file = ROOT / case['file']
    assert hashlib.sha256(file.read_bytes()).hexdigest() == case['sha256']
    samples = {name: [] for name in tools}
    print('Measuring', case['id'], flush=True)
    for round_index in range(5):
        for name in (list(tools) if round_index % 2 else list(tools)[::-1]):
            samples[name].append(sample(name, 'cold', file))
    for name in tools:
        warm = sample(name, 'warm', file)
        assert all(s['diagnostics'] == warm['diagnostics'] for s in samples[name])
        if case['id'].endswith('-valid') or case['id'] == 'string-1m':
            assert warm['diagnostics'] == 0, (name, case['id'], warm)
            assert all(s.get('success', True) for s in samples[name])
        report['rows'].append({'case': case['id'], 'tool': name,
                               'source_sha256': case['sha256'],
                               'cold_s': statistics.median(s['seconds'] for s in samples[name]),
                               'warm_s': warm['seconds'], 'cold': samples[name], 'warm': warm})
        print(name, report['rows'][-1]['cold_s'], warm['seconds'], flush=True)
    output.write_text(json.dumps(report, indent=2) + '\n')
