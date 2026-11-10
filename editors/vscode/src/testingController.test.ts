import { test } from 'node:test';
import * as assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import * as path from 'node:path';
test('opening and editing a dependency captured from disk cancels and suppresses late pass', () => {
    const result = JSON.parse(execFileSync(process.execPath, [path.resolve(__dirname, '../../src/testingController.fixture.cjs')], { encoding: 'utf8' }));
    assert.equal(result.aborted, true);
    assert.equal(result.results.length, 0);
    assert.ok(!result.calls.some((c: string[]) => c[0] === 'passed'));
});
