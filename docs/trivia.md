# Language trivia policies

A project can choose the whitespace and comments accepted between tokens:

```ngr
language Json
{
  extensions .json;
  grammar Json.ngr;
  start Json.Document;
  trivia json;
}
```

The equivalent JSON manifest member is `"trivia": "json"`.

- `nitrogen` is the default: Unicode whitespace and C-style line/block comments.
  Existing unterminated-comment handling remains controlled by parser options.
- `json` accepts only space, tab, CR and LF between tokens, with no comments.
  Characters inside a token still follow that token's grammar.

Unknown policy names and duplicate `.nproj` policy declarations are errors.
Preparation includes a non-default policy in the language identity. Generated
parsers, runtime-loaded parsers, recovery, snapshot edits and editor projects
use the same policy. A project-policy edit reloads the editor's language and
invalidates its cached syntax.

The shipped JSON project selects `json`. Its generated parser is checked against
pinned JSONTestSuite `y_` acceptance and `n_` rejection cases, plus the existing
handwritten fixtures. `i_` cases are implementation-defined. Parsing remains
subject to work/depth limits; a limit error is distinct from invalid grammar.
This does not add string unescaping, number conversion or a JSON DOM API.

Trivia policies are a Nitride project setting. They do not translate arbitrary
Nitrogen semantic helpers or add custom user-defined trivia rules.
