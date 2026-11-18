// A tolerant structural grammar for editor highlighting. Nitride's language
// server remains the authority on syntax and semantics for each file type.
const declarationKinds = ['syntax', 'token', 'type', 'operation', 'function', 'pattern', 'context'];
const keywords = [
  ...declarationKinds, 'module', 'catalog', 'binding', 'language',
  'workspace', 'include', 'extensions', 'grammar', 'grammars', 'start',
  'trivia', 'nitrogen', 'namespace', 'source', 'sources', 'library', 'version',
  'using', 'usings', 'symbols', 'symbol', 'property', 'builtin', 'overload',
  'extensible', 'extend', 'out', 'in', 'hover', 'expected', 'for',
  'except', 'declares', 'references', 'scope', 'dynamic', 'sequential',
  'recursive', 'type', 'lowers', 'rewrites', 'literal', 'template', 'expand',
  'text', 'transparent', 'precedence', 'left', 'right', 'if', 'then', 'else',
  'choice', 'seq', 'let', 'where', 'with', 'match', 'inside', 'parent',
  'check', 'at', 'error', 'warning', 'show', 'call', 'infer', 'host', 'is',
  'wasm', 'component', 'process', 'command', 'arguments', 'timeout',
  'tests', 'project', 'test', 'file', 'expect', 'parse', 'compile', 'run',
  'accept', 'reject', 'values', 'diagnostic', 'code', 'span', 'none',
];

module.exports = grammar({
  name: 'nitride',
  extras: $ => [/\s/, $.comment],
  word: $ => $.identifier,
  rules: {
    source_file: $ => repeat($._item),
    _item: $ => choice(
      $.module_header, $.binding_header, $.language_header, $.declaration_header,
      $.block, $.parenthesized_group, $.bracket_group,
      $.raw_string, $.verbatim_string, $.string, $.character,
      $.signature, $.boolean, $.keyword, $.identifier, $.number,
      $.operator, $.punctuation,
    ),
    module_header: $ => prec.right(3, seq(choice('syntax', 'catalog'), 'module', field('name', $.qualified_name))),
    binding_header: $ => prec.right(2, seq('binding', field('name', $.qualified_name))),
    language_header: $ => prec.right(2, seq('language', field('name', $.identifier))),
    declaration_header: $ => prec.right(1, seq(choice(...declarationKinds), field('name', $.identifier))),
    qualified_name: $ => prec.right(seq($.identifier, repeat(seq('.', $.identifier)))),
    block: $ => seq('{', repeat($._item), '}'),
    parenthesized_group: $ => seq('(', repeat($._item), ')'),
    bracket_group: $ => seq('[', repeat($._item), ']'),
    keyword: _ => choice(...new Set(keywords)),
    boolean: _ => choice('true', 'false'),
    identifier: _ => /[\p{L}_][\p{L}\p{N}_]*/,
    number: _ => /[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?/,
    string: $ => seq('"', repeat(choice(token.immediate(prec(1, /[^"\\\r\n]+/)), $.escape_sequence)), '"'),
    character: $ => seq("'", repeat(choice(token.immediate(prec(1, /[^'\\\r\n]+/)), $.escape_sequence)), "'"),
    escape_sequence: _ => token.immediate(seq('\\', choice(/u[0-9a-fA-F]{4}/, /[^\r\n]/))),
    verbatim_string: _ => token(seq(choice('@"', '$@"', '@$"'), repeat(choice('""', /[^"]/)), '"')),
    raw_string: _ => token(prec(2, seq('<[', /([^\]]|\]+[^\]>])*/, /\]+>/))),
    signature: _ => token(prec(3, /#!nitrogen[^\r\n]*/)),
    comment: _ => token(choice(
      seq('//', /[^\r\n]*/),
      seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/'),
    )),
    operator: _ => /[=+\-*/!?&|<>~^%@$#\\`]+/,
    punctuation: _ => choice('..', '.', ':', ';', ','),
  },
});
