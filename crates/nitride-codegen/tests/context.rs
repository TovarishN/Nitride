//! Scoped parse context: `context` flags, `with(Flag = v)` scopes and
//! `context(Flag)` tests, after Nitrogen's GrammarContext fixtures.
use nitride_codegen::LoadedLanguage;
use nitride_core::*;

fn prepare(text: &str, start: &str) -> Result<serde_json::Value, String> {
    nitride_prepare::prepare_sources(
        &[nitride_prepare::Source {
            path: "test.ngr".into(),
            text: text.into(),
        }],
        "{\"types\":[],\"operations\":[],\"statements\":{}}",
        start,
        "syntax",
    )
    .map_err(|e| e.to_string())
}

fn load(text: &str, start: &str) -> &'static Language<'static> {
    let prepared = prepare(text, start).unwrap();
    // Leaked, so its tables live as long as a snapshot requires.
    let loaded: &'static LoadedLanguage = Box::leak(Box::new(
        LoadedLanguage::load(&prepared.to_string()).unwrap(),
    ));
    loaded.language()
}

const GRAMMAR: &str = r#"
syntax module GrammarContext
{
    context Enabled;
    syntax File = Items:Item*;
    syntax Item = Async / Ordinary;
    syntax Async = "a" Body:with(Enabled = true) Expr;
    syntax Ordinary = "n" Body:Expr;
    syntax Group = "[" Inner:with(Enabled = false) Expr "]";
    syntax Speculative = &(with(Enabled = true) Expr) Body:Expr;
    syntax ScopedLiteral = with(Enabled = true) ("z" / "y");
    syntax External = with(GrammarContext.Flags.External = true) ExternalBody;
    syntax ExternalBody = context(GrammarContext.Flags.External) "z";
    extensible syntax Expr
    {
        | Enabled = context(Enabled) "x"
        | Disabled = !context(Enabled) "x"
        | Nested = "(" Item ")"
        | Neg = (context(Enabled) "-" Arg:Expr) / (!context(Enabled) "-" Arg:Expr) precedence 20
        | Add = Expr "+" Right:Expr precedence 10 left
    }
}
syntax module GrammarContext.Flags { context External; }
"#;

/// Rule name, span and parent of every node, in node order.
fn shape(language: &Language<'_>, tree: &SyntaxTree<'_>) -> Vec<(String, [u32; 2])> {
    tree.nodes
        .iter()
        .map(|n| (language.rules[n.rule as usize].name.to_owned(), n.span))
        .collect()
}

fn names(language: &Language<'_>, tree: &SyntaxTree<'_>, name: &str) -> usize {
    tree.nodes
        .iter()
        .filter(|n| language.rules[n.rule as usize].name == format!("GrammarContext.{name}"))
        .count()
}

fn rule(language: &Language<'_>, name: &str) -> u16 {
    language.rules.iter().position(|r| r.name == name).unwrap() as u16
}

#[test]
fn scopes_select_alternatives_and_callers_pass_their_flags_on() {
    let language = load(GRAMMAR, "GrammarContext.File");
    let tree = parse(language, "a x n x a (n x) n (a x)", Limits::default()).unwrap();
    let selected: Vec<_> = tree
        .nodes
        .iter()
        .map(|n| (language.rules[n.rule as usize].name, n.span[0]))
        .filter(|(name, _)| name.ends_with(".Enabled") || name.ends_with(".Disabled"))
        .collect();
    // `n` inside `a (...)` inherits the enclosing true; only a scope changes it.
    assert_eq!(
        selected,
        [
            ("GrammarContext.Enabled", 2),
            ("GrammarContext.Disabled", 6),
            ("GrammarContext.Enabled", 13),
            ("GrammarContext.Enabled", 21),
        ]
    );
}

#[test]
fn instance_nodes_report_the_original_rule() {
    let language = load(GRAMMAR, "GrammarContext.File");
    let enabled = rule(language, "GrammarContext.Enabled");
    let tree = parse(language, "a x", Limits::default()).unwrap();
    assert!(tree.nodes.iter().any(|n| n.rule == enabled));
    // Every reported rule is an original; instances only parse.
    assert!(
        tree.nodes
            .iter()
            .all(|n| language.rules[n.rule as usize].origin.is_none())
    );
    assert!(language.rules.iter().any(|r| r.origin.is_some()));
}

