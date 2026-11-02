#!/usr/bin/env python3
"""Build a binary that embeds a Nitride-generated parser with profile-guided optimization.

    python3 -I scripts/pgo_generated.py --crate path/to/crate --bin name \
        --train 'parse samples/a.json' --train 'parse samples/b.json'

Each --train is the instrumented binary's arguments, run from the crate folder
on inputs like the ones it will parse in use. The optimized binary is built
under <crate>/target/pgo/<host>/release/ and its path is printed last.
"""
import argparse
from dataclasses import dataclass
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from release.build import profdata_tool  # noqa: E402


@dataclass
class Build:
    command: list
    target_dir: Path
    rustflags: list


@dataclass
class Plan:
    raw: Path
    merged: Path
    instrument: Build
    optimize: Build
    instrumented: Path
    binary: Path


def plan(crate, bin_name, host):
    """The two builds. Both name the host as --target, so RUSTFLAGS reach only
    the target's code: build scripts, such as one that runs the parser
    generator, are neither instrumented nor profiled."""
    crate = Path(crate)
    raw = crate / 'target/pgo-raw'
    merged = raw / 'merged.profdata'

    def build(folder, flags):
        target_dir = crate / 'target' / folder
        command = ['cargo', 'build', '--release', '--bin', bin_name, '--target', host,
                   '--target-dir', str(target_dir)]
        return Build(command, target_dir, flags)

    instrument = build('pgo-gen', [f'-Cprofile-generate={raw}'])
    optimize = build('pgo', [f'-Cprofile-use={merged}', '-Cllvm-args=-pgo-warn-missing-function'])
    return Plan(raw, merged, instrument, optimize,
                instrument.target_dir / host / 'release' / bin_name,
                optimize.target_dir / host / 'release' / bin_name)


def training(plan, runs):
    return [[str(plan.instrumented), *shlex.split(run)] for run in runs]


def cargo(build, crate):
    env = os.environ.copy()
    env.pop('RUSTFLAGS', None)
    env['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join(build.rustflags)
    subprocess.run(build.command, cwd=crate, env=env, check=True)


def host_triple():
    info = subprocess.check_output(['rustc', '-vV'], text=True)
    return next(line.removeprefix('host: ') for line in info.splitlines() if line.startswith('host: '))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--crate', required=True, type=Path, help='folder holding the Cargo.toml')
    parser.add_argument('--bin', required=True, help='binary target to build')
    parser.add_argument('--train', action='append', required=True,
                        help="the instrumented binary's arguments for one training run; repeatable")
    parser.add_argument('--llvm-profdata', help='llvm-profdata matching rustc (found automatically)')
    args = parser.parse_args()
    crate = args.crate.resolve()
    profdata = profdata_tool(args.llvm_profdata)
    steps = plan(crate, args.bin, host_triple())
    shutil.rmtree(steps.raw, ignore_errors=True)
    steps.raw.mkdir(parents=True)
    cargo(steps.instrument, crate)
    env = {**os.environ, 'LLVM_PROFILE_FILE': str(steps.raw / f'{args.bin}-%m-%p.profraw')}
    for command in training(steps, args.train):
        subprocess.run(command, cwd=crate, env=env, check=True, stdout=subprocess.DEVNULL)
    profiles = sorted(p for p in steps.raw.glob('*.profraw') if p.stat().st_size)
    if not profiles:
        raise SystemExit('training wrote no profiles')
    subprocess.run([profdata, 'merge', '-o', str(steps.merged), *map(str, profiles)], check=True)
    cargo(steps.optimize, crate)
    print(steps.binary)


if __name__ == '__main__':
    main()
