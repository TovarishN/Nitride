#!/usr/bin/env python3
"""Build target-local PGO payloads; run on the target OS/architecture (or in QEMU)."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import version
from release.train import train

ROOT = Path(__file__).resolve().parents[2]
MATRIX = json.loads((Path(__file__).with_name('matrix.json')).read_text())['include']
AVX2_FEATURES = '+avx2,+fma,+bmi1,+bmi2,+lzcnt,+popcnt'


def run(command, **kwargs):
    print('+', ' '.join(map(str, command)), flush=True)
    return subprocess.run(list(map(str, command)), cwd=ROOT, check=True, **kwargs)


def output(command):
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def profdata_tool(explicit):
    if explicit:
        return explicit
    host = next(line.removeprefix('host: ') for line in output(['rustc', '-vV']).splitlines()
                if line.startswith('host: '))
    suffix = '.exe' if os.name == 'nt' else ''
    path = Path(output(['rustc', '--print', 'sysroot'])) / 'lib/rustlib' / host / 'bin' / f'llvm-profdata{suffix}'
    if not path.is_file():
        raise RuntimeError('missing compiler-matched llvm-profdata; install rustup component add llvm-tools-preview')
    return str(path)


def require_profiles(folder, pattern):
    profiles = sorted(p for p in folder.glob(pattern) if p.is_file() and p.stat().st_size)
    if not profiles:
        raise RuntimeError(f'no non-empty optimization profiles in {folder}')
    return profiles


def bolt_optimize(binary, work, env, tool, merge):
    data = work / 'bolt-data'
    data.mkdir()
    instrumented = work / 'nitride-bolt-instrumented'
    run([tool, binary, '-instrument', '-o', instrumented,
         f'-instrumentation-file={data / "profile"}', '-instrumentation-file-append-pid'])
    train(instrumented, env)
    profiles = require_profiles(data, '*')
    merged = work / 'merged.fdata'
    with merged.open('wb') as stream:
        run([merge, *profiles], stdout=stream)
    optimized = work / 'nitride-bolt'
    run([tool, binary, '-o', optimized, f'-data={merged}', '-reorder-blocks=ext-tsp',
         '-reorder-functions=hfsort+', '-split-functions', '-split-all-cold', '-dyno-stats'])
    train(optimized, env)
    return optimized


def build(args):
    item = next(row for row in MATRIX if row['id'] == args.platform)
    current = version.check()
    work = ROOT / 'target/native-release' / item['id']
    work.mkdir(parents=True, exist_ok=True)
    payload = work / 'payload'
    if payload.exists():
        shutil.rmtree(payload)
    payload.mkdir()
    profdata = profdata_tool(args.profdata)
    target = item['target']
    compiler = output(['rustc', '-vV'])
    if f'host: {target}' not in compiler.splitlines():
        raise RuntimeError(f'{target} must be built and trained in its target userspace; wrong rustc host')
    is_x64 = target.startswith('x86_64-')
    suffix = '.exe' if 'windows' in target else ''
    # Explicit target keeps target flags off host build scripts/proc macros.
    base = ['-Copt-level=3', '-Ccodegen-units=1', '-Clto=fat']
    if is_x64:
        base += ['-Ctarget-cpu=x86-64']
    elif target == 'aarch64-apple-darwin':
        base += ['-Ctarget-cpu=apple-m1']
    else:
        base += ['-Ctarget-cpu=generic']
    if item.get('bolt'):
        base += ['-Clink-arg=-Wl,--emit-relocs']
    # Prove Wasmtime cannot enter the release dependency graph.
    graph = output(['cargo', 'tree', '--locked', '-p', 'nitride-cli', '--no-default-features', '--target', target])
    if 'wasmtime' in graph or 'nitride-wasm' in graph:
        raise RuntimeError('WebAssembly runtime found in release dependency graph')
    variants = {}
    for variant in (['baseline', 'avx2'] if is_x64 else ['baseline']):
        variant_work = work / variant
        if variant_work.exists():
            shutil.rmtree(variant_work)
        variant_work.mkdir()
        raw = variant_work / 'raw'
        raw.mkdir()
        flags = base + ([f'-Ctarget-feature={AVX2_FEATURES}'] if variant == 'avx2' else [])
        env = os.environ.copy()
        env.pop('RUSTFLAGS', None)
        env['CARGO_TARGET_DIR'] = str(work / 'build' / variant)
        env['LLVM_PROFILE_FILE'] = str(raw / 'nitride-%m-%p.profraw')
        env['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join([*flags, f'-Cprofile-generate={raw}'])
        command = ['cargo', 'build', '--locked', '--profile', 'dist', '--target', target,
                   '-p', 'nitride-cli', '--bin', 'nitride', '--no-default-features']
        run(command, env=env)
        binary = Path(env['CARGO_TARGET_DIR']) / target / 'dist' / f'nitride{suffix}'
        train(binary, env)
        profiles = require_profiles(raw, '*.profraw')
        merged = variant_work / 'merged.profdata'
        run([profdata, 'merge', '-o', merged, *profiles])
        env['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join([*flags, f'-Cprofile-use={merged}',
                                                        '-Cllvm-args=-pgo-warn-missing-function'])
        env.pop('LLVM_PROFILE_FILE')
        run(command, env=env)
        # Validate the actual final payload, including LSP and recovery.
        train(binary, env)
        if item.get('bolt'):
            binary = bolt_optimize(binary, variant_work, env, args.bolt, args.merge_fdata)
        destination = payload / f'nitride-{variant}{suffix}' if is_x64 else payload / f'nitride{suffix}'
        shutil.copy2(binary, destination)
        if 'windows' not in target:
            run(['strip', destination])
        if 'apple' in target:
            run(['codesign', '--force', '--sign', '-', destination])
        found = output([str(destination), '--version'])
        if found != f'Nitride {current}':
            raise RuntimeError(f'wrong binary version: {found}')
        variants[variant] = {'rustflags': flags, 'pgo': True, 'bolt': bool(item.get('bolt')),
                             'profile_sha256': hashlib.sha256(merged.read_bytes()).hexdigest()}
    if is_x64:
        # This process must remain compatible with CPUs that have no AVX instructions.
        run(['rustc', '--edition=2024', '-O', '-Ctarget-cpu=x86-64', '--target', target,
             ROOT / 'scripts/release/launcher.rs', '-o', payload / f'nitride{suffix}'])
        if 'apple' in target:
            run(['codesign', '--force', '--sign', '-', payload / f'nitride{suffix}'])
    train(payload / f'nitride{suffix}', os.environ.copy())
    metadata = {'version': current, 'platform': item['id'], 'target': target,
                'commit': output(['git', 'rev-parse', 'HEAD']), 'rustc': compiler,
                'source_dirty': subprocess.run(['git', 'diff', '--quiet'], cwd=ROOT).returncode != 0,
                'wasmtime': False, 'variants': variants}
    (payload / 'release.json').write_text(json.dumps(metadata, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('platform', choices=[row['id'] for row in MATRIX])
    parser.add_argument('--profdata', help='override compiler-matched llvm-profdata path')
    parser.add_argument('--bolt', default='llvm-bolt-18')
    parser.add_argument('--merge-fdata', default='merge-fdata-18')
    args = parser.parse_args()
    build(args)


if __name__ == '__main__':
    main()
