// Compare frozen audit baseline with separately built scalar/SIMD candidates.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { spawnSync } from 'node:child_process';
const sha = p => crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const reportPath = process.env.NITRIDE_SCANNER_REPORT || 'docs/ascii-scanner-runs.json';
const baseline = JSON.parse(fs.readFileSync('docs/compiler-optimization-audit.json'));
const variants = {
  baseline: 'target/compiler-audit/portable/x86_64-pc-windows-msvc/release/nitride-compiler-audit.exe',
  default: 'target/scanner-default/x86_64-pc-windows-msvc/release/nitride-compiler-audit.exe',
  scalar: 'target/scanner-scalar/x86_64-pc-windows-msvc/release/nitride-compiler-audit.exe',
  simd: 'target/scanner-simd/x86_64-pc-windows-msvc/release/nitride-compiler-audit.exe',
};
if (sha(variants.baseline) !== baseline.variants.portable.binary_sha256) throw new Error('Frozen baseline hash changed');
const names = Object.keys(variants);
const cases = baseline.cases.filter(c => c.id !== '16m-valid').map(c => ({id:c.id, file:path.resolve(c.file), mode:c.mode}));
for (const [id, text] of Object.entries({
  'string-1m': 'a'.repeat(1 << 20),
  'string-unicode-escapes': ('a'.repeat(63)+'é😀\\n').repeat(512),
})) {
  const file = path.resolve(`target/compiler-audit/${id}.cs`);
  fs.writeFileSync(file, `class C { string s = "${text}"; }`);
  cases.push({id, file, mode:'strict'});
}
const report = { date:new Date().toISOString(), baseline_revision:baseline.revision,
  method:'Five fresh-process timed parses and one untimed warmup per case/variant, reversing and rotating variant order. Ordinary allocator; same harness and source. Export/fingerprint outside timing.',
  affinity:process.env.NITRIDE_BENCH_AFFINITY || 'not pinned',
  variants:Object.fromEntries(names.map(n=>[n,{file:variants[n],sha256:sha(variants[n])}])),
  cases:[],mismatches:[] };
for (const c of cases) {
  const row = {...c, file:path.relative(process.cwd(),c.file), source_sha256:sha(c.file),samples:Object.fromEntries(names.map(n=>[n,[]]))};
  let expected;
  console.log(`Measuring ${c.id}`);
  for(let round=-1;round<5;round++) {
    let order=round%2===0 ? names.slice().reverse():names.slice();
    const rotation=Math.max(round,0)%names.length;
    order=order.slice(rotation).concat(order.slice(0,rotation));
    for(const name of order) {
      const r=spawnSync(variants[name],[c.mode,c.file],{encoding:'utf8',maxBuffer:8<<20});
      if(r.status!==0) throw new Error(`${name} ${c.id}: ${r.stderr}`);
      const sample=JSON.parse(r.stdout);
      const signature=JSON.stringify([sample.fingerprint,sample.nodes,sample.diagnostics]);
      expected??=signature;
      if(expected!==signature) report.mismatches.push({case:c.id,name,round,expected,actual:signature});
      if(round>=0)row.samples[name].push(sample);
    }
    console.log(`  round ${round+1}/5`);
  }
  row.median_ns=Object.fromEntries(names.map(n=>[n,row.samples[n].map(s=>s.elapsed_ns).sort((a,b)=>a-b)[2]]));
  report.cases.push(row);
  fs.writeFileSync(reportPath,JSON.stringify(report,null,2)+'\n');
  console.log(JSON.stringify({case:c.id,medians:row.median_ns,mismatches:report.mismatches.length}));
}
if(report.mismatches.length)process.exitCode=1;
