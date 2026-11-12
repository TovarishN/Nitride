use nitride_codegen::LoadedLanguage;
use nitride_core::*;
use std::path::{Path, PathBuf};

const PROFILES: [&str; 13] = [
    "portable",
    "power",
    "composition",
    "xml",
    "namespaces",
    "inference",
    "calls",
    "functions",
    "dates",
    "rewrites",
    "csharp4",
    "context",
    "grammar",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load(profile: &str) -> LoadedLanguage {
    let manifest = root().join(format!("languages/{profile}/project.json"));
    let prepared = nitride_prepare::prepare_project(&manifest).unwrap();
    LoadedLanguage::load(&prepared.data.to_string()).unwrap()
}

fn samples(profile: &str) -> Vec<PathBuf> {
    let directory = root().join("languages").join(profile);
    let mut files: Vec<_> = std::fs::read_dir(&directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_stem().is_some_and(|s| s == "sample"))
        .collect();
    if profile == "csharp4" {
        let corpus = directory.join("corpus/Mono2004");
        let mut corpus: Vec<_> = std::fs::read_dir(corpus)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "cs"))
            .collect();
        corpus.sort();
        files.extend(corpus.into_iter().take(2));
    }
    assert!(!files.is_empty(), "no samples for {profile}");
    files
}

/// Per node: rule name, span and labelled children.
type Shape = Vec<(String, [u32; 2], Vec<(String, u32)>)>;

fn shape(language: &Language<'_>, tree: &SyntaxTree<'_>) -> Shape {
    (0..tree.nodes.len() as u32)
        .map(|id| {
            let node = tree.nodes[id as usize];
            let edges = tree
                .children(id)
                .iter()
                .map(|e| (tree.label(e).to_owned(), e.node))
                .collect();
            (
                language.rules[node.rule as usize].name.to_owned(),
                node.span,
                edges,
            )
        })
        .collect()
}

/// A runtime-built language is the generated static table, value for value,
/// and parses every sample to the same tree.
#[test]
fn runtime_matches_generated() {
    for profile in PROFILES {
        let loaded = load(profile);
        let runtime = loaded.language();
        let generated = nitride_cli::static_language(profile).unwrap();
        assert_eq!(
            format!("{runtime:?}"),
            format!("{generated:?}"),
            "{profile} tables"
        );
        for sample in samples(profile) {
            let text = std::fs::read_to_string(&sample).unwrap();
            let a = parse_recovering(
                runtime,
                &text,
                Limits::default(),
                RecoveryOptions::default(),
            );
            let b = parse_recovering(
                generated,
                &text,
                Limits::default(),
                RecoveryOptions::default(),
            );
            assert_eq!(a.tree.root, b.tree.root, "{}", sample.display());
            assert_eq!(
                shape(runtime, &a.tree),
                shape(generated, &b.tree),
                "{}",
                sample.display()
            );
        }
    }
}

/// The emitted Rust for the grammar frontend is still the checked-in seed.
#[test]
fn generate_unchanged() {
    let manifest = root().join("languages/grammar/project.json");
    let prepared = nitride_prepare::prepare_project(&manifest).unwrap();
    let generated = nitride_codegen::generate(&prepared.data.to_string()).unwrap();
    let seed =
        std::fs::read_to_string(root().join("crates/nitride-prepare/src/grammar_seed.rs")).unwrap();
    assert_eq!(generated, seed);
}

/// Loading and dropping repeatedly frees each language (run under Miri for proof).
#[test]
fn reload_and_drop() {
    for _ in 0..3 {
        let loaded = load("portable");
        let tree = parse(loaded.language(), "1 + 2;", Limits::default()).unwrap();
        assert!(!tree.nodes.is_empty());
    }
}

fn load_grammar(text: &str) -> Result<LoadedLanguage, String> {
    let prepared = nitride_prepare::prepare_sources(
        &[nitride_prepare::Source {
            path: "test.ngr".into(),
            text: text.into(),
        }],
        "{\"types\":[],\"operations\":[],\"statements\":{}}",
        "Test.Start",
        "syntax",
    )
    .map_err(|e| e.to_string())?;
    LoadedLanguage::load(&prepared.to_string())
}

