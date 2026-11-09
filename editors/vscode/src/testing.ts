import * as path from 'node:path';
import * as vscode from 'vscode';
import { bytePosition, diagnosticLocation, listResponse, physicalPath, readInput, withinRoots, Result, selectedNames, SuiteInfo } from './testingProtocol';
import { executeTests, nativeWorker, SuiteRefresh } from './testingWorker';
interface State {
    item: vscode.TestItem;
    info: SuiteInfo;
    snapshot: unknown;
    versions: Map<string, number>;
}
export function activateTesting(context: vscode.ExtensionContext, command: string) {
    const controller = vscode.tests.createTestController('nitride.tests', 'Nitride');
    const diagnostics = vscode.languages.createDiagnosticCollection('nitride-tests');
    const states = new Map<string, State>();
    const generations = new Map<string, number>();
    let disposed = false;
    let queue: Promise<unknown> = Promise.resolve();
    let active: AbortController | undefined;
    const workers = new Set<AbortController>();
    const enqueue = <T>(work: () => Promise<T>): Promise<T> => { const next = queue.then(work); queue = next.catch(() => { }); return next; };
    const roots = () => (vscode.workspace.workspaceFolders ?? []).filter(f => f.uri.scheme === 'file').map(f => f.uri.fsPath);
    const safe = (file: string) => {
        if (!within(file))
            throw new Error(`Test input is outside an open workspace or cannot be resolved: ${file}`);
        return physicalPath(file);
    };
    const within = (file: string) => withinRoots(file, roots());
    const key = (uri: vscode.Uri) => physicalPath(uri.fsPath);
    const snapshot = (paths: string[]) => {
        const wanted = new Set(paths.map(safe));
        const files: {
            path: string;
            text: string;
        }[] = [];
        const versions = new Map<string, number>();
        for (const doc of vscode.workspace.textDocuments) {
            if (doc.uri.scheme !== 'file')
                continue;
            let name: string;
            try {
                name = key(doc.uri);
            }
            catch {
                continue;
            }
            if (wanted.has(name)) {
                if (files.some(f => f.path === name))
                    throw new Error(`Multiple buffers for ${name}`);
                files.push({ path: name, text: doc.getText() });
                versions.set(doc.uri.toString(), doc.version);
            }
        }
        for (const name of wanted) {
            if (!files.some(f => f.path === name)) {
                try {
                    files.push({ path: name, text: readInput(name) });
                }
                catch (error) {
                    if ((error as NodeJS.ErrnoException).code !== 'ENOENT')
                        throw error;
                }
            }
        }
        if (files.some(f => Buffer.byteLength(f.text) > 16 * 1024 * 1024) || Buffer.byteLength(JSON.stringify(files)) > 64 * 1024 * 1024)
            throw new Error('Test input snapshot exceeds size limit');
        files.sort((a, b) => a.path.localeCompare(b.path));
        return { payload: { version: 1, files, roots: roots().map(physicalPath) }, versions };
    };
    const current = (versions: Map<string, number>) => [...versions].every(([uri, version]) => vscode.workspace.textDocuments.some(d => d.uri.toString() === uri && d.version === version));
    const itemFor = (uri: vscode.Uri) => {
        const id = uri.toString();
        let item = controller.items.get(id);
        if (!item) {
            item = controller.createTestItem(id, path.basename(uri.fsPath), uri);
            item.canResolveChildren = true;
            controller.items.add(item);
        }
        return item;
    };
    async function discover(uri: vscode.Uri): Promise<State | undefined> {
        const item = itemFor(uri);
        const generation = (generations.get(item.id) ?? 0) + 1;
        generations.set(item.id, generation);
        return enqueue(async () => {
            if (disposed || generations.get(item.id) !== generation)
                return;
            const abort = new AbortController();
            workers.add(abort);
            try {
                const suite = safe(uri.fsPath);
                let paths = [suite, ...vscode.workspace.textDocuments.filter(d => d.uri.scheme === 'file' && within(d.uri.fsPath) && /\.(nproj|json)$/.test(d.uri.fsPath)).map(d => d.uri.fsPath)];
                let captured = snapshot(paths);
                let info: SuiteInfo | undefined;
                for (let attempt = 0; attempt < 4; attempt++) {
                    const response = await nativeWorker(command, ['test', suite, '--list', '--json', '--snapshot-stdin'], captured.payload, abort.signal);
                    const listed = listResponse(JSON.parse(response.text));
                    if (listed.errors.length) {
                        const error = listed.errors[0];
                        const text = vscode.workspace.textDocuments.find(d => d.uri.toString() === uri.toString())?.getText() ?? '';
                        let range = new vscode.Range(0, 0, 0, 0);
                        try {
                            if (error.span) {
                                const a = bytePosition(text, error.span[0]), b = bytePosition(text, error.span[1]);
                                range = new vscode.Range(a.line, a.character, b.line, b.character);
                            }
                        }
                        catch { }
                        if (generations.get(item.id) === generation) {
                            const diagnostic = new vscode.Diagnostic(range, error.message, vscode.DiagnosticSeverity.Error);
                            diagnostic.source = 'nitride-tests';
                            diagnostics.set(uri, [diagnostic]);
                        }
                        throw new Error(error.message);
                    }
                    info = listed.suites[0];
                    if (!info || physicalPath(info.path) !== suite)
                        throw new Error('Unexpected suite in discovery');
                    paths = info.dependencies.map(safe);
                    const next = snapshot(paths);
                    if (JSON.stringify(next.payload) === JSON.stringify(captured.payload) && current(captured.versions)) {
                        captured = next;
                        break;
                    }
                    if (attempt === 3)
                        throw new Error('Test inputs changed during discovery; retry');
                    captured = next;
                    info = undefined;
                }
                if (!info || disposed || generations.get(item.id) !== generation)
                    return;
                const children = info.tests.map(test => {
                    const child = controller.createTestItem(`${item.id}#${encodeURIComponent(test.name)}`, test.name, uri);
                    const doc = vscode.workspace.textDocuments.find(d => d.uri.toString() === uri.toString());
                    if (doc) {
                        const a = bytePosition(doc.getText(), test.name_span[0]), b = bytePosition(doc.getText(), test.name_span[1]);
                        child.range = new vscode.Range(a.line, a.character, b.line, b.character);
                    }
                    return child;
                });
                item.children.replace(children);
                item.error = info.project_error ?? undefined;
                diagnostics.delete(uri);
                const state = { item, info, snapshot: captured.payload, versions: captured.versions };
                states.set(item.id, state);
                return state;
            }
            catch (error) {
                if (!disposed && generations.get(item.id) === generation) {
                    item.error = String(error);
                    item.children.replace([]);
                    states.delete(item.id);
                }
                return;
            }
            finally {
                workers.delete(abort);
            }
        });
    }
    async function refresh() {
        for (const uri of await vscode.workspace.findFiles('**/*.ntest', '**/{.git,target,node_modules}/**'))
            await discover(uri);
        for (const doc of vscode.workspace.textDocuments)
            if (doc.uri.scheme === 'file' && doc.uri.path.endsWith('.ntest'))
                await discover(doc.uri);
    }
    controller.resolveHandler = async (item) => {
        if (item?.uri)
            await discover(item.uri);
        else
            await refresh();
    };
    async function run(request: vscode.TestRunRequest, token: vscode.CancellationToken, onResult?: (result: Result) => void): Promise<Result[]> {
        active?.abort();
        const abort = new AbortController();
        active = abort;
        const cancellation = token.onCancellationRequested(() => abort.abort());
        if (token.isCancellationRequested)
            abort.abort();
        const run = controller.createTestRun(request);
        const results: Result[] = [];
        try {
            if (!vscode.workspace.isTrusted)
                throw new Error('Trust the workspace before running tests');
            if (!request.include)
                await refresh();
            const suiteItems = new Map<string, vscode.TestItem>();
            for (const item of request.include ?? [...collection(controller.items)]) {
                const suite = item.parent ?? item;
                suiteItems.set(suite.id, suite);
            }
            for (const suiteItem of suiteItems.values()) {
                if (abort.signal.aborted)
                    break;
                if (!suiteItem.uri)
                    continue;
                const state = await discover(suiteItem.uri);
                if (!state) {
                    run.errored(suiteItem, new vscode.TestMessage(suiteItem.error?.toString() ?? 'Suite discovery failed'));
                    continue;
                }
                const included = request.include?.filter(i => i === suiteItem || i.parent === suiteItem);
                const includeNames = !included || included.includes(suiteItem) ? undefined : included.map(i => i.label);
                const excluded = request.exclude ?? [];
                const names = excluded.includes(suiteItem) ? [] : selectedNames(state.info.tests.map(c => c.name), includeNames, excluded.filter(i => i.parent === suiteItem).map(i => i.label));
                const items = new Map(names.map(n => [n, state.item.children.get(`${state.item.id}#${encodeURIComponent(n)}`)!]));
                for (const item of items.values())
                    run.enqueued(item);
                const seen = new Set<string>();
                const stale = () => {
                    for (const item of items.values())
                        item.description = 'Inputs changed; rerun';
                    abort.abort();
                };
                const changed = vscode.workspace.onDidChangeTextDocument(() => {
                    if (!current(state.versions))
                        stale();
                });
                const closed = vscode.workspace.onDidCloseTextDocument(() => {
                    if (!current(state.versions))
                        stale();
                });
                const disk = vscode.workspace.createFileSystemWatcher('**/*');
                const invalidate = (uri: vscode.Uri) => {
                    try {
                        if (state.info.dependencies.some(p => physicalPath(p) === physicalPath(uri.fsPath)))
                            stale();
                    }
                    catch { }
                };
                const diskChanges = [disk.onDidCreate(invalidate), disk.onDidChange(invalidate), disk.onDidDelete(invalidate)];
                try {
                    if (!names.length)
                        continue;
                    for (const item of items.values())
                        run.started(item);
                    const report = await enqueue(() => executeTests(command, state.info.path, names, state.snapshot, abort.signal, result => {
                        if (abort.signal.aborted || !current(state.versions))
                            return;
                        const item = items.get(result.name);
                        if (!item)
                            return;
                        seen.add(result.name);
                        results.push(result);
                        onResult?.(result);
                        if (result.status === 'passed')
                            run.passed(item);
                        else {
                            const message = new vscode.TestMessage(result.message ?? 'Test expectation mismatch');
                            message.expectedOutput = JSON.stringify(result.expected, null, 2);
                            message.actualOutput = JSON.stringify(result.actual, null, 2);
                            try {
                                const test = state.info.tests.find(t => t.name === result.name)!;
                                const actual = result.actual as {
                                    diagnostics?: {
                                        span: [
                                            number,
                                            number
                                        ];
                                    }[];
                                } | undefined;
                                const diagnostic = actual?.diagnostics?.[0];
                                const files = (state.snapshot as {
                                    files: {
                                        path: string;
                                        text: string;
                                    }[];
                                }).files;
                                if (diagnostic) {
                                    const file = test.source.file ? physicalPath(path.resolve(path.dirname(state.info.path), test.source.file)) : state.info.path;
                                    const text = files.find(f => f.path === physicalPath(file))?.text;
                                    if (text !== undefined) {
                                        const location = test.source.file ? { start: bytePosition(text, diagnostic.span[0]), end: bytePosition(text, diagnostic.span[1]) } : diagnosticLocation(text, test.source.raw_span, diagnostic.span);
                                        if (location) {
                                            const doc = vscode.workspace.textDocuments.find(d => d.uri.scheme === 'file' && physicalPath(d.uri.fsPath) === physicalPath(file));
                                            message.location = new vscode.Location(doc?.uri ?? vscode.Uri.file(file), new vscode.Range(location.start.line, location.start.character, location.end.line, location.end.character));
                                        }
                                    }
                                }
                            }
                            catch { /* Keep original byte spans when no honest editor range is available. */ }
                            if (result.status === 'failed')
                                run.failed(item, message);
                            else
                                run.errored(item, message);
                        }
                    }));
                    if (report.errors.length)
                        run.errored(suiteItem, new vscode.TestMessage(report.errors.map(e => e.message).join('\n')));
                    if (report.errors.length)
                        for (const [name, item] of items)
                            if (!seen.has(name))
                                run.errored(item, new vscode.TestMessage(report.errors.map(e => e.message).join('\n')));
                }
                catch (error) {
                    for (const [name, item] of items) {
                        if (abort.signal.aborted) {
                            if (!seen.has(name))
                                run.skipped(item);
                        }
                        else
                            run.errored(item, new vscode.TestMessage(String(error)));
                    }
                    if (!abort.signal.aborted)
                        throw error;
                }
                finally {
                    changed.dispose();
                    closed.dispose();
                    disk.dispose();
                    diskChanges.forEach(d => d.dispose());
                }
            }
        }
        catch (error) {
            run.appendOutput(`${String(error)}\r\n`);
            for (const item of request.include ?? [...collection(controller.items)])
                run.errored(item, new vscode.TestMessage(String(error)));
        }
        finally {
            cancellation.dispose();
            run.end();
            if (active === abort)
                active = undefined;
        }
        return results;
    }
    controller.createRunProfile('Run', vscode.TestRunProfileKind.Run, async (request, token) => { await run(request, token); }, true);
    const refreshes = new SuiteRefresh();
    const soon = (uri: vscode.Uri) => refreshes.schedule(uri.toString(), () => { void discover(uri); });
    const watcher = vscode.workspace.createFileSystemWatcher('**/*.ntest');
    context.subscriptions.push(controller, diagnostics, watcher, watcher.onDidCreate(soon), watcher.onDidChange(soon), watcher.onDidDelete(uri => { controller.items.delete(uri.toString()); states.delete(uri.toString()); diagnostics.delete(uri); }), vscode.workspace.onDidOpenTextDocument(doc => {
        if (doc.uri.scheme === 'file' && doc.uri.path.endsWith('.ntest'))
            soon(doc.uri);
    }), vscode.workspace.onDidChangeTextDocument(e => {
        if (e.document.uri.scheme === 'file' && e.document.uri.path.endsWith('.ntest'))
            soon(e.document.uri);
    }), { dispose() {
            disposed = true;
            active?.abort();
            for (const worker of workers)
                worker.abort();
            refreshes.dispose();
        } });
    void refresh();
    return { controller, discover, run };
}
function* collection(items: vscode.TestItemCollection): IterableIterator<vscode.TestItem> { const found: vscode.TestItem[] = []; items.forEach(item => found.push(item)); yield* found; }
