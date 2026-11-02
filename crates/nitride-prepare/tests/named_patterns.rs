//! Nitrogen PR 92's named patterns, descendant captures and match contexts:
//! how they lower into prepared data, and the problems preparation reports.
use nitride_prepare::*;
use serde_json::{Value, json};
use std::path::Path;

const EMPTY_CATALOG: &str = r#"{"types":[],"operations":[],"statements":{}}"#;

/// A small XML grammar; `CHECKS` goes in `Paired`'s check block and
/// `MEMBERS` at the end of the module.
const XML: &str = r#"syntax module Xml
{
  token Name = ['a'..'z' 'A'..'Z' '_' ':'] ['a'..'z' 'A'..'Z' '0'..'9' '_' ':' '-' '.']*;
  token Quoted = "\"" [^ '"' '<']* "\"" / "'" [^ '\'' '<']* "'";
  token Content = [^ '<']+;
  syntax Document = Root:Element;
  syntax Element = Empty / Paired;
  syntax Empty = "<" Tag:Name Attributes:Attribute* "/>";
  syntax Paired = "<" Open:Name Attributes:Attribute* ">" Children:Node* "</" Close:Name ">"
  {
    check XML0001 Open.Text == Close.Text : $"closing tag '{Close.Text}' does not match '{Open.Text}'" at Close;
CHECKS
  }
  syntax Attribute = Name "=" Quoted;
  syntax Node = Element / Text;
  syntax Text = Content;
MEMBERS
}"#;

const SECRET: &str = "  pattern Secret : Element =
    | <[ <password ..$_>..$_</password> ]>
    | <[ <api-key ..$_>..$_</api-key> ]>;";

const XML1002: &str = r#"    match (secret: ..Secret)
      <[ <vault ..$_>..$_ $secret ..$_</vault> ]>
      => warning XML1002 : $"A vault stores a plaintext secret: {secret}";"#;

const XML1003: &str = r#"    match <[ <api-key ..$_>..$_</api-key> ]>
      inside <[ <service ..$_>..$_</service> ]>
      => warning XML1003 : "An API key is embedded in a service definition; reference it instead.";"#;

fn prepare(checks: &str, members: &str) -> Result<Value, PrepareError> {
    let text = XML.replace("CHECKS", checks).replace("MEMBERS", members);
    let source = Source {
        path: "Xml.ngr".into(),
        text,
    };
    prepare_sources_tolerant(&[source], EMPTY_CATALOG, "Xml.Document").map(|p| p.data)
}

fn check<'a>(data: &'a Value, code: &str) -> &'a Value {
    data["rules"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["checks"].as_array().into_iter().flatten())
        .find(|c| c["code"] == code)
        .unwrap_or_else(|| panic!("no check {code}"))
}

fn error(checks: &str, members: &str) -> PrepareError {
    prepare(checks, members).expect_err("preparation should fail")
}

#[test]
fn prepares_named_pattern() {
    let data = prepare("", SECRET).unwrap();
    assert_eq!(
        data["patterns"],
        json!([{
            "name": "Xml.Secret",
            "category": "Xml.Element",
            "alternatives": [
                {"captures": [], "pattern": " <password ..$_>..$_</password> "},
                {"captures": [], "pattern": " <api-key ..$_>..$_</api-key> "}
            ]
        }])
    );
}

#[test]
fn descendant_capture_and_context_lower() {
    let data = prepare(&format!("{XML1002}\n{XML1003}"), SECRET).unwrap();
    assert_eq!(
        check(&data, "XML1002")["captures"],
        json!([{"name": "secret", "category": "Xml.Element", "pattern": "Xml.Secret", "descendant": true}])
    );
    assert_eq!(
        check(&data, "XML1003")["context"],
        json!({
            "kind": "inside",
            "category": "Xml.Paired",
            "captures": [],
            "pattern": " <service ..$_>..$_</service> "
        })
    );
    assert!(check(&data, "XML1002").get("context").is_none());
}

#[test]
fn plain_string_message_is_accepted() {
    let data = prepare(XML1003, "").unwrap();
    assert_eq!(
        check(&data, "XML1003")["message"],
        "An API key is embedded in a service definition; reference it instead."
    );
}

