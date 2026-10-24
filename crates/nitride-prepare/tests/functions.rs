//! Grammar functions expand like Nitrogen's GrammarFunctions.Expand. Expected
//! expansions were exported by the live Nitrogen reference.
use nitride_prepare::{Source, prepare_sources};
use serde_json::{Value, json};

const HEX: &str = "function HexDigit(d: Grammar.Int) : Grammar.Rule = if d < 10 then [char(48 + d)] else [char(55 + d) char(87 + d)];
function HexIn(width: Grammar.Int, values: Grammar.Class) : Grammar.Rule =
  if width == 0 then ()
  else choice d in 0 .. 15 {
    let size = 16 ^ (width - 1);
    let low = d * size;
    let part = values & [low .. (low + size - 1)];
    where part != [] : HexDigit(d) HexIn(width - 1, part - low)
  };
";

fn prepare(body: &str) -> Result<Value, nitride_prepare::PrepareError> {
    prepare_sources(
        &[Source {
            path: "test.ngr".into(),
            text: format!("syntax module Test {{\n{body}\n}}\n"),
        }],
        "{\"types\":[],\"operations\":[],\"statements\":{}}",
        "Test.Start",
        "syntax",
    )
}

fn rule(prepared: &Value, name: &str) -> Value {
    prepared["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == name)
        .unwrap()["expr"]
        .clone()
}

fn class(ranges: &[[u32; 2]]) -> Value {
    json!({"kind":"class","negated":false,"ranges":ranges})
}

#[test]
fn hex_digit_function_expands_like_nitrogen() {
    let prepared = prepare(&format!(
        "{HEX}token T = HexDigit(11);\nsyntax Start = Value:T;"
    ))
    .unwrap();
    assert_eq!(rule(&prepared, "Test.T"), class(&[[66, 66], [98, 98]]));
}

#[test]
fn comprehension_with_let_and_where() {
    let prepared = prepare(&format!(
        "{HEX}token T = HexIn(2, ['\\u0030'..'\\u0039' '\\u00aa']);\nsyntax Start = Value:T;"
    ))
    .unwrap();
    assert_eq!(
        rule(&prepared, "Test.T"),
        json!({"kind":"choice","items":[
            {"kind":"sequence","items":[class(&[[51, 51]]), class(&[[48, 57]])]},
            {"kind":"sequence","items":[class(&[[65, 65], [97, 97]]), class(&[[65, 65], [97, 97]])]}]})
    );
}

#[test]
fn contextual_word_seq() {
    let prepared = prepare(
        "function Word(w: Grammar.Text) : Grammar.Rule = \"@\"? (seq c in w { [c] [upper(c)]? });\ntoken T = Word(\"get\");\nsyntax Start = Value:T;",
    )
    .unwrap();
    let optional = |c: u32| json!({"kind":"repeat","min":0,"max":1,"inner":class(&[[c, c]])});
    assert_eq!(
        rule(&prepared, "Test.T"),
        json!({"kind":"sequence","items":[
            {"kind":"repeat","min":0,"max":1,"inner":{"kind":"literal","text":"@"}},
            class(&[[103, 103]]), optional(71), class(&[[101, 101]]), optional(69), class(&[[116, 116]]), optional(84)]})
    );
}

#[test]
fn syntax_rule_call_yields_qualified_references() {
    let prepared = prepare(
        "function Twice(r: Grammar.Rule) : Grammar.Rule = r r;\ntoken N = ['0'..'9'];\nsyntax Start = Value:Twice(N);",
    )
    .unwrap();
    assert_eq!(
        rule(&prepared, "Test.Start"),
        json!({"kind":"label","label":"Value","inner":{"kind":"sequence","items":[
            {"kind":"ref","name":"Test.N"},{"kind":"ref","name":"Test.N"}]}})
    );
}

#[test]
fn recursion_and_step_limits_are_errors() {
    let error = prepare("function Loop(n: Grammar.Int) : Grammar.Rule = Loop(n + 1);\ntoken T = Loop(0);\nsyntax Start = Value:T;")
        .unwrap_err();
    assert!(
        error.message.contains("nest deeper than 200"),
        "{}",
        error.message
    );
    let error = prepare("function Big(n: Grammar.Int) : Grammar.Rule = choice i in 0 .. n { \"a\" };\ntoken T = Big(1099511627776);\nsyntax Start = Value:T;")
        .unwrap_err();
    assert!(
        error.message.contains("more than 10000000 steps"),
        "{}",
        error.message
    );
}

#[test]
fn undeclared_call_is_rejected() {
    let error = prepare("token T = Missing(1);\nsyntax Start = Value:T;").unwrap_err();
    assert!(
        error.message.contains("unknown function 'Missing'"),
        "{}",
        error.message
    );
}

#[test]
fn unused_ill_typed_function_is_rejected() {
    let error = prepare("function Bad(n: Grammar.Int) : Grammar.Rule = [n] (if n then \"a\" else \"b\");\ntoken T = \"t\";\nsyntax Start = Value:T;")
        .unwrap_err();
    assert!(
        error.message.contains("expected a condition"),
        "{}",
        error.message
    );
}

#[test]
fn exponential_expansion_is_bounded() {
    let start = std::time::Instant::now();
    let error = prepare(
        "function D(n: Grammar.Int) : Grammar.Rule = if n == 0 then \"a\" else D(n - 1) D(n - 1);\ntoken T = D(40);\nsyntax Start = Value:T;",
    )
    .unwrap_err();
    assert!(
        error.message.contains("exceeds 1000000 nodes"),
        "{}",
        error.message
    );
    assert!(start.elapsed().as_secs() < 10, "took {:?}", start.elapsed());
}

#[test]
fn checker_errors_follow_source_order() {
    for _ in 0..16 {
        let error = prepare(
            "function A(n: Grammar.Int) : Grammar.Rule = [n] (if n then \"a\" else \"b\");\nfunction B(n: Grammar.Int) : Grammar.Rule = Missing(n);\ntoken T = \"t\";\nsyntax Start = Value:T;",
        )
        .unwrap_err();
        assert!(
            error.message.contains("expected a condition"),
            "{}",
            error.message
        );
    }
}
