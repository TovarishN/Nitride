/**
 * Nitrogen's manifest, read here only until Nitrogen reads `.nproj`: `nitride.nproj` workspaces get
 * their extensions from the server (`nitride/languageExtensions`).
 */
export const manifestNames = ['nitrogen.json'];

/**
 * The file extensions (with their dot) of the languages in a folder's nitrogen.json. `read` returns a
 * manifest's text, or undefined when it does not exist.
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
