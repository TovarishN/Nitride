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
    // Author a new standalone language with the actual bundled native CLI.
    const bundled = path.join(extension.extensionPath, 'server', process.platform === 'win32' ? 'nitride.exe' : 'nitride');
    const { execFileSync } = require('node:child_process');
    const authored = path.join(root, 'Calculator');
    execFileSync(bundled, ['new', 'language', 'Calculator', '--directory', authored, '--extension', '.calc']);
    const evaluated = JSON.parse(execFileSync(bundled, ['run', 'nitride.nproj', 'sample.calc'], { cwd: authored, encoding: 'utf8' }));
    assert.deepEqual(evaluated.values, [42]);
    assert(vscode.workspace.updateWorkspaceFolders(vscode.workspace.workspaceFolders.length, 0, { uri: vscode.Uri.file(authored), name: 'Calculator' }));
    await until(() => {
      report.authoring_workspace_folders = vscode.workspace.workspaceFolders.map(f => f.uri.toString());
      return vscode.workspace.workspaceFolders.some(f => f.uri.fsPath === authored);
    }, 'new standalone workspace folder applied');
    let authoredGrammar = await vscode.workspace.openTextDocument(path.join(authored, 'Calculator.ngr'));
    const lexicalPosition = authoredGrammar.positionAt(authoredGrammar.getText().indexOf('AsciiIdentifier') + 2);
    let libraryLocation;
    await until(async () => {
      const locations = await vscode.commands.executeCommand('vscode.executeDefinitionProvider', authoredGrammar.uri, lexicalPosition);
      report.authoring_definition = locations;
      report.authoring_grammar_language = authoredGrammar.languageId;
      report.authoring_diagnostics = vscode.languages.getDiagnostics(authoredGrammar.uri);
      libraryLocation = locations?.find(l => (l.uri ?? l.targetUri)?.scheme === 'nitride-lib');
      return !!libraryLocation;
    }, 'pinned lexical definition');
    const libraryUri = libraryLocation.uri ?? libraryLocation.targetUri;
    const libraryDoc = await vscode.workspace.openTextDocument(libraryUri);
    assert(libraryDoc.getText().includes('token AsciiIdentifier'));
    const formats = await vscode.commands.executeCommand('vscode.executeFormatDocumentProvider', libraryUri, { tabSize: 2, insertSpaces: true });
    assert.equal(formats?.length ?? 0, 0);
    report.checks.push('new typed language executes42; pinned library opens through read-only content provider');
    const authoredProject = await vscode.workspace.openTextDocument(path.join(authored, 'nitride.nproj'));
    const initialProject = authoredProject.getText();
    await replace(authoredProject, initialProject.replace('grammar Calculator.ngr;', 'grammar Calculator.ngr;\n  grammar Extra.ngr;'));
    await until(() => vscode.languages.getDiagnostics(authoredProject.uri).some(d => d.code === 'NPROJ0001'), 'missing referenced grammar diagnostic');
    let creation;
    await until(async () => {
      const actions = await vscode.commands.executeCommand('vscode.executeCodeActionProvider', authoredProject.uri, new vscode.Range(0,0,authoredProject.lineCount,0), 'quickfix', 100);
      creation = actions?.find(a => a.title === 'Create Extra.ngr' && a.edit);
      return !!creation;
    }, 'resolved create-only grammar quick fix');
    assert.equal(await vscode.workspace.applyEdit(creation.edit), true);
    const extra = await vscode.workspace.openTextDocument(path.join(authored, 'Extra.ngr'));
    await extra.save();
    assert(extra.getText().includes('syntax module Extra'));
    assert(!extra.getText().includes('syntax Program'), 'existing start rule is not duplicated');
    await until(() => !vscode.languages.getDiagnostics(authoredProject.uri).some(d => d.code === 'NPROJ0001'), 'created grammar clears missing-file diagnostic');
    await authoredProject.save();
    report.checks.push('structured missing-file diagnostic resolves via create-only quick fix without duplicating start');
    await replace(authoredProject, authoredProject.getText().replace('  library Nitride.Lexical version 1.0.0;\n', ''));
    await until(() => vscode.languages.getDiagnostics(authoredGrammar.uri).some(d => d.message.includes('Nitride.Lexical.AsciiIdentifier')), 'explicit unpinned lexical diagnostic');
    let pin;
    await until(async () => {
      const actions = await vscode.commands.executeCommand('vscode.executeCodeActionProvider', authoredGrammar.uri, new vscode.Range(lexicalPosition, lexicalPosition), 'quickfix', 100);
      pin = actions?.find(a => a.title === 'Pin bundled Nitride.Lexical@1.0.0' && a.edit);
      return !!pin;
    }, 'resolved exact library pin fix');
    assert.equal(await vscode.workspace.applyEdit(pin.edit), true);
    await until(() => vscode.languages.getDiagnostics(authoredGrammar.uri).length === 0, 'library pin restores grammar diagnostics');
    await authoredProject.save();
    report.checks.push('explicit unpinned lexical reference resolves through versioned exact-pin quick fix');
    const testing = extension.exports.testing;
    assert(testing, 'packaged Testing API is available');
    const suiteDoc = await vscode.workspace.openTextDocument(path.join(authored, 'tests.ntest'));
    let suiteState = await testing.discover(suiteDoc.uri);
    assert(suiteState && suiteState.info.tests.length === 3, 'native suite discovery');
    const noCancellation = new vscode.CancellationTokenSource();
    const runSuite = async (extra) => testing.run(new vscode.TestRunRequest([suiteState.item]), noCancellation.token, extra);
    let testResults = await runSuite();
    assert.equal(testResults.length, 3);
    assert(testResults.every(r => r.status === 'passed'));
    const selectedCase = suiteState.item.children.get(`${suiteState.item.id}#${encodeURIComponent('sample evaluates')}`);
    testResults = await testing.run(new vscode.TestRunRequest([selectedCase]), noCancellation.token);
    assert.equal(testResults.length, 1, 'individual case selection survives rediscovery');
    assert.equal(testResults[0].status, 'passed');
    const fixture = await vscode.workspace.openTextDocument(path.join(authored, 'sample.calc'));
    const fixtureOriginal = fixture.getText();
    await replace(fixture, fixtureOriginal.replace('42', '43'));
    testResults = await runSuite();
    assert(testResults.some(r => r.name === 'sample evaluates' && r.status === 'failed'), 'unsaved fixture affects test values');
    assert.equal(fs.readFileSync(fixture.uri.fsPath, 'utf8'), fixtureOriginal, 'tests do not save fixture');
    await replace(fixture, fixtureOriginal);
    const grammarBeforeTests = authoredGrammar.getText();
    await replace(authoredGrammar, grammarBeforeTests.replaceAll('"let"', '"bind"'));
    testResults = await runSuite();
    assert(testResults.some(r => r.name === 'sample evaluates' && r.status === 'failed'), 'unsaved grammar affects test parser');
    assert.equal(fs.readFileSync(authoredGrammar.uri.fsPath, 'utf8'), grammarBeforeTests, 'tests do not save grammar');
    await replace(authoredGrammar, grammarBeforeTests);
    testResults = await runSuite();
    assert(testResults.every(r => r.status === 'passed'));
    const suiteOriginal = suiteDoc.getText();
    await replace(suiteDoc, 'broken tests');
    assert.equal(await testing.discover(suiteDoc.uri), undefined);
    assert(vscode.languages.getDiagnostics(suiteDoc.uri).some(d => d.source === 'nitride-tests'));
    await replace(suiteDoc, suiteOriginal);
    suiteState = await testing.discover(suiteDoc.uri);
    assert(suiteState, 'suite syntax recovers');
    const cancellation = new vscode.CancellationTokenSource();
    testResults = await testing.run(new vscode.TestRunRequest([suiteState.item]), cancellation.token, () => cancellation.cancel());
    assert.equal(testResults.length, 1, 'cancel retains completed case and ignores late worker results');
    cancellation.dispose();
    noCancellation.dispose();
    report.checks.push('Testing view discovers native suites, selects cases and reports values/diagnostics');
    report.checks.push('Testing snapshots use unsaved fixture/grammar without saving and recover suite syntax');
    report.checks.push('Testing cancellation retains completed results and rejects late success');
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
    const launched = Number(process.env.NITRIDE_ACCEPTANCE_LAUNCH_EPOCH_MS);
    assert(Number.isFinite(launched) && launched > 0 && Date.now() >= launched);
    report.host_launch_to_source_features_ms = Date.now() - launched;
    report.checks.push('initial feature provider sees prepared source');
    // A unique unresolved name prevents an old diagnostic satisfying a new edit.
    for (let i = 0; i < 5; i++) {
      const marker = `missing_sample_${i}`;
      const start = performance.now();
      await replace(one, `${marker};\n`);
      await until(() => has(one, marker), marker);
      report.samples.push({ case: 'edit-to-current-diagnostic', ms: performance.now() - start });
    }
    const editor = await vscode.window.showTextDocument(one);
    await vscode.commands.executeCommand('workbench.action.focusWindow');
    if (!vscode.window.state.focused && process.env.NITRIDE_ACCEPTANCE_ALLOW_UNFOCUSED === '1') {
      report.unavailable_checks = ['Focused editor type command: disposable window could not become active'];
      await replace(one, 'let typed : Scalar = 1;\ntyped;\n');
    } else {
      await until(() => vscode.window.state.focused, 'disposable editor window is focused');
      await vscode.commands.executeCommand('editor.closeTestPeek');
      await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
      editor.selection = new vscode.Selection(one.positionAt(0), one.positionAt(one.getText().length));
      const typed = performance.now();
      await vscode.commands.executeCommand('type', { text: 'missing_typed_editor;\n' });
      await until(() => one.getText().includes('missing_typed_editor'), 'type command edits the focused source editor');
      await until(() => has(one, 'missing_typed_editor'), 'editor typing diagnostic');
      report.samples.push({ case: 'editor-type-to-current-diagnostic', ms: performance.now() - typed });
      editor.selection = new vscode.Selection(one.positionAt(0), one.positionAt(one.getText().length));
      await vscode.commands.executeCommand('type', { text: 'let typed : Scalar = 1;\ntyped;\n' });
      report.checks.push('focused editor type command updates current diagnostics');
    }
    await until(async () => await symbol(one, 'typed') && diagnostics(one).length === 0, 'valid source after editor input');
    await replace(one, 'let broken : Scalar = ;\n');
    await until(() => diagnostics(one).length > 0, 'malformed typed source diagnostic');
    await replace(one, 'let recovered : Scalar = 1;\nrecovered;\n');
    await until(async () => await symbol(one, 'recovered') && diagnostics(one).length === 0, 'typed source recovers');
    report.checks.push('malformed source produces diagnostics and recovers');
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
    if (process.env.NITRIDE_ACCEPTANCE_LARGE) {
      const opening = performance.now();
      let large = await vscode.workspace.openTextDocument(process.env.NITRIDE_ACCEPTANCE_LARGE);
      await until(() => vscode.workspace.textDocuments.some(current =>
        current.uri.toString() === large.uri.toString() && current.languageId === 'nitride-signed'),
      'large document routing');
      large = await vscode.workspace.openTextDocument(large.uri);
      await vscode.window.showTextDocument(large);
      const folds = () => vscode.commands.executeCommand('vscode.executeFoldingRangeProvider', large.uri);
      const ready = async expected => {
        const ranges = await folds();
        report.large_fold_count = ranges?.length ?? 0;
        report.large_first_folds = ranges?.slice(0, 6).map(range => ({ start: range.start, end: range.end }));
        // VS Code applies its default 5,000-region cap even to the provider
        // command. Keep the final freshness probe early, within that cap.
        const found = expected ? ranges?.some(range => range.start === expected.start && range.end === expected.end)
          : ranges?.length > 0;
        return found && diagnostics(large).length === 0;
      };
      await until(ready, 'large source features and valid diagnostics');
      report.samples.push({ case: 'large-open-to-current-features', ms: performance.now() - opening });
      const at = large.getText().lastIndexOf(';');
      assert(at > 0 && large.getText()[at - 1] === '1');
      const remove = new vscode.WorkspaceEdit();
      remove.delete(large.uri, new vscode.Range(large.positionAt(at), large.positionAt(at + 1)));
      const broken = performance.now();
      assert.equal(await vscode.workspace.applyEdit(remove), true);
      await until(async () => { await folds(); return diagnostics(large).length > 0; }, 'large missing-semicolon diagnostic');
      report.samples.push({ case: 'large-edit-to-recovery-diagnostic', ms: performance.now() - broken });
      const restore = new vscode.WorkspaceEdit();
      restore.insert(large.uri, large.positionAt(at), ';');
      const restored = performance.now();
      assert.equal(await vscode.workspace.applyEdit(restore), true);
      await until(ready, 'large source restored');
      report.samples.push({ case: 'large-restore-to-current-features', ms: performance.now() - restored });
      for (let i = 0; i < 8; i++) {
        const edit = new vscode.WorkspaceEdit();
        edit.replace(large.uri, new vscode.Range(large.positionAt(at - 1), large.positionAt(at)), String(2 + i % 2));
        assert.equal(await vscode.workspace.applyEdit(edit), true);
      }
      assert(!(await folds()).some(range => range.start === 1 && range.end === 3), 'final marker fold is absent before edit');
      const final = new vscode.WorkspaceEdit();
      final.insert(large.uri, large.positionAt(0), '\n// LatestLarge revision\n// freshness marker\n// checkpoint\n');
      const settled = performance.now();
      assert.equal(await vscode.workspace.applyEdit(final), true);
      await until(() => ready({ start: 1, end: 3 }), 'large latest revision has the new leading comment fold');
      report.samples.push({ case: 'large-final-edit-to-current-features', ms: performance.now() - settled });
      report.large_latest_document_version = large.version;
      report.checks.push('4 MiB source opens, recovers, restores and rapid edits expose the final new leading fold');
    }
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
