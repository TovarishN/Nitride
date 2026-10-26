/** A document's Nitrogen signature, from its leading trivia: a grammar, or a program choosing its language. */
export type SignatureMode = 'grammar' | 'language';

const directive = /^#!nitrogen\s+(grammar|language)\b/;

/**
 * Nitrogen's signatureDocumentMode, without augmentations (Nitride does not serve them): directives may
 * follow a BOM, blank lines, and `//`, `/* *\/` or `<!-- -->` comments. Only a hint for routing; the
 * server validates signatures.
 */
export function signatureMode(text: string): SignatureMode | undefined {
  let rest = text.replace(/^﻿/, '');
  while (rest.length) {
    rest = rest.trimStart();
    if (rest.startsWith('#!nitrogen')) {
      const mode = directive.exec(rest)?.[1] as SignatureMode | undefined;
      if (mode) return mode;
      const end = rest.search(/[\r\n]/);
      rest = end < 0 ? '' : rest.slice(end + 1);
    } else if (rest.startsWith('//')) {
      const end = rest.search(/[\r\n]/);
      const mode = directive.exec((end < 0 ? rest : rest.slice(0, end)).slice(2).trimStart())?.[1] as SignatureMode | undefined;
      if (mode) return mode;
      rest = end < 0 ? '' : rest.slice(end + 1);
    } else if (rest.startsWith('/*') || rest.startsWith('<!--')) {
      const html = rest.startsWith('<!--');
      const end = rest.indexOf(html ? '-->' : '*/');
      if (end < 0) break;
      const mode = rest.slice(html ? 4 : 2, end).split(/\r\n|\r|\n/)
        .map(line => directive.exec(line.trimStart())?.[1] as SignatureMode | undefined)
        .find(found => found !== undefined);
      if (mode) return mode;
      rest = rest.slice(end + (html ? 3 : 2));
    } else break;
  }
  return undefined;
}
