use nitride_cli::LANGUAGE;
use nitride_core::{Limits, parse};

#[test]
fn precedence_and_associativity() {
    let tree = parse(&LANGUAGE, "10 - 3 - 2 * 4;", Limits::default()).unwrap();
    let sub = tree
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| LANGUAGE.rules[n.rule as usize].name == "Portable.Sub")
        .map(|(i, _)| i as u32)
        .next_back()
        .unwrap();
    assert_eq!(tree.text(tree.field(sub, "Left").unwrap()), Some("10 - 3"));
    assert_eq!(tree.text(tree.field(sub, "Right").unwrap()), Some("2 * 4"));
}

#[test]
fn backtracking_rolls_back_nodes() {
    let tree = parse(&LANGUAGE, "let x : Scalar = 2; x;", Limits::default()).unwrap();
    let identifiers = tree
        .nodes
        .iter()
        .filter(|n| LANGUAGE.rules[n.rule as usize].name == "Portable.Identifier")
        .count();
    assert_eq!(identifiers, 3);
}

#[test]
fn unicode_byte_spans() {
    let source = "// 😀\n1 + 2;";
    let tree = parse(&LANGUAGE, source, Limits::default()).unwrap();
    let add = tree
        .nodes
        .iter()
        .find(|n| LANGUAGE.rules[n.rule as usize].name == "Portable.Add")
        .unwrap();
    assert_eq!(add.span, [8, 13]);
}

#[test]
fn depth_limit_returns_diagnostic() {
    let input = format!("{}1{};", "(".repeat(200), ")".repeat(200));
    assert_eq!(
        parse(
            &LANGUAGE,
            &input,
            Limits {
                depth: 32,
                work: 100_000
            }
        )
        .unwrap_err()
        .category,
        "limit"
    );
}

#[test]
fn keyword_prefix_cannot_become_a_declaration() {
    assert!(parse(&LANGUAGE, "letx:Scalar=1; x;", Limits::default()).is_err());
}

#[test]
fn unterminated_comment_matches_nitrogen_trivia_to_eof_policy() {
    assert!(parse(&LANGUAGE, "1; /* bad", Limits::default()).is_ok());
}
