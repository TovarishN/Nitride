import * as fs from 'fs';
import * as path from 'path';
import { ExtensionContext, languages, Range, TextDocument, workspace } from 'vscode';
import { DocumentSelector, LanguageClient, LanguageClientOptions } from 'vscode-languageclient/node';
import { manifestExtensions } from './manifest';
import { serverCommand } from './server';
import { routedLanguage, signatureMode } from './signatures';

let client: LanguageClient | undefined;

/** File extensions of the languages every workspace folder's nitrogen.json declares. */
function workspaceExtensions(): string[] {
  const folders = workspace.workspaceFolders ?? [];
  return [...new Set(folders.flatMap(folder => manifestExtensions(name => {
    try {
      return fs.readFileSync(path.join(folder.uri.fsPath, name), 'utf8');
    } catch {
      return undefined;
    }
  })))];
}

export async function activate(context: ExtensionContext): Promise<void> {
  const server = serverCommand({
    serverPath: workspace.getConfiguration('nitride').get<string>('server.path') ?? '',
    extensionPath: context.extensionPath,
    platform: process.platform,
    exists: file => fs.existsSync(file),
  });
  const selector: DocumentSelector = [
    { scheme: 'file', language: 'ngr' },
    { scheme: 'file', language: 'ncatalog' },
    { scheme: 'file', language: 'nproj' },
    { scheme: 'file', pattern: '**/nitrogen.json' },
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
    // Colour with the theme's TextMate palette (contributes.semanticTokenScopes).
    initializationOptions: { paletteTokens: true },
  };
  client = new LanguageClient('nitride', 'Nitride', server, options);
  await client.start();
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
    const mode = signatureMode(head) ?? (claimed && document.languageId !== 'ngr' ? 'language' : undefined);
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
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
