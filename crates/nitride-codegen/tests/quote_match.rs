//! The budgeted quote matcher: named patterns, recursion, descendant search,
//! work limits and the two flavors (checks and function arms).
use nitride_codegen::LoadedLanguage;
use nitride_core::quote_match::{Budget, Flavor, MAX_WORK_STEPS, Patterns, Quote};
use nitride_core::*;
use nitride_prepare::*;

const EMPTY_CATALOG: &str = r#"{"types":[],"operations":[],"statements":{}}"#;

/// A small XML grammar; `CHECKS` goes in `Paired`'s check block and
/// `MEMBERS` at the end of the module.
const XML: &str = r#"syntax module Xml
{
  token Name = ['a'..'z' 'A'..'Z' '_' ':'] ['a'..'z' 'A'..'Z' '0'..'9' '_' ':' '-' '.']*;
  token Quoted = "\"" [^ '"' '<']* "\"";
  token Content = [^ '<']+;
  syntax Document = Root:Element;
  syntax Element = Empty / Paired;
  syntax Empty = "<" Tag:Name Attributes:Attribute* "/>";
  syntax Paired = "<" Open:Name Attributes:Attribute* ">" Children:Node* "</" Close:Name ">"
  {
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

fn data(checks: &str, members: &str) -> serde_json::Value {
    let text = XML.replace("CHECKS", checks).replace("MEMBERS", members);
    let source = Source {
        path: "Xml.ngr".into(),
        text,
    };
    prepare_sources_tolerant(&[source], EMPTY_CATALOG, "Xml.Document")
        .unwrap()
        .data
}

fn language(checks: &str, members: &str) -> LoadedLanguage {
    LoadedLanguage::load(&data(checks, members).to_string()).unwrap()
}

/// `(code, matched text)` of each check diagnostic over `source`.
fn found(language: &LoadedLanguage, source: &str) -> Vec<(String, String)> {
    let language = language.language();
    let tree = parse(language, source, Limits::default()).unwrap();
    rule_diagnostics(language, &tree)
        .into_iter()
        .map(|d| {
            let text = source[d.span[0] as usize..d.span[1] as usize].to_owned();
            (d.code.unwrap().into(), text)
        })
        .collect()
}

/// The outermost element of a parsed document.
fn outer(language: &Language<'_>, tree: &SyntaxTree<'_>) -> u32 {
    (0..tree.nodes.len() as u32)
        .filter(|&id| language.rules[tree.nodes[id as usize].rule as usize].name == "Xml.Paired")
        .max_by_key(|&id| {
            let span = tree.nodes[id as usize].span;
            span[1] - span[0]
        })
        .unwrap()
}

fn found_codes(language: &LoadedLanguage, source: &str) -> Vec<String> {
    found(language, source)
        .into_iter()
        .map(|(c, _)| c)
        .collect()
}

const SECRET_IN_VAULT: &str =
    r#"    match (s: Secret) <[ <vault>$s</vault> ]> => warning X1 : $"secret {s}";"#;

#[test]
fn alternatives_fall_through_and_commit() {
    let language = language(SECRET_IN_VAULT, SECRET);
    assert_eq!(
        found_codes(&language, "<vault><password>p</password></vault>"),
        ["X1"]
    );
    assert_eq!(
        found_codes(&language, "<vault><api-key>k</api-key></vault>"),
        ["X1"]
    );
    assert!(found_codes(&language, "<vault><other>o</other></vault>").is_empty());
}

#[test]
fn reference_message_shows_the_referenced_node() {
    let language = language(SECRET_IN_VAULT, SECRET);
    let tree_source = "<vault><api-key>k</api-key></vault>";
    let tree = parse(language.language(), tree_source, Limits::default()).unwrap();
    let diagnostics = rule_diagnostics(language.language(), &tree);
    assert_eq!(diagnostics[0].message, "secret <api-key>k</api-key>");
}

/// A first alternative that binds `v` and then fails leaves no `v` behind,
/// and the caller never sees an alternative's captures.
#[test]
fn private_captures_do_not_leak_and_roll_back() {
    let members = "  pattern Token : Element =
    | (v: Text) <[ <token>$v</token> ]>
    | (w: Text) <[ <key>$w</key> ]>;";
    let language = language("", members);
    let language = language.language();
    let patterns = Patterns::compile(language).unwrap();
    let element = language
        .rules
        .iter()
        .position(|r| r.name == "Xml.Element")
        .unwrap() as u16;
    let hole = Hole {
        name: "t",
        category: element,
        pattern: Some(0),
        descendant: false,
    };
    let quote = Quote::compile(
        language,
        element,
        Box::leak(Box::new([hole])),
        "<vault>$t</vault>",
        Flavor::Check,
    )
    .unwrap();
    let tree = parse(language, "<vault><key>k</key></vault>", Limits::default()).unwrap();
    let mut bindings = Vec::new();
    let matched = quote
        .matches(
            &patterns,
            &tree,
            outer(language, &tree),
            &mut Budget::new(MAX_WORK_STEPS),
            &mut bindings,
        )
        .unwrap();
    assert!(matched);
    let names: Vec<_> = bindings.iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["t"]);
}

const NESTED: &str = "  pattern Nested : Element =
    | (n: Nested) <[ <box>$n</box> ]>
    | <[ <leaf/> ]>;";

#[test]
fn recursive_pattern_matches_at_several_depths() {
    let checks = r#"    match (n: Nested) <[ <root>$n</root> ]> => warning X1 : "nested";"#;
    let language = language(checks, NESTED);
    assert_eq!(found_codes(&language, "<root><leaf/></root>"), ["X1"]);
    assert_eq!(
        found_codes(&language, "<root><box><box><leaf/></box></box></root>"),
        ["X1"]
    );
    assert!(found_codes(&language, "<root><box><other/></box></root>").is_empty());
}

const DESCENDANT: &str = r#"    match (s: ..Secret) <[ <vault ..$_>..$_ $s ..$_</vault> ]> => warning X2 : $"found {s}";"#;

const DESCENDANT_PAIRED: &str = r#"    match (p: ..Paired) <[ <vault ..$_>..$_ $p ..$_</vault> ]> => warning X4 : $"found {p}";"#;

fn messages(language: &LoadedLanguage, source: &str) -> Vec<String> {
    let tree = parse(language.language(), source, Limits::default()).unwrap();
    rule_diagnostics(language.language(), &tree)
        .into_iter()
        .map(|d| d.message)
        .collect()
}

/// A Nitride extension (issue #33): a descendant capture over a plain
/// category binds the first node, in document order, of that category.
#[test]
fn descendant_over_plain_category_binds_first_in_document_order() {
    let language = language(DESCENDANT_PAIRED, "");
    assert_eq!(
        messages(&language, "<vault><y/><a><b>1</b></a><c>2</c></vault>"),
        ["found <a><b>1</b></a>"]
    );
}

#[test]
fn descendant_over_plain_category_without_hit_reports_nothing() {
    let language = language(DESCENDANT_PAIRED, "");
    assert!(messages(&language, "<vault><y/>text<z/></vault>").is_empty());
}

#[test]
fn descendant_over_alias_category_accepts_its_alternatives() {
    let checks = r#"    match (e: ..Element) <[ <vault ..$_>..$_ $e ..$_</vault> ]> => warning X5 : $"found {e}";"#;
    let language = language(checks, "");
    assert_eq!(
        messages(&language, "<vault>t<y/><a/></vault>"),
        ["found <y/>"]
    );
}

#[test]
fn descendant_hits_depth_zero() {
    let language = language(DESCENDANT, SECRET);
    assert_eq!(
        found(&language, "<vault><password>p</password></vault>")[0].0,
        "X2"
    );
}

#[test]
fn descendant_hits_deep() {
    let language = language(DESCENDANT, SECRET);
    let source = "<vault><a><b><api-key>k</api-key></b></a></vault>";
    let tree = parse(language.language(), source, Limits::default()).unwrap();
    let diagnostics = rule_diagnostics(language.language(), &tree);
    assert_eq!(diagnostics.len(), 1, "one report on the vault");
    assert_eq!(diagnostics[0].message, "found <api-key>k</api-key>");
}

#[test]
fn descendant_none() {
    let language = language(DESCENDANT, SECRET);
    assert!(found_codes(&language, "<vault><a><b>text</b></a></vault>").is_empty());
}

#[test]
fn descendant_moves_to_next_candidate() {
    let language = language(DESCENDANT, SECRET);
    let source = "<vault><folder><x/><password>p</password></folder></vault>";
    let tree = parse(language.language(), source, Limits::default()).unwrap();
    let diagnostics = rule_diagnostics(language.language(), &tree);
    assert_eq!(diagnostics[0].message, "found <password>p</password>");
}

#[test]
fn spread_backtracks_into_fresh_descendant_search() {
    let language = language(DESCENDANT, SECRET);
    let source = "<vault><a>none</a><b><password>p</password></b></vault>";
    let tree = parse(language.language(), source, Limits::default()).unwrap();
    let diagnostics = rule_diagnostics(language.language(), &tree);
    assert_eq!(diagnostics[0].message, "found <password>p</password>");
}

#[test]
fn budget_exhaustion_returns_limit_exceeded() {
    let language = language("", SECRET);
    let language = language.language();
    let patterns = Patterns::compile(language).unwrap();
    let paired = language
        .rules
        .iter()
        .position(|r| r.name == "Xml.Paired")
        .unwrap() as u16;
    let quote = Quote::compile(
        language,
        paired,
        &[],
        "<vault ..$_>..$_</vault>",
        Flavor::Check,
    )
    .unwrap();
    let wide = format!("<vault>{}</vault>", "<x/>".repeat(200));
    let tree = parse(language, &wide, Limits::default()).unwrap();
    let result = quote.matches(
        &patterns,
        &tree,
        outer(language, &tree),
        &mut Budget::new(10),
        &mut Vec::new(),
    );
    assert!(result.is_err());
}

#[test]
fn work_limit_reports_ns0002() {
    let checks = r#"    match (n: Nested) <[ <root>$n</root> ]> => warning X1 : "nested";"#;
    let language = language(checks, NESTED);
    let deep = format!(
        "<root>{}<leaf/>{}</root>",
        "<box>".repeat(300),
        "</box>".repeat(300)
    );
    let tree = parse(language.language(), &deep, Limits::default()).unwrap();
    let diagnostics = nitride_core::rule_diagnostics_with(language.language(), &tree, 50);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code.as_deref(), Some("NS0002"));
    assert_eq!(
        diagnostics[0].message,
        "check X1 failed: InvalidOperationException: NQP0006: The syntax-pattern match exceeded its work limit."
    );
}

