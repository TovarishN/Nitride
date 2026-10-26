use nitride_codegen::LoadedLanguage;
use nitride_core::*;
use std::path::{Path, PathBuf};

const PROFILES: [&str; 12] = [
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