#[test]
fn a_lookahead_under_true_never_answers_a_parse_under_false() {
    let language = load(GRAMMAR, "GrammarContext.Speculative");
    let tree = parse(language, "x", Limits::default()).unwrap();
    assert_eq!(names(language, &tree, "Disabled"), 1);
    assert_eq!(names(language, &tree, "Enabled"), 0);
}

#[test]
fn explicit_false_overrides_an_enclosing_true() {
    let language = load(
        &GRAMMAR.replace("Nested = \"(\" Item \")\"", "Nested = Group"),
        "GrammarContext.File",
    );
    let tree = parse(language, "a [x] a x", Limits::default()).unwrap();
    assert_eq!(names(language, &tree, "Disabled"), 1);
    assert_eq!(names(language, &tree, "Enabled"), 1);
}

#[test]
fn context_selected_prefix_operands_keep_their_precedence() {
    let language = load(GRAMMAR, "GrammarContext.File");
    for source in ["a -x+x", "n -x+x"] {
        let tree = parse(language, source, Limits::default()).unwrap();
        let add = tree
            .nodes
            .iter()
            .find(|n| language.rules[n.rule as usize].name == "GrammarContext.Add")
            .unwrap();
        let neg = tree
            .nodes
            .iter()
            .find(|n| language.rules[n.rule as usize].name == "GrammarContext.Neg")
            .unwrap();
        assert_eq!((add.span, neg.span), ([2, 6], [2, 4]), "{source}");
    }
}

#[test]
fn scopes_and_tests_build_no_nodes_and_flags_may_be_qualified() {
    let language = load(GRAMMAR, "GrammarContext.ScopedLiteral");
    let tree = parse(language, "y", Limits::default()).unwrap();
    assert_eq!(tree.nodes.len(), 1);
    let language = load(GRAMMAR, "GrammarContext.External");
    let tree = parse(language, "z", Limits::default()).unwrap();
    assert_eq!(
        shape(language, &tree),
        [
            ("GrammarContext.ExternalBody".into(), [0, 1]),
            ("GrammarContext.External".into(), [0, 1])
        ]
    );
    let language = load(GRAMMAR, "GrammarContext.ExternalBody");
    assert!(parse(language, "z", Limits::default()).is_err());
}

#[test]
fn snapshot_edits_match_a_full_parse_without_stale_context() {
    let language = load(GRAMMAR, "GrammarContext.File");
    let strict = SnapshotMode::Strict(ParseOptions::default());
    for (from, edit, to) in [
        ("a x n x", [0, 1], "n x n x"),
        ("n x n x", [4, 5], "n x a x"),
        ("n x a x", [0, 1], "a x a x"),
    ] {
        let base = parse_snapshot(language, from, Limits::default(), strict).unwrap();
        let edited = base
            .edit(
                &TextEdit {
                    range: edit,
                    replacement: to[edit[0] as usize..edit[1] as usize].into(),
                },
                Limits::default(),
            )
            .unwrap();
        let full = parse(language, to, Limits::default()).unwrap();
        assert_eq!(
            shape(language, &edited.materialize().unwrap()),
            shape(language, &full),
            "{to}"
        );
    }
}