#[test]
fn deep_input_does_not_overflow() {
    let checks = format!(
        "{}\n{}",
        r#"    match (n: Nested) <[ <root>$n</root> ]> => warning X1 : "nested";"#,
        r#"    match (s: ..Secret) <[ <root ..$_>..$_ $s ..$_</root> ]> => warning X2 : "secret";"#
    );
    let language = language(&checks, &format!("{SECRET}\n{NESTED}"));
    let deep = format!(
        "<root>{}<password>p</password>{}</root>",
        "<box>".repeat(1000),
        "</box>".repeat(1000)
    );
    let tree = parse(language.language(), &deep, Limits::default()).unwrap();
    // A result or NS0002 for each check, never a crash.
    let diagnostics = rule_diagnostics(language.language(), &tree);
    assert!(diagnostics.len() <= 2, "{diagnostics:?}");
}

/// Checks compare a repeated capture by structure, as Nitrogen's
/// `QuotePattern.Match` does: interior trivia does not count.
#[test]
fn check_flavor_compares_repeated_captures_by_structure() {
    let checks = r#"    match (e: Element) <[ <pair>$e $e</pair> ]> => warning X3 : "pair";"#;
    let language = language(checks, "");
    assert_eq!(found_codes(&language, "<pair><a/><a/></pair>"), ["X3"]);
    assert_eq!(found_codes(&language, "<pair><a/><a /></pair>"), ["X3"]);
    assert_eq!(
        found_codes(&language, r#"<pair><a x="1"/><a  x="1"/></pair>"#),
        ["X3"]
    );
    assert!(found_codes(&language, "<pair><a/><b/></pair>").is_empty());
    assert!(found_codes(&language, r#"<pair><a x="1"/><a x="2"/></pair>"#).is_empty());
    assert!(found_codes(&language, "<pair><a/><a></a></pair>").is_empty());
}

#[test]
fn arm_flavor_keeps_same_syntax_equality() {
    let language = language("", "");
    let language = language.language();
    let paired = language
        .rules
        .iter()
        .position(|r| r.name == "Xml.Paired")
        .unwrap() as u16;
    // Arms take the hole's own category (or one it owns), not an alias member.
    let empty = language
        .rules
        .iter()
        .position(|r| r.name == "Xml.Empty")
        .unwrap() as u16;
    let holes: &'static [Hole<'static>] = Box::leak(Box::new([Hole {
        name: "e",
        category: empty,
        pattern: None,
        descendant: false,
    }]));
    let patterns = Patterns::compile(language).unwrap();
    let quote = Quote::compile(language, paired, holes, "<pair>$e $e</pair>", Flavor::Arm).unwrap();
    let tree = parse(language, "<pair><a/><a /></pair>", Limits::default()).unwrap();
    let mut budget = Budget::new(MAX_WORK_STEPS);
    let paired_node = outer(language, &tree);
    assert!(
        quote
            .matches(&patterns, &tree, paired_node, &mut budget, &mut Vec::new())
            .unwrap()
    );
}

#[test]
fn compile_rejects_unknown_pattern_nqp0007() {
    let mut data = data(SECRET_IN_VAULT, SECRET);
    data["patterns"] = serde_json::json!([]);
    let error = LoadedLanguage::load(&data.to_string()).err().unwrap();
    assert!(
        error.contains("NQP0007: 'Xml.Secret' is not a named pattern of this language."),
        "{error}"
    );
}

#[test]
fn compile_rejects_repeated_descendant_nqp0009() {
    let mut data = data(DESCENDANT, SECRET);
    for rule in data["rules"].as_array_mut().unwrap() {
        for check in rule["checks"].as_array_mut().into_iter().flatten() {
            if check["code"] == "X2" {
                check["pattern"] = serde_json::json!(" <vault>$s $s</vault> ");
            }
        }
    }
    let error = LoadedLanguage::load(&data.to_string()).err().unwrap();
    assert!(error.contains("NQP0009"), "{error}");
}

const XML1003: &str = r#"    match <[ <api-key ..$_>..$_</api-key> ]>
      inside <[ <service ..$_>..$_</service> ]>
      => warning XML1003 : "An API key is embedded in a service definition; reference it instead.";"#;

const SERVICES: &str = r#"<application><service name="search"><settings><api-key>nested</api-key></settings><api-key>direct</api-key></service><vault><api-key>kept</api-key></vault></application>"#;

#[test]
fn inside_reports_once_per_inner_item() {
    let language = language(XML1003, "");
    let found = found(&language, SERVICES);
    assert_eq!(
        found,
        [
            ("XML1003".to_owned(), "<api-key>nested</api-key>".to_owned()),
            ("XML1003".to_owned(), "<api-key>direct</api-key>".to_owned()),
        ]
    );
}

#[test]
fn parent_checks_only_the_semantic_parent() {
    let checks = r#"    match <[ <api-key ..$_>..$_</api-key> ]>
      parent <[ <service ..$_>..$_</service> ]>
      => warning X4 : "direct";"#;
    let language = language(checks, "");
    let found = found(&language, SERVICES);
    assert_eq!(
        found,
        [("X4".to_owned(), "<api-key>direct</api-key>".to_owned())]
    );
}

#[test]
fn nearest_ancestor_supplies_captures() {
    let checks = r#"    match <[ <api-key ..$_>..$_</api-key> ]>
      inside (t: Text) <[ <group ..$_>$t ..$_</group> ]>
      => warning X5 : $"in {t}";"#;
    let language = language(checks, "");
    let source = "<group>outer<group>inner<api-key>k</api-key></group></group>";
    let tree = parse(language.language(), source, Limits::default()).unwrap();
    let diagnostics = rule_diagnostics(language.language(), &tree);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "in inner");
}

#[test]
fn broken_ancestor_is_skipped() {
    let language = language(XML1003, "");
    let language = language.language();
    for source in [
        // A missing attribute value: a damaged node.
        r#"<application><service name="search"><api-key>k</api-key><x a=></x></service></application>"#,
        // An unquoted value: recovery inserts the missing `>`.
        r#"<application><service name=search><api-key>k</api-key></service></application>"#,
        // An unterminated quote: recovery skips text.
        r#"<application><service name="s"><api-key>k</api-key><settings name="x></settings></service></application>"#,
    ] {
        let parsed = parse_recovering(
            language,
            source,
            Limits::default(),
            RecoveryOptions::default(),
        );
        assert!(parsed.tree.recovery().is_some(), "{source}");
        let diagnostics = rule_diagnostics(language, &parsed.tree);
        assert!(
            diagnostics.iter().all(
                |d| d.code.as_deref() != Some("XML1003") && d.code.as_deref() != Some("NS0002")
            ),
            "{source}: {diagnostics:?}"
        );
    }
}

#[test]
fn context_budget_exhaustion_reports_ns0002() {
    let language = language(XML1003, "");
    let deep = format!(
        "<service>{}<api-key>k</api-key>{}</service>",
        "<box>".repeat(300),
        "</box>".repeat(300)
    );
    let tree = parse(language.language(), &deep, Limits::default()).unwrap();
    let diagnostics = nitride_core::rule_diagnostics_with(language.language(), &tree, 100);
    let failed: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("NS0002"))
        .collect();
    assert_eq!(failed.len(), 1, "{diagnostics:?}");
    assert_eq!(
        failed[0].message,
        "check XML1003 failed: InvalidOperationException: NQP0006: The syntax-pattern match exceeded its work limit."
    );
}

