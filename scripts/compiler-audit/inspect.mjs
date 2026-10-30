// Summarize emitted Rust assembly: node scripts/compiler-audit/inspect.mjs <assembly.s>
import fs from 'node:fs';
import crypto from 'node:crypto';
const file = process.argv[2];
const source = fs.readFileSync(file, 'utf8');
const functions = [];
let current;
for (const line of source.split('\n')) {
  const start = line.match(/^\s*\.seh_proc\s+(\S+)/);
  if (start) current = { symbol: start[1], instructions: 0, xmm: 0, ymm: 0, zmm: 0, packed_comparisons: 0, vector_moves: 0 };
  if (!current) continue;
  if (/^\s+[a-z][a-z0-9]+\s/.test(line)) current.instructions++;
  for (const reg of ['xmm', 'ymm', 'zmm']) if (new RegExp(`%${reg}[0-9]+`).test(line)) current[reg]++;
  if (/\b(v?pcmpeq[bwdq]|v?pcmpgt[bwdq]|v?pmovmskb)\b/.test(line)) current.packed_comparisons++;
  if (/\b(v?movdqu\w*|v?movdqa\w*|v?movups|v?movaps)\b/.test(line)) current.vector_moves++;
  if (line.includes('.seh_endproc')) { functions.push(current); current = undefined; }
}
const lexical = functions.filter(f => /nitride_lex_[0-9]/.test(f.symbol));
const syntax = functions.filter(f => /nitride_syntax_[0-9]/.test(f.symbol));
const totals = list => Object.fromEntries(['instructions', 'xmm', 'ymm', 'zmm', 'packed_comparisons', 'vector_moves'].map(k => [k, list.reduce((n,f) => n+f[k], 0)]));
console.log(JSON.stringify({ file, sha256: crypto.createHash('sha256').update(source).digest('hex'),
  method: 'Static instruction-site counts within Windows .seh_proc blocks; not executed instruction counts. Vector register use includes moves and support work. Inlined routines are attributed to their enclosing function.',
  all: { functions: functions.length, ...totals(functions) },
  lexical: { functions: lexical.length, ...totals(lexical) },
  syntax: { functions: syntax.length, ...totals(syntax) },
  lexical_functions: lexical,
  packed_comparison_functions: functions.filter(f => f.packed_comparisons > 0).map(f => ({symbol:f.symbol, packed_comparisons:f.packed_comparisons})),
}, null, 2));