#[test]
fn recovery_keeps_context_and_reports_original_rules() {
    let language = load(GRAMMAR, "GrammarContext.File");
    let strict = parse(language, "a x n x", Limits::default()).unwrap();
    let recovered = parse_recovering(
        language,
        "a x n x",
        Limits::default(),
        RecoveryOptions::default(),
    );
    assert!(recovered.diagnostics.is_empty());
    assert_eq!(shape(language, &recovered.tree), shape(language, &strict));
    let damaged = parse_recovering(
        language,
        "a x n ? n x",
        Limits::default(),
        RecoveryOptions::default(),
    );
    assert!(!damaged.diagnostics.is_empty());
    let nodes = &damaged.tree.nodes;
    assert!(
        nodes
            .iter()
            .all(|n| language.rules[n.rule as usize].origin.is_none())
    );
    // Whatever the repairs, each selection lies inside the header that set it.
    let within = |inner: &str, outer: &str| {
        let name = |n: &Node| language.rules[n.rule as usize].name;
        nodes.iter().filter(|n| name(n) == inner).all(|n| {
            nodes
                .iter()
                .any(|o| name(o) == outer && o.span[0] <= n.span[0] && n.span[1] <= o.span[1])
        })
    };
    assert!(within("GrammarContext.Enabled", "GrammarContext.Async"));
    assert!(within("GrammarContext.Disabled", "GrammarContext.Ordinary"));
    assert!(names(language, &damaged.tree, "Enabled") > 0);
    assert!(names(language, &damaged.tree, "Disabled") > 0);
}

#[test]
fn context_errors_are_preparation_errors() {
    let unknown = "syntax module T { syntax Start = context(Missing) \"x\"; }";
    assert!(
        prepare(unknown, "T.Start")
            .unwrap_err()
            .contains("unknown or ambiguous context flag Missing")
    );
    let token = "syntax module T { context F; token W = context(F) \"x\"; syntax Start = W; }";
    assert!(
        prepare(token, "T.Start")
            .unwrap_err()
            .contains("token cannot use parse context")
    );
    let duplicate = "syntax module T { context F; context F; syntax Start = \"x\"; }";
    assert!(
        prepare(duplicate, "T.Start")
            .unwrap_err()
            .contains("duplicate context flag")
    );
    let flags: String = (0..65).map(|i| format!("context F{i}; ")).collect();
    let many = format!("syntax module T {{ {flags} syntax Start = \"x\"; }}");
    assert!(prepare(&many, "T.Start").unwrap_err().contains("64"));
    let sixty_four: String = (0..64).map(|i| format!("context F{i}; ")).collect();
    let most = format!(
        "syntax module T {{ {sixty_four} syntax Start = with(F63 = true) S; syntax S = context(F63) \"x\"; }}"
    );
    let language = load(&most, "T.Start");
    assert!(parse(language, "x", Limits::default()).is_ok());
}

#[test]
fn grammars_without_context_generate_unchanged() {
    let text = "syntax module T { syntax Start = \"x\" Rest:R; syntax R = \"y\"; }";
    let prepared = prepare(text, "T.Start").unwrap();
    assert!(prepared.get("contexts").is_none());
    let generated = nitride_codegen::generate(&prepared.to_string()).unwrap();
    assert!(!generated.contains("origin:Some"));
}

#[test]
fn scoped_right_operands_keep_the_authored_precedence() {
    let scoped = GRAMMAR.replace(
        "Add = Expr \"+\" Right:Expr",
        "Add = Expr \"+\" Right:with(Enabled = true) Expr",
    );
    let language = load(&scoped, "GrammarContext.File");
    let tree = parse(language, "n x + x + x", Limits::default()).unwrap();
    assert_eq!(names(language, &tree, "Add"), 2);
    assert_eq!(names(language, &tree, "Enabled"), 2);
    assert_eq!(names(language, &tree, "Disabled"), 1);
    // Left associative: the outer Add's right operand is the last `x` alone.
    let spans: Vec<_> = tree
        .nodes
        .iter()
        .filter(|n| language.rules[n.rule as usize].name == "GrammarContext.Add")
        .map(|n| n.span)
        .collect();
    assert_eq!(spans, [[2, 7], [2, 11]]);
    let snapshot = parse_snapshot(
        language,
        "n x + x + x",
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    assert_eq!(
        shape(language, &snapshot.materialize().unwrap()),
        shape(language, &tree)
    );
}

#[test]
fn a_scoped_left_operand_is_rejected() {
    let scoped = GRAMMAR.replace(
        "Add = Expr \"+\" Right:Expr",
        "Add = with(Enabled = true) Expr \"+\" Right:Expr",
    );
    let error = prepare(&scoped, "GrammarContext.File").unwrap_err();
    assert!(error.contains("left operand"), "{error}");
}
