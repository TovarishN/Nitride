import { test } from 'node:test';
import * as assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { EventStream, withinRoots, bytePosition, selectedNames } from './testingProtocol';
test('test events require complete consistent results and exit code', () => {
    const stream = new EventStream('/suite.ntest', ['a']);
    stream.accept({ version: 1, event: 'runStarted' });
    stream.accept({ version: 1, event: 'caseFinished', result: { suite: '/suite.ntest', name: 'a', status: 'passed' } });
    stream.accept({ version: 1, event: 'runFinished', report: { version: 1, exit_code: 0, results: [{ suite: '/suite.ntest', name: 'a', status: 'passed' }], errors: [], counts: { passed: 1, failed: 0, errored: 0 } } });
    assert.equal(stream.finish(0).exit_code, 0);
    assert.throws(() => stream.finish(1));
    assert.throws(() => new EventStream('/suite.ntest', ['a']).finish(0));
    const bad = new EventStream('/suite.ntest', ['a']);
    assert.throws(() => bad.accept({ version: 1, event: 'caseFinished', result: { name: 'a', status: 'passed' } }));
});
test('byte spans map to UTF16 and selections preserve exclusions', () => {
    assert.deepEqual(bytePosition('é😀\nx', 6), { line: 0, character: 3 });
    assert.deepEqual(bytePosition('é😀\nx', 7), { line: 1, character: 0 });
    assert.throws(() => bytePosition('é', 1));
    assert.deepEqual(selectedNames(['a', 'b', 'c'], ['a', 'b'], ['b']), ['a']);
});
test('workspace bounds resolve symlinks and accept new leaf files', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nitride-test-path-'));
    try {
        assert.equal(withinRoots(path.join(root, 'new.ntest'), [root]), true);
        assert.equal(withinRoots(path.join(root, '../outside'), [root]), false);
        if (process.platform !== 'win32') {
            fs.symlinkSync(os.tmpdir(), path.join(root, 'escape'));
            assert.equal(withinRoots(path.join(root, 'escape', 'other.ntest'), [root]), false);
        }
    }
    finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});
test('disk fixtures are strict UTF8 and bounded before snapshot encoding', async () => {
    const { readInput } = await import('./testingProtocol');
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nitride-test-input-'));
    try {
        const file = path.join(root, 'fixture');
        fs.writeFileSync(file, Buffer.from([0xff]));
        assert.throws(() => readInput(file));
        fs.writeFileSync(file, 'é');
        assert.equal(readInput(file), 'é');
    }
    finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});
test('raw source diagnostics map into the suite while quoted sources keep byte spans', async () => {
    const { diagnosticLocation } = await import('./testingProtocol');
    const text = 'source <[é😀]>;';
    assert.deepEqual(diagnosticLocation(text, [9, 15], [2, 6]), { start: { line: 0, character: 10 }, end: { line: 0, character: 12 } });
    assert.equal(diagnosticLocation(text, null, [0, 2]), undefined);
});
