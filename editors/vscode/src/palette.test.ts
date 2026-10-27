import * as assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { test } from 'node:test';

const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, '..', '..', 'package.json'), 'utf8')) as {
  contributes: {
    semanticTokenTypes: { id: string; description: string }[];
    semanticTokenScopes?: { scopes: Record<string, string[]> }[];
  };
};

test('palette: every token type maps to its scope', () => {
  const scopes = manifest.contributes.semanticTokenScopes?.[0]?.scopes ?? {};
  for (const type of manifest.contributes.semanticTokenTypes) {
    assert.deepEqual(scopes[type.id], [type.description], type.id);
  }
});
