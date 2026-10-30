import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
const reportPath = 'docs/compiler-optimization-audit.json';
const report = JSON.parse(fs.readFileSync(reportPath));
const sha = p => crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
for (const [name, variant] of Object.entries(report.variants)) {
  const dir = `target/compiler-audit/${name}/x86_64-pc-windows-msvc/release`;
  const exe = path.join(dir, 'nitride-compiler-audit.exe');
  if (sha(exe) !== variant.binary_sha256) throw new Error(`Binary changed since measurement: ${name}`);
  variant.binary_bytes = fs.statSync(exe).size;
  const fingerprintDir = path.join(dir, '.fingerprint');
  const fingerprint = fs.readdirSync(fingerprintDir).map(f => path.join(fingerprintDir, f, 'bin-nitride-compiler-audit.json')).find(p => fs.existsSync(p));
  variant.actual_rustflags = JSON.parse(fs.readFileSync(fingerprint)).rustflags;
}
report.harness_sources = Object.fromEntries(['Cargo.toml', 'Cargo.lock', 'build.rs', 'src/main.rs'].map(f => [f, sha(path.join('scripts/compiler-audit', f))]));
report.training = { bytes_target: 64 << 10, patterns: ['valid', 'one', 'junk', 'dense'],
  profile: 'Portable instrumentation profile shared by both PGO builds; includes untimed export and fingerprint validation.',
  profile_sha256: sha('target/compiler-audit/training.profdata') };
fs.writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n');
const primary = report.cases.filter(c => !['long-trivia', 'unicode'].includes(c.id));
const reductions = primary.map(c => 100 * (1 - c.median_ns.pgo / c.median_ns.portable));
const native = primary.map(c => 100 * (1 - c.median_ns.native / c.median_ns.portable));
const runs = report.cases.reduce((n,c) => n + Object.values(c.samples).reduce((m,s) => m + s.length, 0), 0);
const table = report.cases.map(c => `| ${c.id} | ${Object.values(c.median_ns).map(ns => (ns/1e6).toFixed(2)).join(' | ')} | ${(100*(1-c.median_ns.pgo/c.median_ns.portable)).toFixed(1)}% |`).join('\n');
const pgoGrowth = 100*(report.variants.pgo.binary_bytes/report.variants.portable.binary_bytes-1);
const md = `# Rust compiler optimization audit

Measured ${report.date.slice(0,10)} on ${report.cpu}, Windows x86-64, ${report.toolchain.split('\n')[0]}. Source revision: \`${report.revision}\`.

PGO reduces parse-phase time by ${Math.min(...reductions).toFixed(1)}–${Math.max(...reductions).toFixed(1)}% across the eight primary CSharp4 cases. Native CPU targeting ranges from ${Math.abs(Math.min(...native)).toFixed(1)}% slower to ${Math.max(...native).toFixed(1)}% faster; it has no consistent advantage. Combining native targeting with PGO provides no consistent extra improvement. The PGO harness executable is ${pgoGrowth.toFixed(1)}% larger (${report.variants.portable.binary_bytes.toLocaleString('en-US')} → ${report.variants.pgo.binary_bytes.toLocaleString('en-US')} bytes; these are harness binaries, not shipping CLI sizes).

## Parse-phase medians

Milliseconds; five fresh processes per cell. “PGO reduction” is elapsed-time reduction, not throughput increase.

| Input (target bytes / pattern) | Portable | Native CPU | PGO | Native + PGO | PGO reduction |
|---|---:|---:|---:|---:|---:|
${table}

## Equivalence and method

${runs} timed executions, plus one untimed process per case/build. Tree/diagnostic fingerprint mismatches: **${report.mismatches.length}**. Fingerprints cover node rules/spans/order, labeled edges, diagnostics and recovery records. All four builds use the same generated grammar, limits and ordinary allocator. Parse time includes parser setup and syntax construction, but excludes input I/O, export, fingerprinting, process launch and destruction. Logical work/native-call/fallback counters and every raw sample are retained in [the JSON report](compiler-optimization-audit.json).

Build order rotates and reverses between rounds. CPU affinity is not pinned on this hybrid-core host. Five samples and one host support the sizeable PGO result; percent-level native differences should not be treated as reliable wins. The 80-byte Unicode case is a correctness check, not a useful SIMD throughput benchmark. Long-trivia input has a large comment and string, but very little syntax.

PGO trains on separate 64 KiB valid/one-error/junk/dense files from the same generator family, including harness export/fingerprint validation. One portable profile is shared by both PGO builds. No missing-function or profile-mismatch warnings were emitted. Training does not establish generalization to other grammars or production workload distributions. Deployment PGO should train the actual deployed executable. This run does not measure incremental edits, peak memory or a new Roslyn comparison.

## SIMD findings

Both portable and native assembly contain 59 generated lexical functions with unwind frames. Neither set uses YMM registers or the inspected packed comparison instructions. Portable lexical code uses 446 XMM-register instruction sites, including 432 vector moves. Native lexical code has the same counts. This supports the source-level finding that generated character recognition remains scalar.

The native build does use YMM registers elsewhere: 10,720 static instruction sites overall, including 6,098 in functions associated with generated syntax. This demonstrates AVX register use, largely for support work; it does not demonstrate vectorized token recognition. Inspection counts static instruction sites, not CPU samples; inlined operations belong to their enclosing functions. It checks selected packed comparison mnemonics and does not prove the absence of every possible SIMD algorithm. See [portable assembly summary](compiler-optimization-assembly-portable.json) and [native summary](compiler-optimization-assembly-native.json).

The runtime already delegates comment-delimiter searches to standard string search. Generated lexical repetition still performs stateful per-character work, observation, depth and budget tracking. Enabling AVX2 does not transform that design into a bulk scanner. Explicit SIMD would require a new bulk-scanning path that preserves UTF-8 boundaries, observations, partial failures and resource-limit behavior.

## Result

Keep the portable release defaults. PGO is a demonstrated next deployment optimization, subject to production training and incremental-workload validation. CPU targeting alone does not justify a portability change here. No handwritten SIMD scanner is added from these measurements: they establish scalar recognition and a PGO opportunity, but do not measure an end-to-end gain from a SIMD replacement. Further SIMD work needs scanner-specific profiling and a compared prototype.

The only repository additions are the audit harness, inspection scripts and evidence. No production parser, generator or release defaults are changed. Reproduction instructions: [compiler-audit README](../scripts/compiler-audit/README.md).
`;
fs.writeFileSync('docs/compiler-optimization-audit.md', md);
console.log(JSON.stringify({ cases: report.cases.length, timed_executions: runs, mismatches: report.mismatches.length, pgo_reduction_percent: [Math.min(...reductions), Math.max(...reductions)] }));
if (report.cases.length !== 10 || report.mismatches.length) process.exitCode = 1;
