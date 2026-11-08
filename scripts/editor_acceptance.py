"""Exercise an extracted native VSIX in a disposable VS Code extension host."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import signal
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def extract(vsix, destination):
    with zipfile.ZipFile(vsix) as archive:
        for entry in archive.infolist():
            path = PurePosixPath(entry.filename)
            if path.is_absolute() or '..' in path.parts or '\\' in entry.filename:
                raise ValueError('unsafe VSIX path')
            if (entry.external_attr >> 16) & 0o170000 == 0o120000:
                raise ValueError('unsafe VSIX symbolic link path')
        archive.extractall(destination)
    extension = destination / 'extension'
    server = extension / 'server' / ('nitride.exe' if os.name == 'nt' else 'nitride')
    if not server.is_file() or not (extension / 'server/release.json').is_file():
        raise ValueError('VSIX must contain a bundled native server and release metadata')
    if os.name != 'nt':
        for binary in (extension / 'server').glob('nitride*'):
            binary.chmod(binary.stat().st_mode | 0o100)
    return extension


def fixture(root, repository_languages=False):
    workspace = root / 'workspace'
    if repository_languages:
        shutil.copytree(ROOT / 'languages', workspace / 'languages')
        shutil.copy2(ROOT / 'nitride.nproj', workspace / 'nitride.nproj')
    else:
        shutil.copytree(ROOT / 'languages/portable', workspace / 'portable')
        (workspace / 'nitride.nproj').write_text('workspace { include portable; }\n')
    for name in ['one', 'two']:
        (workspace / f'{name}.portable').write_text(f'let {name} : Scalar = 1;\n{name};\n')
    return workspace


def run_host(command, env, timeout=180):
    process = subprocess.Popen(command, env=env, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, start_new_session=os.name != 'nt')
    try:
        log, _ = process.communicate(timeout=timeout)
        return log, process.returncode
    finally:
        if process.poll() is None:
            try:
                if os.name == 'nt':
                    subprocess.run(['taskkill', '/PID', str(process.pid), '/T', '/F'], check=False,
                                   capture_output=True, timeout=5)
                else:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass  # It exited between poll and kill.
            finally:
                process.communicate(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--code', type=Path, required=True, help='VS Code application executable')
    parser.add_argument('--vsix', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--repository-languages', action='store_true',
                        help='load all repository languages instead of only Portable')
    parser.add_argument('--trace', action='store_true', help='enable language-client protocol logging')
    args = parser.parse_args()
    report = {'complete': False,
              'metric': 'VS Code extension-host API observations, not painted-frame or visual acceptance'}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    try:
        run(args, report)
    except BaseException as error:
        report['complete'] = False
        report['error'] = f'{type(error).__name__}: {error}'
        raise
    finally:
        args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(args.output)


def run(args, report):
    report['vsix_sha256'] = hashlib.sha256(args.vsix.read_bytes()).hexdigest()
    report['code'] = str(args.code.resolve())
    # macOS Unix-domain IPC sockets have a short path limit.
    with tempfile.TemporaryDirectory(prefix='nre-') as directory:
        root = Path(directory)
        extension = extract(args.vsix, root / 'unpacked')
        workspace = fixture(root, args.repository_languages)
        report['workspace'] = 'repository-languages' if args.repository_languages else 'portable-only'
        report['release'] = json.loads((extension / 'server/release.json').read_text())
        server = extension / 'server' / ('nitride.exe' if os.name == 'nt' else 'nitride')
        report['bundled_server_sha256'] = hashlib.sha256(server.read_bytes()).hexdigest()
        result = root / 'result.json'
        env = os.environ.copy()
        env.pop('ELECTRON_RUN_AS_NODE', None)
        env['NITRIDE_ACCEPTANCE_RESULT'] = str(result)
        env['NITRIDE_ACCEPTANCE_EXTENSION'] = str(extension)
        env['NITRIDE_ACCEPTANCE_GRAMMAR'] = str(workspace / (
            'languages/portable/Portable.ngr' if args.repository_languages else 'portable/Portable.ngr'))
        settings = root / 'user/User/settings.json'
        settings.parent.mkdir(parents=True)
        settings.write_text(json.dumps({'telemetry.telemetryLevel': 'off', 'update.mode': 'none',
                                       'extensions.autoUpdate': False, 'extensions.autoCheckUpdates': False,
                                       'nitride.trace.server': 'verbose' if args.trace else 'off'}))
        command = [str(args.code.resolve()), '--new-window', '--wait',
                   '--user-data-dir', str(root / 'user'), '--extensions-dir', str(root / 'extensions'),
                   '--skip-welcome', '--skip-release-notes', '--disable-workspace-trust',
                   '--extensionDevelopmentPath=' + str(extension),
                   '--extensionTestsPath=' + str(ROOT / 'editors/vscode/acceptance.cjs'), str(workspace)]
        log, returncode = run_host(command, env)
        if result.exists():
            report.update(json.loads(result.read_text()))
        if not report['complete']:
            report['extension_logs'] = {str(p.relative_to(root / 'user')): p.read_text(errors='replace')
                                        for p in (root / 'user/logs').rglob('*.log')
                                        if 'output_logging' in str(p)}
        report['host_exit'] = returncode
        report['log'] = log.decode(errors='replace')
    if returncode or not report['complete']:
        raise RuntimeError(f'VS Code acceptance failed; see {args.output}')


if __name__ == '__main__':
    main()
