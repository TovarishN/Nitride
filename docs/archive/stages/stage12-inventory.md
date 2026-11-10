# CSharp4 admission inventory

`scripts/csharp_inventory.py` copies the vendored CSharp4 grammar, stubs each
construct Nitride's frontend rejects, and retries preparation until it succeeds.
With six constructs stubbed, the full 30-module composition prepares.

| # | Construct | Example | Owning task |
|---|-----------|---------|-------------|
| 1 | Grammar functions | `Lexical.ngr`: `function HexIn(width: Grammar.Int, values: Grammar.Class) : Grammar.Rule = ...`, `token ContextualGet = Contextual("get");` | Task 5 |
| 2 | UTF-16 surrogate-pair classes | `GlobalTargetsThird.ngr`: `(['\ud834'] ['\udd73'] / "\\U" ...)` | Task 4 |
| 3 | `transparent` alternatives | `Statements.ngr`: `\| transparent StandardEmbeddedStatement = Body:(Block / ...)` | Task 2 |
| 4 | Infix/postfix operators that are rule references | `Expressions.ngr`: `\| InvocationExpression = Target:Expression Args:Arguments precedence 180 left` | Task 3 |
| 5 | Several infix/postfix alternatives sharing one literal operator | `Generics.ngr`: `\| GenericMemberExpression = Target:Expression "." Name:... Arguments:TypeArgumentList &GenericFollower precedence 180 left` beside `MemberExpression` | Task 5a (new) |
| 6 | Prefix alternatives with associativity | `Lambdas.ngr`: `\| LambdaExpression = Parameters:LambdaSignature "=>" Body:LambdaBody precedence 10 right` | Task 5b (new) |

Nitrogen semantics for the new gaps:

- Shared operators: extensible alternatives compete by longest match, then
  precedence (language guide, "Transparent alternatives"). Nitride's own
  "conflicting extension operator" admission rule has no Nitrogen counterpart.
- Prefix associativity: Nitrogen's generator ignores it for prefix
  alternatives. `SyntaxCodeWriter.OperandPrecedence` lowers the trailing
  operand only for `right` postfix alternatives.

Likely runtime divergences for Task 6 (not frontend gaps): a ranking tie in
Nitrogen produces an `Ambiguous` node whose children are the candidates, while
Nitride raises an ambiguity error. CSNitrogen documents ambiguity handling in
`docs/coverage/ambiguity-capabilities.md`.
