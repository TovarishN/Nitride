import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
const sha = data => crypto.createHash('sha256').update(data).digest('hex');
const reportFile = 'docs/ascii-scanner-runs.json';
const r = JSON.parse(fs.readFileSync(reportFile));
const wanted = { default:[], scalar:['ascii-runs'], simd:['ascii-runs','simd-ascii-runs'] };
function sections(bytes) {
  const pe=bytes.readUInt32LE(0x3c), start=pe+24+bytes.readUInt16LE(pe+20);
  const result={};
  for(let i=0;i<bytes.readUInt16LE(pe+6);i++) {
    const at=start+i*40, name=bytes.subarray(at,at+8).toString().replace(/\0/g,'');
    const size=bytes.readUInt32LE(at+16), offset=bytes.readUInt32LE(at+20);
    result[name]=sha(bytes.subarray(offset,offset+size));
  }
  return result;
}
for(const [name,v] of Object.entries(r.variants)) {
  const bytes=fs.readFileSync(v.file);
  if(sha(bytes)!==v.sha256) throw new Error(`Measured binary changed: ${name}`);
  v.bytes=bytes.length; v.section_sha256=sections(bytes);
  if(name==='baseline')continue;
  const dir=path.join(path.dirname(v.file),'.fingerprint');
  const candidates=fs.readdirSync(dir).map(p=>path.join(dir,p,'lib-nitride_core.json')).filter(p=>fs.existsSync(p)).sort((a,b)=>fs.statSync(b).mtimeMs-fs.statSync(a).mtimeMs);
  const info=JSON.parse(fs.readFileSync(candidates[0]));
  v.features=JSON.parse(info.features); v.rustflags=info.rustflags;
  if(JSON.stringify(v.features)!==JSON.stringify(wanted[name]))throw new Error(`Unexpected ${name} features: ${v.features}`);
  if(v.rustflags.length)throw new Error(`Unexpected compiler flags for ${name}`);
}
r.source_sha256=Object.fromEntries([
  'crates/nitride-core/src/lexical.rs','crates/nitride-core/src/lexical/runs.rs',
  'crates/nitride-core/Cargo.toml','crates/nitride-codegen/src/lexical.rs',
  'crates/nitride-codegen/src/lexical/fused.rs','scripts/compiler-audit/src/main.rs',
].map(p=>[p,sha(fs.readFileSync(p))]));
fs.writeFileSync(reportFile,JSON.stringify(r,null,2)+'\n');
const string=r.cases.find(c=>c.id==='string-1m');
const count=r.cases.reduce((n,c)=>n+Object.values(c.samples).reduce((m,s)=>m+s.length,0),0);
const sameText=r.variants.baseline.section_sha256['.text']===r.variants.default.section_sha256['.text'];
const rows=r.cases.map(c=>`| ${c.id} | ${['baseline','default','scalar','simd'].map(n=>(c.median_ns[n]/1e6).toFixed(3)).join(' | ')} |`).join('\n');
const md=`# Optional ASCII run batching and AVX2 scanning

The generated lexer can batch repeated ASCII traces instead of replaying each grammar expression per byte. The final pinned 1 MiB string case takes ${(string.median_ns.default/1e6).toFixed(2)} ms with batching disabled, ${(string.median_ns.scalar/1e6).toFixed(2)} ms with scalar batching and ${(string.median_ns.simd/1e6).toFixed(2)} ms with AVX2: ${(string.median_ns.default/string.median_ns.simd).toFixed(2)}x faster than the default on this workload. This is a long-string result, not a general parser speedup.

Both features are **off by default**. Mixed-code measurements include small regressions and timing variation, so the experiment does not justify a default cutover. The default method returns zero without replay, and the compiler removes the generated fast-path call. The default harness's PE text section is ${sameText?'byte-identical to':'different from'} the frozen baseline (section hashes are in the report). All variants retain the same ordinary allocator, portable release flags and grammar.

## Implementation

- The generator finds uniform successful ASCII traces that consume exactly one byte, observe only that byte and execute no rule calls. A repeat can batch those traces while preserving the exact per-iteration work/depth cost.
- Short prefixes decline. Depth/work shortages, Unicode, escapes, unknown traces and the terminating failure execute the original body. Complete iterations stop at the work budget; the original loop reproduces partial limit failures.
- \`nitride-core/ascii-runs\` enables scalar batching. \`nitride-core/simd-ascii-runs\` includes batching and adds runtime AVX2 detection on x86-64. Other CPUs/architectures use the same scalar path.
- AVX2 uses two nibble-table shuffles, a membership mask and a first-rejected-byte mask to classify 32 bytes at a time. Loads stay within the source slice. Disassembly contains \`vpshufb\`, \`vpcmpeqb\` and \`vpmovmskb\` in the new classifier. No native CPU flags are required.
- Regenerated grammar, binding and project bootstrap seeds match the emitter. All four frontend fixed-point checks pass.

## Final pinned comparison

Milliseconds, median of five fresh-process parses per cell; one untimed warmup per cell. ${r.affinity}. Variant order rotates/reverses. Parse timing excludes input reading, export, fingerprinting, process startup and destruction. Same i7-13700/Windows/Rust 1.99.1 host as the compiler audit. No PGO in this comparison.

| Input | Frozen baseline | Default (disabled) | Scalar feature | SIMD feature |
|---|---:|---:|---:|---:|
${rows}

${count} timed executions plus ${r.cases.length*Object.keys(r.variants).length} warmups; **${r.mismatches.length} tree/diagnostic fingerprint mismatches**. Final binary hashes, PE section hashes, feature fingerprints, source hashes and every sample are in [the final report](ascii-scanner-runs.json). [Unpinned](ascii-scanner-runs-unpinned.json) and [pinned](ascii-scanner-runs-prototype-pinned.json) preliminary trials used the prototype before the final feature gate and are retained as exploratory evidence; their binary paths have since been reused. The final report is authoritative.

## Validation and limits

Classifier tests cover all 256 byte values, masks, unaligned inputs, vector lengths and rejected-byte positions. Runtime differential tests compare exact position, observations, work and depth at work/frame/repetition boundaries, EOF, Unicode and escapes. Generated-parser/lexer tests cover the frozen C# fixtures, modes, long strings, limits, edits inside runs and retained old readers. Core tests pass with features disabled and enabled. Strict workspace all-target/all-feature Clippy, formatting and bootstrap checks pass. CI now checks the optional feature on Linux, macOS and Windows.

The workspace run's remaining failure is the pre-existing \`existing_exports_match_native_frontend\` comparison against an ignored XML export. Its old \`omitted_semantics\` metadata lists \`Xml.Paired\`; the pre-change executable also emits an empty list. The ignored export is preserved. Remaining prepare/process/WASM tests pass with that comparison excluded. A temporary test-only Python PATH entry and workspace-local temp directory avoid this machine's Windows Store Python alias and sandbox temp-file permissions; no machine settings are changed.

This is one host and a bounded workload set. The tiny Unicode case measures mostly parser setup, not vector throughput. There is no updated peak-memory, Roslyn, PGO-combination, ARM SIMD or long-running edit/retention measurement. Use the opt-in features only after checking the consuming workload. Reproduction commands are in [the harness README](../scripts/compiler-audit/README.md).
`;
fs.writeFileSync('docs/ascii-scanner-runs.md',md);
console.log(JSON.stringify({timed:count,mismatches:r.mismatches.length,default_text_matches_baseline:sameText,long_string_speedup:string.median_ns.default/string.median_ns.simd}));
if(r.cases.length!==11||count!==220||r.mismatches.length)process.exitCode=1;
