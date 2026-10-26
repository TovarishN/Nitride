import * as assert from 'node:assert/strict';
import { test } from 'node:test';
import { manifestExtensions } from './manifest';

const files = (entries: Record<string, string>) => (name: string) => entries[name];
const manifest = (...extensions: string[][]) =>
  JSON.stringify({ languages: extensions.map((e, i) => ({ name: `L${i}`, extensions: e, grammars: [], start: 'M.P' })) });

test('manifest: prefers nitride.json', () => {
  assert.deepEqual(
    manifestExtensions(files({ 'nitride.json': manifest(['.a']), 'nitrogen.json': manifest(['.b']) })),
    ['.a'],
  );
});

test('manifest: falls back to nitrogen.json', () => {
  assert.deepEqual(manifestExtensions(files({ 'nitrogen.json': manifest(['.b', 'c'], ['.b']) })), ['.b', '.c']);
});

test('manifest: invalid JSON yields []', () => {
  assert.deepEqual(manifestExtensions(files({ 'nitride.json': '{"languages":[],}' })), []);
});
