/** The workspace manifests Nitride reads, in order: the first present one wins. */
export const manifestNames = ['nitride.nproj', 'nitrogen.json'];

/** `.nproj` text without comments. */
function code(text: string): string {
  return text.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/\/\/[^\n]*/g, ' ');
}

/** The extensions of the languages a `nitride.nproj` includes: a routing hint; the server validates. */
function projectExtensions(
  text: string,
  read: (path: string) => string | undefined,
  list: (dir: string) => string[],
): string[] {
  const extensions: string[] = [];
  for (const [, dir] of code(text).matchAll(/\binclude\s+([^;\s]+)\s*;/g)) {
    const files = list(dir).filter(name => name.endsWith('.nproj'));
    const language = files.length === 1 ? read(`${dir}/${files[0]}`) : undefined;
    for (const [, items] of code(language ?? '').matchAll(/\bextensions\s+([^;]*);/g)) {
      extensions.push(...items.split(',').map(e => e.trim()).filter(e => e));
    }
  }
  return extensions;
}

/**
 * The file extensions (with their dot) of the languages a folder declares: those `nitride.nproj`
 * includes, else those of Nitrogen's `nitrogen.json`. `read` returns a file's text by its path relative
 * to the folder (undefined when it does not exist); `list` the file names in a relative directory.
 */
export function manifestExtensions(
  read: (path: string) => string | undefined,
  list: (dir: string) => string[] = () => [],
): string[] {
  let extensions: string[] = [];
  const project = read(manifestNames[0]);
  if (project !== undefined) {
    extensions = projectExtensions(project, read, list);
  } else {
    const text = read(manifestNames[1]);
    if (text === undefined) return [];
    try {
      const config = JSON.parse(text) as { languages?: { extensions?: string[] }[] };
      extensions = (config.languages ?? []).flatMap(l => l.extensions ?? []);
    } catch {
      return [];
    }
  }
  return [...new Set(extensions.map(e => (e.startsWith('.') ? e : `.${e}`)))];
}