/// A pattern capture used as a spread takes only items the pattern matches.
#[test]
fn pattern_capture_spread_checks_each_item() {
    let checks = r#"    match (s: Secret) <[ <vault>..$s</vault> ]> => warning X6 : "secrets";"#;
    let language = language(checks, SECRET);
    assert!(found_codes(&language, "<vault><other/></vault>").is_empty());
    assert_eq!(
        found_codes(
            &language,
            "<vault><password>p</password><api-key>k</api-key></vault>"
        ),
        ["X6"]
    );
    assert!(found_codes(&language, "<vault><password>p</password><other/></vault>").is_empty());
}

/// In an arm, a pattern capture is accepted by the pattern's category, as in
/// a check, even when that category is an alias.
#[test]
fn arm_flavor_pattern_capture_over_alias() {
    let language = language("", SECRET);
    let language = language.language();
    let rule = |name: &str| language.rules.iter().position(|r| r.name == name).unwrap() as u16;
    let holes: &'static [Hole<'static>] = Box::leak(Box::new([Hole {
        name: "t",
        category: rule("Xml.Element"),
        pattern: Some(0),
        descendant: false,
    }]));
    let patterns = Patterns::compile(language).unwrap();
    let quote = Quote::compile(
        language,
        rule("Xml.Paired"),
        holes,
        "<vault>$t</vault>",
        Flavor::Arm,
    )
    .unwrap();
    let tree = parse(
        language,
        "<vault><password>p</password></vault>",
        Limits::default(),
    )
    .unwrap();
    let mut budget = Budget::new(MAX_WORK_STEPS);
    let vault = outer(language, &tree);
    assert!(
        quote
            .matches(&patterns, &tree, vault, &mut budget, &mut Vec::new())
            .unwrap()
    );
}
