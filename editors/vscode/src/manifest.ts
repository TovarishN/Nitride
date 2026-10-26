/** The workspace manifests Nitride reads, in order: the first present one wins. */
export const manifestNames = ['nitride.json', 'nitrogen.json'];

/**
 * The file extensions (with their dot) of the languages in a folder's manifest: nitride.json, else
 * nitrogen.json. `read` returns a manifest's text, or undefined when it does not exist.
 */
export function manifestExtensions(read: (name: string) => string | undefined): string[] {
  const text = manifestNames.map(read).find(t => t !== undefined);
  if (text === undefined) return [];
  try {
    const config = JSON.parse(text) as { languages?: { extensions?: string[] }[] };
    const extensions = (config.languages ?? []).flatMap(l => l.extensions ?? []);
    return [...new Set(extensions.map(e => (e.startsWith('.') ? e : `.${e}`)))];
  } catch {
    return [];
  }
}
