#!/usr/bin/env python3
"""Keep Cargo, npm and release tags on one stable SemVer version."""
import argparse
import json
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
SEMVER = re.compile(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', re.ASCII)


def version():
    return tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']


def package_names():
    workspace = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']
    return {tomllib.loads((ROOT / member / 'Cargo.toml').read_text())['package']['name']
            for member in workspace['members']}


def check(tag=None):
    expected = version()
    if not SEMVER.fullmatch(expected):
        raise ValueError(f'invalid stable SemVer: {expected}')
    package = json.loads((ROOT / 'editors/vscode/package.json').read_text())
    lock = json.loads((ROOT / 'editors/vscode/package-lock.json').read_text())
    versions = [package['version'], lock['version'], lock['packages']['']['version']]
    names = package_names()
    locked = tomllib.loads((ROOT / 'Cargo.lock').read_text())
    local = {p['name']: p['version'] for p in locked['package']
             if p['name'] in names and 'source' not in p}
    if local.keys() != names or any(v != expected for v in [*versions, *local.values()]):
        raise ValueError('version mismatch: run python3 scripts/version.py set VERSION')
    if tag is not None and tag != f'v{expected}':
        raise ValueError(f'tag {tag!r} does not match v{expected}')
    return expected


def set_version(new):
    if not SEMVER.fullmatch(new):
        raise ValueError('expected stable SemVer MAJOR.MINOR.PATCH, without a v prefix')
    # Validate all inputs before making any changes.
    check()
    cargo = ROOT / 'Cargo.toml'
    text = cargo.read_text()
    start = text.index('[workspace.package]')
    end = text.find('\n[', start + 1)
    end = len(text) if end == -1 else end
    section = re.sub(r'(?m)^version = "[^"]+"$', f'version = "{new}"', text[start:end])
    updates = {cargo: text[:start] + section + text[end:]}
    names = package_names()
    lockfile = ROOT / 'Cargo.lock'
    blocks = lockfile.read_text().split('[[package]]')
    for index in range(1, len(blocks)):
        parsed = tomllib.loads(blocks[index])
        if parsed['name'] in names and 'source' not in parsed:
            blocks[index] = re.sub(r'(?m)^version = "[^"]+"$', f'version = "{new}"', blocks[index], count=1)
    updates[lockfile] = '[[package]]'.join(blocks)
    for name in ['package.json', 'package-lock.json']:
        path = ROOT / 'editors/vscode' / name
        data = json.loads(path.read_text())
        data['version'] = new
        if name == 'package-lock.json':
            data['packages']['']['version'] = new
        updates[path] = json.dumps(data, indent=2) + '\n'
    for path, contents in updates.items():
        path.write_text(contents)
    check()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('check').add_argument('--tag')
    commands.add_parser('set').add_argument('version')
    args = parser.parse_args()
    try:
        if args.command == 'set':
            set_version(args.version)
        print(check(getattr(args, 'tag', None)))
    except (ValueError, KeyError) as error:
        parser.exit(1, f'{error}\n')


if __name__ == '__main__':
    main()
