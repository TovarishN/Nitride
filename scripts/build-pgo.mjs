// Production build: node scripts/build-pgo.mjs [--offline]
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {spawnSync} from 'node:child_process';
const root=process.cwd(), out=path.resolve('target/pgo');
fs.mkdirSync(out,{recursive:true});
let lastOutput='';
function run(exe,args,extra={}) {
  const env={...process.env}; delete env.RUSTFLAGS; delete env.CARGO_ENCODED_RUSTFLAGS;
  const p=spawnSync(exe,args,{cwd:root,env:{...env,...extra},encoding:'utf8',maxBuffer:64<<20});
  lastOutput=(p.stdout||'')+(p.stderr||'');
  if(p.status!==0)throw new Error(`${exe} ${args.join(' ')}\n${p.error||''}\n${p.stderr}`);
  return p.stdout.trim();
}
const hash=p=>crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const toolchain=run('rustc',['-vV']), target=toolchain.match(/host: (.*)/)[1];
const ext=process.platform==='win32'?'.exe':'';
const profdata=path.join(run('rustc',['--print','sysroot']),'lib/rustlib',target,'bin','llvm-profdata'+ext);
if(!fs.existsSync(profdata))throw new Error('Install matching tools first: rustup component add llvm-tools-preview');
const buildDir=path.join(out,'build'), exe=path.join(buildDir,target,'release','nitride'+ext);
const profiles=path.join(out,'profiles-'+Date.now());fs.mkdirSync(profiles);
function build(flags,label) {
  console.log(`Building ${label}`);
  run('cargo',['build','--locked','--release','-p','nitride-cli','--bin','nitride','--target',target,'--target-dir',buildDir,...(process.argv.includes('--offline')?['--offline']:[])],{CARGO_ENCODED_RUSTFLAGS:flags.join('\x1f')});
  fs.writeFileSync(path.join(out,label.startsWith('PGO')?'optimized-build.log':'instrumented-build.log'),lastOutput);
}
const cases=[];
for(const profile of ['portable','power','composition','xml','namespaces','inference','calls','functions','dates','calendar','rewrites','grammar']) {
  const dir=path.join('languages',profile);
  const file=fs.readdirSync(dir).find(f=>f.startsWith('sample.'));
  if(!file)throw new Error(`Missing ${profile} training sample`);
  cases.push({profile,file:path.resolve(dir,file)});
}
const fixture=JSON.parse(fs.readFileSync('fixtures/csharp4-reference.json'));
for(const [i,c] of fixture.slice(0,32).entries()) {
  const file=path.join(out,`csharp-${i}.cs`);fs.writeFileSync(file,c.source);cases.push({profile:'csharp4',file});
}
// Longer valid and malformed inputs exercise loops and recovery beyond tiny fixtures.
for(const damaged of [false,true]) {
  const file=path.join(out,`scaling-${damaged}.cs`);
  const source=Array.from({length:256},(_,i)=>`class C${i} { int F(int x) { return x + ${i}${damaged?'':';'} } }`).join('\n');
  fs.writeFileSync(file,source);cases.push({profile:'csharp4',file});
}
// Optional manifest extends training with consuming-app workloads (paths relative to root).
if(process.env.NITRIDE_PGO_MANIFEST)cases.push(...JSON.parse(fs.readFileSync(process.env.NITRIDE_PGO_MANIFEST)));
build([`-Cprofile-generate=${profiles}`],'instrumented shipping executable');
const observations=[];
for(const [i,c] of cases.entries()) {
  const observation=JSON.parse(run(exe,['--pgo-parse',c.profile,c.file],{LLVM_PROFILE_FILE:path.join(profiles,'%m-%p.profraw')}));
  if(c.profile==='csharp4' && observation.native_calls===0)throw new Error('Generated C# parser not exercised');
  observations.push(observation);console.log(`Trained ${i+1}/${cases.length}: ${c.profile}`);
}
fs.writeFileSync(path.join(out,'training-observations.json'),JSON.stringify(observations));
const raw=fs.readdirSync(profiles).filter(f=>f.endsWith('.profraw')).map(f=>path.join(profiles,f));
if(!raw.length)throw new Error('No instrumentation profiles produced');
const merged=path.join(out,'training.profdata');run(profdata,['merge','-o',merged,...raw]);
build([`-Cprofile-use=${merged}`,'-Cllvm-args=-pgo-warn-missing-function'],'PGO shipping executable');
for(const [i,c] of cases.entries()) {
  const actual=JSON.parse(run(exe,['--pgo-parse',c.profile,c.file]));
  if(c.profile==='csharp4' && actual.native_calls===0)throw new Error('Optimized generated C# parser not exercised');
  // Physical stack/frame layout can change call counts; compare semantic output.
  delete actual.native_calls; delete observations[i].native_calls;
  if(JSON.stringify(actual)!==JSON.stringify(observations[i]))throw new Error(`PGO changed parse/edit results: ${c.file}`);
}
const destination=path.join(out,'dist');fs.mkdirSync(destination,{recursive:true});
fs.copyFileSync(exe,path.join(destination,'nitride'+ext));
const sourceFiles=[...new Set([...run('git',['ls-files','crates']).split('\n').filter(f=>/\.(rs|toml)$/.test(f)),'crates/nitride-cli/src/pgo.rs','Cargo.lock'])];
fs.writeFileSync(path.join(destination,'pgo-build.json'),JSON.stringify({toolchain,target,revision:run('git',['rev-parse','HEAD']),source_dirty:run('git',['status','--porcelain']).length>0,source_sha256:Object.fromEntries(sourceFiles.map(f=>[f,hash(f)])),binary_sha256:hash(exe),profile_sha256:hash(merged),cases:cases.map(c=>({...c,source_sha256:hash(c.file)})),validation:'Instrumented and PGO shipping executable: identical full trees, diagnostics and edited snapshots.',flags:'-Cprofile-use (portable CPU; no SIMD feature enabled)'},null,2)+'\n');
console.log(`Validated PGO executable: ${destination}`);
