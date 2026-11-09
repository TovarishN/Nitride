import { test } from 'node:test';
import * as assert from 'node:assert/strict';
import { nativeWorker } from './testingWorker';
test('native test worker receives snapshot stdin without shell interpretation', async () => {
    const result = await nativeWorker(process.execPath, ['-e', "let s='';process.stdin.on('data',c=>s+=c);process.stdin.on('end',()=>console.log(s));"], { text: '$()`\\"' }, new AbortController().signal);
    assert.deepEqual(JSON.parse(result.text), { text: '$()`\\"' });
    assert.equal(result.code, 0);
});
test('cancellation reaps a live worker and never reports success', async () => {
    const abort = new AbortController();
    const result = nativeWorker(process.execPath, ['-e', "setInterval(()=>{},1000)"], {}, abort.signal);
    setTimeout(() => abort.abort(), 40);
    await assert.rejects(result, /cancelled/);
});
test('invalid streaming JSON rejects and terminates worker', async () => {
    await assert.rejects(nativeWorker(process.execPath, ['-e', "console.log('not-json');setInterval(()=>{},1000)"], {}, new AbortController().signal, () => { }));
});
test('independent suite refreshes do not debounce each other away', async () => {
    const { SuiteRefresh } = await import('./testingWorker');
    const calls: string[] = [];
    const refresh = new SuiteRefresh(5);
    refresh.schedule('one', () => calls.push('old one'));
    refresh.schedule('two', () => calls.push('two'));
    refresh.schedule('one', () => calls.push('one'));
    await new Promise(resolve => setTimeout(resolve, 30));
    refresh.dispose();
    assert.deepEqual(calls.sort(), ['one', 'two']);
});
