import * as assert from 'node:assert/strict';
import { test } from 'node:test';
import { manifestExtensions } from './manifest';

const files = (entries: Record<string, string>) => (name: string) => entries[name];
const manifest = (...extensions: string[][]) =>
  JSON.stringify({ languages: extensions.map((e, i) => ({ name: `L${i}`, extensions: e, grammars: [], start: 'M.P' })) });

test("manifest: ignores nitride.json", () => {
  assert.deepEqual(manifestExtensions(files({ 'nitride.json': manifest(['.a']) })), []);
});

test('manifest: reads nitrogen.json', () => {
  assert.deepEqual(manifestExtensions(files({ 'nitrogen.json': manifest(['.b', 'c'], ['.b']) })), ['.b', '.c']);
});

test('manifest: invalid JSON yields []', () => {
  assert.deepEqual(manifestExtensions(files({ 'nitrogen.json': '{"languages":[],}' })), []);
});

const tree = (entries: Record<string, string>) => ({
  read: (name: string) => entries[name],
  list: (dir: string) =>
    Object.keys(entries)
      .filter(name => name.startsWith(`${dir}/`) && !name.slice(dir.length + 1).includes('/'))
      .map(name => name.slice(dir.length + 1)),
});

test('manifest: nitride.nproj lists the extensions of the languages it includes', () => {
  const { read, list } = tree({
    'nitride.nproj': '// all\nworkspace\n{\n  include languages/a;\n  /* include languages/x; */\n  include languages/b;\n}\n',
    'languages/a/A.nproj': 'language A { extensions .a, .aa; grammar A.ngr; start A.S; }',
    'languages/a/A.ngr': '',
    'languages/b/B.nproj': 'language B\n{\n  extensions .b;\n  extensions .a; // repeated\n}\n',
    'languages/x/X.nproj': 'language X { extensions .x; }',
  });
  assert.deepEqual(manifestExtensions(read, list), ['.a', '.aa', '.b']);
});

test('manifest: nitride.nproj wins over nitrogen.json', () => {
  const { read, list } = tree({
    'nitride.nproj': 'workspace { include a; }',
    'a/A.nproj': 'language A { extensions .a; }',
    'nitrogen.json': manifest(['.b']),
  });
  assert.deepEqual(manifestExtensions(read, list), ['.a']);
});

test('manifest: an include with two .nproj files is skipped', () => {
  const { read, list } = tree({
    'nitride.nproj': 'workspace { include a; }',
    'a/A.nproj': 'language A { extensions .a; }',
    'a/B.nproj': 'language B { extensions .b; }',
  });
  assert.deepEqual(manifestExtensions(read, list), []);
});
