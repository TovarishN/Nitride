# Structural checks and named patterns

A syntax rule can declare diagnostics over its own nodes, written as in
Nitrogen. Nitride follows Nitrogen's semantics; the full reference is the
"Structural diagnostic matches" and "Named patterns and tree context" sections
of Nitrogen's `docs/language-guide.md`. `languages/xml/Xml.ngr` is Nitrogen's
`examples/XmlPatterns` example and uses every form below.

```ngr
syntax Paired = "<" Open:Name Attributes:Attribute* ">" Children:Node* "</" Close:Name ">"
{
  check XML0001 Open.Text == Close.Text : $"closing tag '{Close.Text}' does not match '{Open.Text}'" at Close;
  match (pass: Text) <[ <credentials ..$_>..$_<password ..$_>$pass</password>..$_</credentials> ]>
    => warning XML1001 : $"A password is stored directly inside credentials. {pass}";
}
```

- `check CODE A.Text == B.Text : message [at Field];` reports when the two
  fields' texts differ (or, with `!=`, are equal).
- `match (captures) <[ quote ]> => warning|error CODE : message;` reports when
  the node matches the quote, parsed as the rule. `$name` is one node of its
  capture's category, `..$name` any run of siblings, and a repeated capture
  must be structurally equal: the same rules and the same tokens, whatever
  whitespace lies between them (`<a/>` and `<a />` are the same). A capture's
  category must be a syntax rule, not a token (`NGR0146`); to capture a token,
  wrap it in a syntax rule, as in `syntax Text = Content;`, and capture that.
  The message is `$"..."` (with `{capture}` and
  `{Field.Text}` placeholders) or a plain `"..."`.

## Named patterns

```ngr
pattern Secret : Element =
  | <[ <password ..$_>..$_</password> ]>
  | <[ <api-key ..$_>..$_</api-key> ]>;
```

A module declares ordered alternatives over one category. A capture that names
a pattern, as in `(s: Secret)`, accepts only syntax the pattern matches. The
alternatives are tried in order and the first that matches is committed; each
alternative's own captures stay private to it. Patterns may refer to
themselves, as long as every cycle steps into a child (`NQP0008` otherwise).
They work in `match` checks and in `syntax function` match arms.

## Looking down and up

- `(s: ..Secret)` searches the hole's node and everything below it, in
  document order, and binds only the first node `Secret` matches. The check
  reports once, on its own node.
- `(p: ..Paired)`, with a plain syntax category instead of a pattern, binds
  the first node, in document order, that stands for the category (in a
  check, an alias category such as `Element` accepts any of its
  alternatives). **This is a Nitride extension:** Nitrogen requires a named
  pattern here and rejects the grammar with `NGR0163`. For a grammar that must
  prepare in both tools, write a pattern that accepts every node of the
  category instead:

  ```ngr
  pattern AnyPaired : Element = (e: Paired) <[ $e ]>;
  match (p: ..AnyPaired) <[ <vault ..$_>..$_ $p ..$_</vault> ]> => warning X1 : $"{p}";
  ```

- A descendant capture finds one node: it may occur only once in a pattern and
  cannot be a list (`NGR0166`).
- `inside [Category] [(captures)] <[ quote ]>` after a check's quote requires an
  ancestor (the nearest first) to match; `parent` checks only the parent. The
  check reports on each inner node, and the nearest matching ancestor supplies
  its captures. Ancestors whose syntax was recovered from errors are passed over.

## Limits

Each check on each node may take 1,000,000 steps (every node compared, choice,
alternative, descendant and ancestor visited) and a bounded amount of stack.
A check that runs out reports, as Nitrogen does, `NS0002`:
`check CODE failed: InvalidOperationException: NQP0006: The syntax-pattern match exceeded its work limit.`
In a function arm, running out fails the rewrite at its use.

## Problems preparation reports

| Code | Meaning |
|------|---------|
| NGR0146 | a capture, pattern or context category is a token, not a syntax rule |
| NGR0163 | a `..` capture names neither a pattern nor a syntax category |
| NGR0164 | a pattern repeats a rule, helper or pattern name |
| NGR0165 | a context capture repeats one of the pattern's captures |
| NGR0166 | a `..` capture is a list, or is used more than once |
| NQP0007 | prepared data names an unknown pattern |
| NQP0008 | patterns refer to each other without stepping into a child |
| NQP0009 | prepared data repeats a descendant capture |

The editor shows these codes, and offers definition, references, rename, hover
and completion for named patterns.
