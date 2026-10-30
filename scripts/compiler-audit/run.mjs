// Run from the repository root: node scripts/compiler-audit/run.mjs
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';

const root = process.cwd();
const out = path.resolve(process.env.NITRIDE_AUDIT_OUT || 'target/compiler-audit');
const reportPath = process.env.NITRIDE_AUDIT_REPORT || 'docs/compiler-optimization-audit.json';
fs.mkdirSync(out, { recursive: true });
const sha = p => crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
function command(exe, args, env = {}, log) {
  const cleanEnv = { ...process.env };
  delete cleanEnv.RUSTFLAGS;
  delete cleanEnv.CARGO_ENCODED_RUSTFLAGS;
  const r = spawnSync(exe, args, { cwd: root, env: { ...cleanEnv, ...env },
    encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  if (log) fs.writeFileSync(path.join(out, log), (r.stdout || '') + (r.stderr || ''));
  if (r.status !== 0) throw new Error(`${exe} ${args.join(' ')}\n${r.error || ''}\n${r.stderr}`);
  return r.stdout.trim();
}
const target = command('rustc', ['-vV']).match(/host: (.*)/)[1];
const sysroot = command('rustc', ['--print', 'sysroot']);
const tools = process.env.NITRIDE_LLVM_BIN || path.join(sysroot, 'lib/rustlib', target, 'bin');
const ext = process.platform === 'win32' ? '.exe' : '';
const llvm = name => path.join(tools, name + ext);
const binary = name => path.join(out, name, target, 'release', 'nitride-compiler-audit' + ext);
const manifest = 'scripts/compiler-audit/Cargo.toml';
function build(name, flags, features = []) {
  console.log(`Building ${name}`);
  command('cargo', ['build', '--offline', '--verbose', '--release', '--manifest-path', manifest,
    '--target', target, '--target-dir', path.join(out, name), ...features], { RUSTFLAGS: flags }, `${name}-build.log`);
}
// Same deterministic valid source generator as csharp4_scaling.py, LF bytes.
function generated(size) {
  let source = 'using System;\nusing System.Linq;\n';
  for (let i = 0; source.length < size; i++) source +=
    `namespace N${i} { public class C${i}<T> where T : class {\n` +
    `int f${i} = ${i};\n` +
    `public string Name { get { return "n${i}"; } set { } }\n` +
    `int F(int x) { if (x > ${i}) { return x * 2 + f${i}; } else { for (int i = 0; i < x; i++) { x += i; } return x; } }\n` +
    `void Q() { var xs = from a in new[] { ${i}, 2, 3 } where a > 1 select a * 2; Func<int, int> g = y => y + ${i}; g(1); }\n` + '} }\n';
  return source;
}
function damage(source, pattern) {
  if (pattern === 'valid') return source;
  if (pattern === 'dense') return source.replaceAll('return x;', 'return x');
  const at = source.indexOf(pattern === 'one' ? ';' : 'return', source.length >> 1);
  return pattern === 'one' ? source.slice(0, at) + source.slice(at + 1)
    : source.slice(0, at) + '@ '.repeat(64) + source.slice(at);
}
function fixture(id, source, mode) {
  const file = path.join(out, id + '.cs');
  fs.writeFileSync(file, source);
  return { id, file, mode, bytes: Buffer.byteLength(source), sha256: sha(file) };
}
const cases = [];
for (const size of [256 << 10, 4 << 20]) {
  const source = generated(size);
  for (const pattern of ['valid', 'one', 'junk', ...(size < 1 << 20 ? ['dense'] : [])])
    cases.push(fixture(`${size}-${pattern}`, damage(source, pattern), pattern === 'valid' ? 'strict' : 'recover'));
}
cases.push(fixture('16m-valid', generated(16 << 20), 'strict'));
cases.push(fixture('long-trivia', `/*${' long comment '.repeat(18000)}*/\nclass C { string s = "${'abcdef'.repeat(16000)}"; }`, 'strict'));
cases.push(fixture('unicode', '// café Ελληνικά 日本語\nclass Café { string s = "日本語😀"; }\n'.repeat(1), 'strict'));
const training = ['valid', 'one', 'junk', 'dense'].map(p => fixture(`train-${p}`, damage(generated(64 << 10), p), p === 'valid' ? 'strict' : 'recover'));
const variants = { portable: '', native: '-Ctarget-cpu=native' };
for (const [name, flags] of Object.entries(variants)) build(name, flags);
if (fs.existsSync(llvm('llvm-profdata'))) {
  const merged = path.join(out, 'training.profdata');
  if (process.env.NITRIDE_AUDIT_REUSE_PROFILE !== '1' || !fs.existsSync(merged)) {
    const profiles = path.join(out, 'profiles-' + Date.now());
    fs.mkdirSync(profiles);
    build('training', `-Cprofile-generate=${profiles}`);
    console.log('Training PGO on separate 64 KiB inputs');
    for (const c of training) command(binary('training'), [c.mode, c.file], { LLVM_PROFILE_FILE: path.join(profiles, '%m-%p.profraw') });
    command(llvm('llvm-profdata'), ['merge', '-o', merged, ...fs.readdirSync(profiles).filter(f => f.endsWith('.profraw')).map(f => path.join(profiles, f))]);
  }
  variants.pgo = `-Cprofile-use=${merged} -Cllvm-args=-pgo-warn-missing-function`;
  variants['native-pgo'] = `-Ctarget-cpu=native ${variants.pgo}`;
  build('pgo', variants.pgo);
  build('native-pgo', variants['native-pgo']);
}
if (process.env.NITRIDE_AUDIT_ASCII === '1') {
  // This feature uses scalar batching on ARM64; its explicit SIMD is x86 AVX2.
  variants['ascii-feature'] = '';
  build('ascii-feature', '', ['--features', 'nitride-core/simd-ascii-runs']);
  if (variants.pgo) {
    const profiles = path.join(out, 'ascii-profiles-' + Date.now());
    fs.mkdirSync(profiles);
    const features = ['--features', 'nitride-core/simd-ascii-runs'];
    build('ascii-training', `-Cprofile-generate=${profiles}`, features);
    for (const c of training) command(binary('ascii-training'), [c.mode, c.file], { LLVM_PROFILE_FILE: path.join(profiles, '%m-%p.profraw') });
    const merged = path.join(out, 'ascii-training.profdata');
    command(llvm('llvm-profdata'), ['merge', '-o', merged, ...fs.readdirSync(profiles).filter(f => f.endsWith('.profraw')).map(f => path.join(profiles, f))]);
    variants['ascii-feature-pgo'] = `-Cprofile-use=${merged} -Cllvm-args=-pgo-warn-missing-function`;
    build('ascii-feature-pgo', variants['ascii-feature-pgo'], features);
  }
  cases.push(fixture('string-1m', `class C { string s = "${'a'.repeat(1 << 20)}"; }`, 'strict'));
}
const report = { date: new Date().toISOString(), host: os.platform(), cpu: os.cpus()[0].model,
  toolchain: command('rustc', ['-vV']), revision: command('git', ['rev-parse', 'HEAD']),
  method: '5 fresh processes per case/variant, rotating and reversing variant order; one untimed warmup per case/variant. Ordinary allocator. Parse phase excludes source I/O, export, fingerprint and process launch. Separate 64 KiB PGO training, same generator family. LF UTF-8 input. No affinity pinning.',
  variants: Object.fromEntries(Object.entries(variants).map(([name, flags]) => [name, { flags, features: name.startsWith('ascii-feature') ? ['nitride-core/simd-ascii-runs'] : [], binary_sha256: sha(binary(name)), binary_bytes: fs.statSync(binary(name)).size }])),
  cases: [], mismatches: [] };
report.harness_sources = Object.fromEntries(['Cargo.toml', 'Cargo.lock', 'build.rs', 'src/main.rs'].map(f => [f, sha(path.join(root, 'scripts/compiler-audit', f))]));
report.training = { bytes_target: 64 << 10, patterns: ['valid', 'one', 'junk', 'dense'],
  profile: 'Portable profile shared by pgo/native-pgo; separate feature-enabled training for ascii-feature-pgo when requested. Training includes export/fingerprint validation.',
  ascii_profile_sha256: fs.existsSync(path.join(out, 'ascii-training.profdata')) ? sha(path.join(out, 'ascii-training.profdata')) : null,
  profile_sha256: fs.existsSync(path.join(out, 'training.profdata')) ? sha(path.join(out, 'training.profdata')) : null };
report.llvm_profdata_version = fs.existsSync(llvm('llvm-profdata')) ? command(llvm('llvm-profdata'), ['--version']) : null;
report.profile_warnings = Object.fromEntries(Object.keys(variants).filter(name => name.includes('pgo')).map(name => {
  const log = fs.readFileSync(path.join(out, `${name}-build.log`), 'utf8');
  return [name, {
    missing_function_count: (log.match(/no profile data available for function/g) || []).length,
    hash_mismatch_count: (log.match(/hash mismatch/gi) || []).length,
  }];
}));
const names = Object.keys(variants);
for (const c of cases) {
  console.log(`Measuring ${c.id}`);
  const row = { ...c, file: path.relative(root, c.file), samples: Object.fromEntries(names.map(n => [n, []])) };
  let expected;
  for (let round = -1; round < 5; round++) {
    let order = names.slice();
    if (round % 2 === 0) order.reverse();
    const rotate = Math.max(round, 0) % order.length;
    order = order.slice(rotate).concat(order.slice(0, rotate));
    for (const name of order) {
      const result = JSON.parse(command(binary(name), [c.mode, c.file]));
      const signature = JSON.stringify([result.fingerprint, result.nodes, result.diagnostics]);
      expected ??= signature;
      if (signature !== expected) report.mismatches.push({ case: c.id, name, round, expected, actual: signature });
      if (round >= 0) row.samples[name].push(result);
    }
  }
  row.median_ns = Object.fromEntries(names.map(name => [name, row.samples[name].map(r => r.elapsed_ns).sort((a,b) => a-b)[2]]));
  report.cases.push(row);
  fs.writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({ case: c.id, medians: row.median_ns, mismatches: report.mismatches.length }));
}
if (report.mismatches.length) process.exitCode = 1;