#[test]
fn rule_checks_load_and_report() {
    let grammar = "syntax module Test { token N = ['a'..'z']+; \
        syntax Start = \"(\" Open:N Items:Item* \")\" Close:N \
        { check T1 Open.Text == Close.Text : $\"{Close.Text} closes {Open.Text}\" at Close; \
          match (x:Item)<[ (a ..$_ $x ..$_) a ]> => warning T2 : $\"saw {x}\"; } \
        syntax Item = N; }";
    let loaded = load_grammar(grammar).unwrap();
    let language = loaded.language();
    assert_eq!(language.rule_checks.len(), 2);
    let diagnostics = |text: &str| {
        let parsed = parse_recovering(language, text, Limits::default(), Default::default());
        compile_recovering(language, &parsed, Limits::default())
            .diagnostics
            .into_iter()
            .map(|d| {
                (
                    d.code.unwrap_or_default().to_string(),
                    d.severity,
                    d.message,
                    d.span,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        diagnostics("(a b) b"),
        [("T1".into(), Severity::Error, "b closes a".into(), [6, 7])]
    );
    // One report per node, with the first binding that matches.
    assert_eq!(
        diagnostics("(a q r s) a"),
        [("T2".into(), Severity::Warning, "saw q".into(), [0, 11])]
    );
    assert!(diagnostics("(b q) b").is_empty());
}

#[test]
fn unparseable_rule_check_patterns_fail_to_load() {
    let grammar = "syntax module Test { token N = ['a'..'z']+; \
        syntax Start = Open:N { match <[ 1 2 3 ]> => error T1 : $\"x\"; } }";
    let error = load_grammar(grammar).err().unwrap();
    assert!(error.contains("pattern of check T1"), "{error}");
}

/// Named patterns, pattern-referencing captures and match contexts survive
/// prepared JSON → generated table → loaded language.
#[test]
fn named_patterns_round_trip() {
    let manifest = root().join("languages/xml/project.json");
    let mut data = nitride_prepare::prepare_project(&manifest).unwrap().data;
    data["patterns"] = serde_json::json!([{
        "name": "Xml.Secret",
        "category": "Xml.Element",
        "alternatives": [
            {"captures": [], "pattern": " <password ..$_>..$_</password> "},
            {"captures": [], "pattern": " <api-key ..$_>..$_</api-key> "}
        ]
    }]);
    let paired = data["rules"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["name"] == "Xml.Paired")
        .unwrap();
    let check = paired["checks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["code"] == "XML1001")
        .unwrap();
    check["captures"][0] = serde_json::json!(
        {"name": "pass", "category": "Xml.Element", "pattern": "Xml.Secret", "descendant": true}
    );
    check["context"] = serde_json::json!({
        "kind": "inside",
        "category": "Xml.Paired",
        "captures": [],
        "pattern": " <service ..$_>..$_</service> "
    });
    let loaded = LoadedLanguage::load(&data.to_string()).unwrap();
    let language = loaded.language();
    assert_eq!(language.patterns.len(), 1);
    let secret = &language.patterns[0];
    assert_eq!(secret.name, "Xml.Secret");
    let patterns: Vec<_> = secret.alternatives.iter().map(|a| a.pattern).collect();
    assert_eq!(
        patterns,
        [
            " <password ..$_>..$_</password> ",
            " <api-key ..$_>..$_</api-key> "
        ]
    );
    let check = language
        .rule_checks
        .iter()
        .find(|c| c.code == "XML1001")
        .unwrap();
    let RuleTest::Match {
        captures, context, ..
    } = &check.test
    else {
        panic!("XML1001 is a match check");
    };
    assert_eq!(captures[0].pattern, Some(0));
    assert!(captures[0].descendant);
    assert_eq!(captures[0].category, secret.category);
    let context = context.as_ref().expect("an inside context");
    assert!(!context.parent_only);
    assert_eq!(context.pattern, " <service ..$_>..$_</service> ");
}

/// Every Pratt point is memoized, including points whose candidates begin
/// differently: alternatives that share a prefix of such points (C#'s
/// modifiers and type references) re-parse it from the memo.
#[test]
fn every_pratt_point_is_memoized() {
    let manifest = root().join("languages/csharp4/project.json");
    let prepared = nitride_prepare::prepare_project(&manifest).unwrap();
    let generated = nitride_codegen::generate(&prepared.data.to_string()).unwrap();
    assert!(generated.contains("memoize:true"));
    assert!(!generated.contains("memoize:false"));
}
