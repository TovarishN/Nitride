import * as fs from 'fs';
import * as path from 'path';
import { ExtensionContext, workspace } from 'vscode';
import { DocumentSelector, LanguageClient, LanguageClientOptions } from 'vscode-languageclient/node';
import { manifestExtensions } from './manifest';
import { serverCommand } from './server';

let client: LanguageClient | undefined;

/** File extensions of the languages every workspace folder's manifest declares. */
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
    { scheme: 'file', pattern: '**/{nitride,nitrogen}.json' },
    ...workspaceExtensions().map(extension => ({ scheme: 'file', pattern: `**/*${extension}` })),
  ];
  const watcher = workspace.createFileSystemWatcher('**/{nitride.json,nitrogen.json,*.ngr,*.json}');
  context.subscriptions.push(watcher);
  const options: LanguageClientOptions = { documentSelector: selector, synchronize: { fileEvents: watcher } };
  client = new LanguageClient('nitride', 'Nitride', server, options);
  await client.start();
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