#[test]
fn context_names_its_category_and_captures() {
    let checks = r#"    match <[ <api-key ..$_>..$_</api-key> ]>
      parent Paired (t: Text) <[ <service ..$_>$t ..$_</service> ]>
      => warning XML1004 : $"An API key directly inside a service: {t}";"#;
    let data = prepare(checks, "").unwrap();
    assert_eq!(
        check(&data, "XML1004")["context"],
        json!({
            "kind": "parent",
            "category": "Xml.Paired",
            "captures": [{"name": "t", "category": "Xml.Text"}],
            "pattern": " <service ..$_>$t ..$_</service> "
        })
    );
}

fn assert_code(error: &PrepareError, code: &str, message: &str) {
    assert_eq!(error.code.as_deref(), Some(code), "{error:?}");
    assert_eq!(error.message, message);
}

#[test]
fn ngr0163_unknown_descendant_pattern() {
    let checks =
        r#"    match (s: ..Nope) <[ <vault ..$_>..$_ $s ..$_</vault> ]> => warning X1 : "x";"#;
    assert_code(
        &error(checks, ""),
        "NGR0163",
        "'Nope' is not a named pattern; a descendant capture needs one",
    );
}

#[test]
fn ngr0164_duplicate_pattern() {
    assert_code(
        &error("", &format!("{SECRET}\n{SECRET}")),
        "NGR0164",
        "duplicate rule/helper/pattern 'Secret'",
    );
}

#[test]
fn ngr0164_pattern_named_like_a_rule() {
    let members = "  pattern Text : Element = <[ <x/> ]>;";
    assert_code(
        &error("", members),
        "NGR0164",
        "duplicate rule/helper/pattern 'Text'",
    );
}

#[test]
fn ngr0165_context_capture_collision() {
    let checks = r#"    match (t: Text) <[ <api-key ..$_>$t</api-key> ]>
      inside (t: Text) <[ <service ..$_>$t ..$_</service> ]>
      => warning X1 : "x";"#;
    assert_code(
        &error(checks, ""),
        "NGR0165",
        "capture 't' is already a capture of the matched pattern; rename one",
    );
}

#[test]
fn ngr0166_descendant_list() {
    let checks = r#"    match (s: ..Secret*) <[ <vault ..$_>$s</vault> ]> => warning X1 : "x";"#;
    assert_code(
        &error(checks, SECRET),
        "NGR0166",
        "descendant capture 's' finds one node; it cannot be a list",
    );
}

#[test]
fn ngr0166_descendant_twice() {
    let checks = r#"    match (s: ..Secret) <[ <vault ..$_>$s $s</vault> ]> => warning X1 : "x";"#;
    assert_code(
        &error(checks, SECRET),
        "NGR0166",
        "descendant capture 's' may occur only once in a pattern",
    );
}

#[test]
fn nqp0008_non_consuming_cycle() {
    let members = "  pattern A : Element = (x: A) <[ $x ]>;";
    assert_code(
        &error("", members),
        "NQP0008",
        "Named patterns refer to each other without consuming syntax: Xml.A -> Xml.A.",
    );
}

#[test]
fn nqp0008_cycle_through_descendant() {
    let members = "  pattern A : Element = (x: ..B) <[ $x ]>;
  pattern B : Element = (y: A) <[ $y ]>;";
    assert_code(
        &error("", members),
        "NQP0008",
        "Named patterns refer to each other without consuming syntax: Xml.A -> Xml.B -> Xml.A.",
    );
}

#[test]
fn recursion_that_consumes_is_accepted() {
    let members = "  pattern Nested : Element =
    | (n: Nested) <[ <box>$n</box> ]>
    | <[ <leaf/> ]>;";
    let data = prepare("", members).unwrap();
    assert_eq!(
        data["patterns"][0]["alternatives"][0]["captures"],
        json!([{"name": "n", "category": "Xml.Element", "pattern": "Xml.Nested"}])
    );
}

#[test]
fn existing_grammars_prepare_unchanged() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for profile in ["xml", "portable"] {
        let prepared = prepare_project(&root.join(format!("languages/{profile}/project.json")))
            .unwrap()
            .data;
        let fixture: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join(format!(
                "crates/nitride-prepare/tests/fixtures/named_patterns/{profile}.json"
            )))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(prepared, fixture, "{profile}");
    }
}
