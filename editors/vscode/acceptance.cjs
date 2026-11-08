// Loaded by --extensionTestsPath in a fresh VS Code host with an extracted VSIX.
const vscode = require('vscode');
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { performance } = require('node:perf_hooks');

async function until(check, label, timeout = 30000) {
  const start = performance.now();
  while (performance.now() - start < timeout) {
    if (await check()) return;
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  throw new Error(`Timed out: ${label}`);
}

async function replace(document, text) {
  const edit = new vscode.WorkspaceEdit();
  edit.replace(document.uri, new vscode.Range(document.positionAt(0), document.positionAt(document.getText().length)), text);
  assert.equal(await vscode.workspace.applyEdit(edit), true);
}

exports.run = async function () {
  const report = { complete: false, samples: [], checks: [] };
  const target = process.env.NITRIDE_ACCEPTANCE_RESULT;
  try {
    const root = vscode.workspace.workspaceFolders[0].uri.fsPath;
    const extension = vscode.extensions.getExtension('nitride.nitride');
    assert(extension, 'packaged extension discovered');
    assert.equal(fs.realpathSync(extension.extensionPath), fs.realpathSync(process.env.NITRIDE_ACCEPTANCE_EXTENSION));
    assert.equal(vscode.workspace.getConfiguration('nitride').get('server.path'), '');
    report.already_active = extension.isActive;
    const started = performance.now();
    await extension.activate();
    report.activation_wait_ms = performance.now() - started;
    report.vscode_version = vscode.version;
    report.checks.push('isolated packaged extension activated with bundled server');
    const opening = performance.now();
    let one = await vscode.workspace.openTextDocument(path.join(root, 'one.portable'));
    let two = await vscode.workspace.openTextDocument(path.join(root, 'two.portable'));
    // Routing changes plaintext into nitride-signed, closing/reopening the API
    // document. Do not cache a provider query against that obsolete incarnation.
    await until(() => [one, two].every(old => vscode.workspace.textDocuments.some(
      current => current.uri.toString() === old.uri.toString() && current.languageId === 'nitride-signed')),
    'workspace extension routing');
    one = await vscode.workspace.openTextDocument(one.uri);
    two = await vscode.workspace.openTextDocument(two.uri);
    await vscode.window.showTextDocument(one);
    const diagnostics = document => vscode.languages.getDiagnostics(document.uri);
    const has = (document, marker) => diagnostics(document).some(d => d.message.includes(marker));
    const symbol = async (document, name) => {
      const symbols = await vscode.commands.executeCommand('vscode.executeDocumentSymbolProvider', document.uri);
      report.last_symbols = symbols;
      return symbols?.some(s => s.name === name);
    };
    await until(() => symbol(one, 'one'), 'initial source symbols');
    report.source_open_to_features_ms = performance.now() - opening;
    report.checks.push('initial feature provider sees prepared source');
    // A unique unresolved name prevents an old diagnostic satisfying a new edit.
    for (let i = 0; i < 5; i++) {
      const marker = `missing_sample_${i}`;
      const start = performance.now();
      await replace(one, `${marker};\n`);
      await until(() => has(one, marker), marker);
      report.samples.push({ case: 'edit-to-current-diagnostic', ms: performance.now() - start });
    }
    await replace(one, 'missing_independent;\n');
    const start = performance.now();
    // Keep B changing beyond A's debounce period, then inspect A before stopping.
    for (let i = 0; i < 20; i++) {
      await replace(two, `let two : Scalar = ${i};\ntwo;\n`);
      await new Promise(resolve => setTimeout(resolve, 30));
    }
    assert(has(one, 'missing_independent'), 'A diagnostics while B remains active');
    report.samples.push({ case: 'cross-document-checkpoint', ms: performance.now() - start });
    report.checks.push('neighbor edits do not postpone diagnostics');
    for (let i = 0; i < 12; i++) await replace(one, `missing_obsolete_${i};\n`);
    await replace(one, 'let restored : Scalar = 1;\nrestored;\n');
    await until(async () => await symbol(one, 'restored') && diagnostics(one).length === 0, 'latest revision restored');
    report.checks.push('rapid obsolete edits settle to valid latest symbols and diagnostics');
    const grammar = await vscode.workspace.openTextDocument(process.env.NITRIDE_ACCEPTANCE_GRAMMAR);
    const original = grammar.getText();
    assert(original.includes('"let"'));
    const reload = performance.now();
    await replace(grammar, original.replaceAll('"let"', '"bind"'));
    await until(() => diagnostics(one).length > 0, 'open grammar reload invalidates old source keyword');
    report.samples.push({ case: 'grammar-change-to-dependent-diagnostic', ms: performance.now() - reload });
    const restore = performance.now();
    await replace(grammar, original);
    await until(async () => await symbol(one, 'restored') && diagnostics(one).length === 0, 'grammar restored');
    report.samples.push({ case: 'grammar-restore-to-source-features', ms: performance.now() - restore });
    report.checks.push('unsaved grammar change reloads dependent source and restores');
    report.complete = true;
  } catch (error) {
    report.error = error.stack || String(error);
    report.documents = vscode.workspace.textDocuments.map(d => ({
      path: d.uri.fsPath, language: d.languageId, version: d.version,
      diagnostics: vscode.languages.getDiagnostics(d.uri).map(p => p.message),
    }));
    throw error;
  } finally {
    fs.writeFileSync(target, JSON.stringify(report, null, 2));
  }
};
