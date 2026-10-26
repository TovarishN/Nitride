import * as assert from 'node:assert/strict';
import { test } from 'node:test';
import { routedLanguage, signatureMode } from './signatures';

test('grammar header routes to ngr', () => {
  assert.equal(signatureMode('#!nitrogen grammar\nlanguage Calc { start Calc.Program; }\n'), 'grammar');
});

test('language header is detected', () => {
  assert.equal(signatureMode('#!nitrogen language "./Calc.syntax"\nlet a=1;'), 'language');
  assert.equal(signatureMode('// #!nitrogen language Calc\nlet a=1;'), 'language');
});

test('prose in a block comment before the header', () => {
  assert.equal(signatureMode('/* notes about this file\n#!nitrogen grammar\n*/\nsyntax module M {}'), 'grammar');
});

test('no header', () => {
  assert.equal(signatureMode('let a=1;\n#!nitrogen grammar\n'), undefined);
  assert.equal(signatureMode('// #!nitrogen augment "./Checks.syntax" document\nclass X {}'), undefined);
});

test('routing: grammars to ngr, signed programs to nitride-signed', () => {
  assert.equal(routedLanguage('grammar', 'plaintext'), 'ngr');
  assert.equal(routedLanguage('language', 'plaintext'), 'nitride-signed');
  assert.equal(routedLanguage(undefined, 'plaintext'), undefined);
  assert.equal(routedLanguage('language', 'nitride-signed'), undefined);
});
