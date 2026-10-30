#!/usr/bin/env python3
"""Stage exactly one target, package its VSIX/archive, and verify release assets."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import xml.etree.ElementTree as ET
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import version
from release.build import MATRIX, ROOT


def binary_names(platform):
    suffix = '.exe' if platform.startswith('win32-') else ''
    variants = ['nitride', 'nitride-baseline', 'nitride-avx2'] if platform.endswith('-x64') else ['nitride']
    return {name + suffix for name in variants}


def verify_metadata(data, platform, current):
    if data['platform'] != platform or data['version'] != current or data['wasmtime'] is not False:
        raise ValueError(f'invalid release metadata for {platform}')
    wanted = {'baseline', 'avx2'} if platform.endswith('-x64') else {'baseline'}
    if data['variants'].keys() != wanted or any(not row['pgo'] for row in data['variants'].values()):
        raise ValueError(f'missing PGO variants for {platform}')
    needs_bolt = next(row for row in MATRIX if row['id'] == platform).get('bolt', False)
    if any(row['bolt'] != needs_bolt for row in data['variants'].values()):
        raise ValueError(f'wrong BOLT optimization for {platform}')


def verify_provenance(data, commit):
    if data.get('commit') != commit or data.get('source_dirty') is not False:
        raise ValueError('release assets must come from the exact clean tagged commit')


def verify_vsix(path, platform, current):
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        if len(set(names)) != len(names) or any(name.endswith('.wasm') for name in names):
            raise ValueError('duplicate ZIP entries or WebAssembly in VSIX')
        server = {name.removeprefix('extension/server/') for name in names
                  if name.startswith('extension/server/') and not name.endswith('/')}
        if server != binary_names(platform) | {'release.json'}:
            raise ValueError(f'unexpected server payload in {path}: {server}')
        package = json.loads(archive.read('extension/package.json'))
        if package['version'] != current or 'browser' in package:
            raise ValueError('wrong extension version or web extension')
        manifest = ET.fromstring(archive.read('extension.vsixmanifest'))
        identity = next(element for element in manifest.iter() if element.tag.endswith('Identity'))
        if identity.attrib.get('TargetPlatform') != platform or identity.attrib['Version'] != current:
            raise ValueError('VSIX target/version mismatch')
        verify_metadata(json.loads(archive.read('extension/server/release.json')), platform, current)
        for name in binary_names(platform):
            if not archive.getinfo('extension/server/' + name).file_size:
                raise ValueError('empty executable in VSIX')


def stem(platform, current):
    return f'nitride-{current}-{platform}'


def package(platform, out):
    # Container builds leave root-owned payload directories on hosted Linux.
    # Stage on the packaging host instead of writing into that directory.
    with tempfile.TemporaryDirectory(prefix='nitride-package-') as directory:
        package_in(platform, out, Path(directory))


def package_in(platform, out, work):
    current = version.check()
    payload = ROOT / 'target/native-release' / platform / 'payload'
    metadata = json.loads((payload / 'release.json').read_text())
    verify_metadata(metadata, platform, current)
    if {p.name for p in payload.iterdir()} != binary_names(platform) | {'release.json'}:
        raise ValueError(f'stale or unexpected payload files in {payload}')
    out.mkdir(parents=True, exist_ok=True)
    extension = work / 'vscode'
    extension.mkdir()
    source = ROOT / 'editors/vscode'
    for name in ['package.json', 'README.md', 'LICENSE', 'language-configuration.json']:
        shutil.copy2(source / name, extension / name)
    shutil.copytree(source / 'syntaxes', extension / 'syntaxes')
    (extension / 'out').mkdir()
    shutil.copy2(source / 'out/extension.js', extension / 'out/extension.js')
    shutil.copytree(payload, extension / 'server')
    vsix = out / f'{stem(platform, current)}.vsix'
    subprocess.run(['node', str(source / 'node_modules/@vscode/vsce/vsce'), 'package',
                    '--no-dependencies', '--allow-missing-repository', '--target', platform,
                    '--out', str(vsix)], cwd=extension, check=True)
    verify_vsix(vsix, platform, current)
    folder = work / stem(platform, current)
    shutil.copytree(payload, folder)
    shutil.copy2(ROOT / 'README.md', folder / 'README.md')
    shutil.copy2(source / 'LICENSE', folder / 'LICENSE')
    archive = out / f'{folder.name}{".zip" if platform.startswith("win32-") else ".tar.gz"}'
    if platform.startswith('win32-'):
        with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as stream:
            for file in sorted(folder.iterdir()):
                stream.write(file, f'{folder.name}/{file.name}')
    else:
        with tarfile.open(archive, 'w:gz') as stream:
            stream.add(folder, arcname=folder.name)
    metadata_file = out / f'{stem(platform, current)}.json'
    shutil.copy2(payload / 'release.json', metadata_file)
    assets = [vsix, archive, metadata_file]
    checksums = out / f'SHA256SUMS-{platform}.txt'
    checksums.write_text(''.join(f'{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n' for path in assets))
    verify_assets(out, [platform])


def verify_assets(out, platforms):
    current = version.check()
    expected_files = set()
    for platform in platforms:
        base = stem(platform, current)
        assets = {base + suffix for suffix in ['.vsix', '.json', '.zip' if platform.startswith('win32-') else '.tar.gz']}
        manifest = f'SHA256SUMS-{platform}.txt'
        rows = (out / manifest).read_text().splitlines()
        found = set()
        for row in rows:
            digest, name = row.split('  ', 1)
            if name not in assets or name in found:
                raise ValueError(f'invalid checksum manifest {manifest}')
            found.add(name)
            if hashlib.sha256((out / name).read_bytes()).hexdigest() != digest:
                raise ValueError(f'checksum mismatch: {name}')
        if found != assets:
            raise ValueError(f'incomplete artifacts for {platform}')
        metadata = json.loads((out / (base + '.json')).read_text())
        verify_metadata(metadata, platform, current)
        verify_vsix(out / (base + '.vsix'), platform, current)
        archive_path = out / (base + ('.zip' if platform.startswith('win32-') else '.tar.gz'))
        if platform.startswith('win32-'):
            with zipfile.ZipFile(archive_path) as stream:
                contents = {name: stream.read(name) for name in stream.namelist()}
        else:
            with tarfile.open(archive_path, 'r:gz') as stream:
                contents = {file.name: stream.extractfile(file).read() for file in stream.getmembers() if file.isfile()}
        wanted = {f'{base}/{name}' for name in binary_names(platform) | {'README.md', 'LICENSE', 'release.json'}}
        if contents.keys() != wanted:
            raise ValueError(f'unexpected CLI archive contents: {archive_path}')
        if json.loads(contents[f'{base}/release.json']) != metadata:
            raise ValueError(f'CLI archive metadata mismatch for {platform}')
        with zipfile.ZipFile(out / (base + '.vsix')) as stream:
            if json.loads(stream.read('extension/server/release.json')) != metadata:
                raise ValueError(f'VSIX metadata mismatch for {platform}')
            for name in binary_names(platform):
                if stream.read('extension/server/' + name) != contents[f'{base}/{name}']:
                    raise ValueError(f'CLI/VSIX executable mismatch for {platform}/{name}')
        expected_files |= assets | {manifest}
    if platforms == [row['id'] for row in MATRIX]:
        commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
        for platform in platforms:
            verify_provenance(json.loads((out / (stem(platform, current) + '.json')).read_text()), commit)
        if {file.name for file in out.iterdir()} - {'SHA256SUMS.txt'} != expected_files:
            raise ValueError('unexpected or missing release assets')
        (out / 'SHA256SUMS.txt').write_text(''.join((out / f'SHA256SUMS-{platform}.txt').read_text() for platform in platforms))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('platform', choices=[row['id'] for row in MATRIX] + ['verify'])
    parser.add_argument('--out', type=Path, default=ROOT / 'dist')
    args = parser.parse_args()
    out = args.out.resolve()
    if args.platform == 'verify':
        verify_assets(out, [row['id'] for row in MATRIX])
    else:
        package(args.platform, out)


if __name__ == '__main__':
    main()
