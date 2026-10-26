import * as path from 'path';

/** What choosing the server depends on: the setting, the extension's folder and the file system. */
export interface ServerInputs {
  /** nitride.server.path */
  serverPath: string;
  extensionPath: string;
  platform: string;
  exists: (file: string) => boolean;
}

/**
 * The server to start: the executable set in the settings; else the native binary bundled in
 * server/; else nitride from PATH (an extension built from source, without a bundled server).
 */
export function serverCommand(inputs: ServerInputs): { command: string; args: string[] } {
  if (inputs.serverPath) return { command: inputs.serverPath, args: ['lsp'] };
  const bundled = path.join(inputs.extensionPath, 'server', inputs.platform === 'win32' ? 'nitride.exe' : 'nitride');
  return { command: inputs.exists(bundled) ? bundled : 'nitride', args: ['lsp'] };
}
