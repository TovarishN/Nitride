#!/usr/bin/env python3
"""Build target-local PGO payloads (or, with --no-pgo, plain optimized ones); run on the target OS/architecture."""
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


def sysroot_profdata(rustc):
    host = next(line.removeprefix('host: ') for line in output([rustc, '-vV']).splitlines()
                if line.startswith('host: '))
    suffix = '.exe' if os.name == 'nt' else ''
    return Path(output([rustc, '--print', 'sysroot'])) / 'lib/rustlib' / host / 'bin' / f'llvm-profdata{suffix}'


def profdata_tool(explicit):
    if explicit:
        return explicit
    path = sysroot_profdata('rustc')
    if path.is_file():
        return str(path)
    # A rustc without llvm-tools (Homebrew's, say) may shadow rustup's toolchain on PATH.
    # Build with rustup's toolchain instead, so the compiler and its profdata stay matched.
    try:
        rustc = Path(output(['rustup', 'which', 'rustc']))
    except (OSError, subprocess.CalledProcessError):
        rustc = None
    if rustc and sysroot_profdata(rustc).is_file():
        print(f'+ using rustup toolchain {rustc.parent} for its llvm-profdata', flush=True)
        os.environ['PATH'] = os.pathsep.join([str(rustc.parent), os.environ['PATH']])
        return str(sysroot_profdata('rustc'))
    raise RuntimeError('missing compiler-matched llvm-profdata; install rustup component add llvm-tools-preview')


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


def release_folder(platform, pgo=True):
    """A platform's folder in target/native-release; a build without PGO keeps its own, so neither evicts the other."""
    return platform if pgo else f'{platform}-no-pgo'


def build(args):
    item = next(row for row in MATRIX if row['id'] == args.platform)
    current = version.check()
    work = ROOT / 'target/native-release' / release_folder(item['id'], args.pgo)
    work.mkdir(parents=True, exist_ok=True)
    payload = work / 'payload'
    if payload.exists():
        shutil.rmtree(payload)
    payload.mkdir()
    profdata = profdata_tool(args.profdata) if args.pgo else None
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
    # BOLT reorders by a sampled profile, so it only follows a PGO build.
    bolt = args.pgo and item.get('bolt')
    if bolt:
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
        command = ['cargo', 'build', '--locked', '--profile', 'dist', '--target', target,
                   '-p', 'nitride-cli', '--bin', 'nitride', '--no-default-features']
        binary = Path(env['CARGO_TARGET_DIR']) / target / 'dist' / f'nitride{suffix}'
        merged = None
        if args.pgo:
            env['LLVM_PROFILE_FILE'] = str(raw / 'nitride-%m-%p.profraw')
            env['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join([*flags, f'-Cprofile-generate={raw}'])
            run(command, env=env)
            train(binary, env)
            profiles = require_profiles(raw, '*.profraw')
            merged = variant_work / 'merged.profdata'
            run([profdata, 'merge', '-o', merged, *profiles])
            env['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join([*flags, f'-Cprofile-use={merged}',
                                                            '-Cllvm-args=-pgo-warn-missing-function'])
            env.pop('LLVM_PROFILE_FILE')
        else:
            env['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join(flags)
        run(command, env=env)
        # Validate the actual final payload, including LSP and recovery.
        train(binary, env)
        if bolt:
            binary = bolt_optimize(binary, variant_work, env, args.bolt, args.merge_fdata)
        destination = payload / f'nitride-{variant}{suffix}' if is_x64 else payload / f'nitride{suffix}'
        shutil.copy2(binary, destination)
        # A post-BOLT strip rewrites ELF segments and corrupts the binary.
        # Keep BOLT's verified output intact, including its symbol table.
        if 'windows' not in target and not bolt:
            run(['strip', destination])
        if 'apple' in target:
            run(['codesign', '--force', '--sign', '-', destination])
        found = output([str(destination), '--version'])
        if found != f'Nitride {current}':
            raise RuntimeError(f'wrong binary version: {found}')
        variants[variant] = {'rustflags': flags, 'pgo': args.pgo, 'bolt': bool(bolt),
                             'profile_sha256': merged and hashlib.sha256(merged.read_bytes()).hexdigest()}
    if is_x64:
        # This process must remain compatible with CPUs that have no AVX instructions.
        launcher = work / 'launcher' / f'nitride{suffix}'
        launcher.parent.mkdir(exist_ok=True)
        run(['rustc', '--edition=2024', '-O', '-Clto=fat', '-Ccodegen-units=1', '-Cpanic=abort',
             '-Ctarget-cpu=x86-64', '--target', target,
             ROOT / 'scripts/release/launcher.rs', '-o', launcher])
        shutil.copy2(launcher, payload / f'nitride{suffix}')
        if 'apple' in target:
            run(['codesign', '--force', '--sign', '-', payload / f'nitride{suffix}'])
    train(payload / f'nitride{suffix}', os.environ.copy())
    metadata = {'version': current, 'platform': item['id'], 'target': target,
                'commit': output(['git', 'rev-parse', 'HEAD']), 'rustc': compiler,
                'source_dirty': bool(output(['git', 'status', '--porcelain', '--untracked-files=normal'])),
                'wasmtime': False, 'variants': variants}
    (payload / 'release.json').write_text(json.dumps(metadata, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('platform', choices=[row['id'] for row in MATRIX])
    parser.add_argument('--no-pgo', dest='pgo', action='store_false',
                        help='one optimized build without profile training (and so without BOLT); not for releases')
    parser.add_argument('--profdata', help='override compiler-matched llvm-profdata path')
    parser.add_argument('--bolt', default='llvm-bolt-23')
    parser.add_argument('--merge-fdata', default='merge-fdata-23')
    args = parser.parse_args()
    build(args)


if __name__ == '__main__':
    main()
