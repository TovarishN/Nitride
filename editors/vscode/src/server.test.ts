import * as assert from 'node:assert/strict';
import * as path from 'node:path';
import { test } from 'node:test';
import { serverCommand, ServerInputs } from './server';

const extension = path.join(path.sep, 'ext');
const bundled = path.join(extension, 'server', 'nitride');
const bundledExe = path.join(extension, 'server', 'nitride.exe');

function inputs(overrides: Partial<ServerInputs>, files: string[] = [bundled, bundledExe]): ServerInputs {
  return { serverPath: '', extensionPath: extension, platform: 'linux', exists: file => files.includes(file), ...overrides };
}

test('server: setting wins', () => {
  assert.deepEqual(serverCommand(inputs({ serverPath: '/opt/nitride' })), { command: '/opt/nitride', args: ['lsp'] });
});

test('server: bundled binary', () => {
  assert.deepEqual(serverCommand(inputs({})), { command: bundled, args: ['lsp'] });
});

test('server: windows exe name', () => {
  assert.deepEqual(serverCommand(inputs({ platform: 'win32' })), { command: bundledExe, args: ['lsp'] });
});

test('server: PATH fallback', () => {
  assert.deepEqual(serverCommand(inputs({}, [])), { command: 'nitride', args: ['lsp'] });
});
