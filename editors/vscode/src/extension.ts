import * as fs from 'fs';
import * as path from 'path';
import { commands, ExtensionContext, languages, Range, TextDocument, Uri, window, workspace } from 'vscode';
import { DocumentSelector, LanguageClient, LanguageClientOptions } from 'vscode-languageclient/node';
import { manifestExtensions } from './manifest';
import { serverCommand } from './server';
import { activateTesting } from './testing';
import { claimedMode, routedLanguage, signatureMode } from './signatures';

let client: LanguageClient | undefined;

/** File extensions of the languages every workspace folder's manifest declares. */
function workspaceExtensions(): string[] {
  const folders = workspace.workspaceFolders ?? [];
  return [...new Set(folders.flatMap(folder => manifestExtensions(
    name => {
      try {
        return fs.readFileSync(path.join(folder.uri.fsPath, name), 'utf8');
      } catch {
        return undefined;
      }
    },
    dir => {
      try {
        return fs.readdirSync(path.join(folder.uri.fsPath, dir));
      } catch {
        return [];
      }
    },
  )))];
}

export async function activate(context: ExtensionContext) {
  const server = serverCommand({
    serverPath: workspace.getConfiguration('nitride').get<string>('server.path') ?? '',
    extensionPath: context.extensionPath,
    platform: process.platform,
    exists: file => fs.existsSync(file),
  });
  const selector: DocumentSelector = [
    { scheme: 'file', language: 'ngr' },
    { scheme: 'nitride-lib', language: 'ngr' },
    { scheme: 'file', language: 'ncatalog' },
    { scheme: 'file', language: 'nbind' },
    { scheme: 'file', language: 'nproj' },
    { scheme: 'file', pattern: '**/{nitride.nproj,nitrogen.json}' },
    ...workspaceExtensions().map(extension => ({ scheme: 'file', pattern: `**/*${extension}` })),
    // Files that choose their language with a #!nitrogen signature (routed below).
    { scheme: 'file', language: 'nitride-signed' },
  ];
  // Signed grammars can have any name, so every file is watched.
  const watcher = workspace.createFileSystemWatcher('**/*');
  context.subscriptions.push(watcher);
  const options: LanguageClientOptions = {
    documentSelector: selector,
    synchronize: { fileEvents: watcher },
    middleware: {
      resolveCodeAction: async (action, token, next) => {
        const data = client?.code2ProtocolConverter.asCodeActionSync(action).data as { code?: string; project?: string } | undefined;
        if (data?.code === 'NLIB0004' && data.project) await workspace.openTextDocument(Uri.parse(data.project));
        return next(action, token);
      },
    },
    // Colour with the theme's TextMate palette (contributes.semanticTokenScopes).
    initializationOptions: { paletteTokens: true, libraryDocuments: true },
  };
  client = new LanguageClient('nitride', 'Nitride', server, options);
  await client.start();
  context.subscriptions.push(commands.registerCommand('nitride.newLanguage', async () => {
    try {
      const folders = workspace.workspaceFolders ?? [];
      if (!folders.length) { await window.showInformationMessage('Open a workspace folder before creating a language.'); return; }
      const folder = folders.length === 1 ? folders[0] : (await window.showQuickPick(folders.map(folder => ({ label: folder.name, folder })), { placeHolder: 'Create inside workspace' }))?.folder;
      if (!folder) return;
      const name = await window.showInputBox({ prompt: 'Language name', value: 'Calculator', validateInput: value => /^[A-Za-z_][A-Za-z0-9_]*$/.test(value) ? undefined : 'Use an ASCII identifier' });
      if (!name) return;
      const extension = await window.showInputBox({ prompt: 'Source file extension', value: '.calc', validateInput: value => /^\.[A-Za-z0-9_]+$/.test(value) ? undefined : 'Use a dot followed by letters, digits or underscores' });
      if (!extension) return;
      const kind = await window.showQuickPick(['typed', 'syntax'], { placeHolder: 'Language starter' });
      if (!kind) return;
      const directory = Uri.joinPath(folder.uri, name);
      await client?.sendRequest('workspace/executeCommand', { command: 'nitride.createLanguage', arguments: [{ directory: directory.toString(), name, extension, kind }] });
      const choice = await window.showInformationMessage(`Created ${name} in ${directory.fsPath}`, 'Open in New Window');
      if (choice) await commands.executeCommand('vscode.openFolder', directory, { forceNewWindow: true });
    } catch (error) { await window.showErrorMessage(`Cannot create language: ${String(error)}`); }
  }));
  context.subscriptions.push(workspace.registerTextDocumentContentProvider('nitride-lib', {
    provideTextDocumentContent: async uri => {
      if (!client) throw new Error('Nitride server is unavailable');
      return client.sendRequest<string>('nitride/librarySource', { uri: uri.toString() });
    },
  }));
  context.subscriptions.push(workspace.onDidOpenTextDocument(document => {
    if (document.uri.scheme === 'nitride-lib' && document.languageId !== 'ngr') void languages.setTextDocumentLanguage(document, 'ngr');
  }));
  // A signed grammar (`#!nitrogen grammar`) is a grammar whatever its name, and a program with a
  // `#!nitrogen language` header belongs to Nitride: give each its language, so only these documents
  // (not every file) reach the server. The header is in the leading trivia: the first lines suffice.
  // Extensions that workspace languages (`nitride.nproj`) and signed grammars
  // (`language X { extensions ".x"; }`) claim, from the server.
  let languageExtensions: string[] = [];
  const route = (document: TextDocument): void => {
    if (document.uri.scheme !== 'file') return;
    const head = document.getText(new Range(0, 0, Math.min(document.lineCount, 200), 0));
    const claimed = languageExtensions.some(extension => document.uri.path.toLowerCase().endsWith(extension.toLowerCase()));
    const mode = signatureMode(head) ?? claimedMode(claimed, document.languageId);
    const language = routedLanguage(mode, document.languageId);
    if (language) void languages.setTextDocumentLanguage(document, language);
  };
  const refreshExtensions = async (): Promise<void> => {
    try {
      languageExtensions = (await client?.sendRequest<string[]>('nitride/languageExtensions', {})) ?? [];
      workspace.textDocuments.forEach(route);
    } catch {
      // The server may be restarting.
    }
  };
  // File events come in bursts (a checkout, a build): ask once they settle.
  let pending: ReturnType<typeof setTimeout> | undefined;
  const soon = (): void => {
    if (pending) clearTimeout(pending);
    pending = setTimeout(() => void refreshExtensions(), 500);
  };
  context.subscriptions.push(watcher.onDidCreate(soon), watcher.onDidChange(soon), watcher.onDidDelete(soon));
  void refreshExtensions();
  context.subscriptions.push(
    workspace.onDidOpenTextDocument(route),
    workspace.onDidChangeTextDocument(event => route(event.document)),
  );
  workspace.textDocuments.forEach(route);
  const testing = activateTesting(context, server.command);
  return { testing };
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
