//! `.nproj` language and workspace files, resolved with spans.
use nitride_prepare::{PrepareError, Source, read_language, read_workspace};
use std::path::{Path, PathBuf};

fn directory() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "nitride-nproj-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn touch(dir: &Path, files: &[&str]) {
    for file in files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }
}

fn source(path: PathBuf, text: &str) -> Source {
    Source {
        path: path.display().to_string(),
        text: text.into(),
    }
}

fn exists(path: &Path) -> bool {
    path.exists()
}

fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// The single error of `text` as a language file in `dir`.
fn language_error(dir: &Path, text: &str) -> (String, PrepareError) {
    let errors = read_language(&source(dir.join("X.nproj"), text), &exists).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    let error = errors.into_iter().next().unwrap();
    (text[error.span[0]..error.span[1]].to_owned(), error)
}

const DATES: &str = r#"// Dates: calendar arithmetic over day numbers.
language Dates
{
  extensions .dates;
  grammar Dates.ngr;
  start Dates.Program;
  catalog Dates.ncatalog;

  nitrogen
  {
    namespace Nitride.Reference.Dates;
    grammar Dates.Reference.ngr;   // replaces the grammars above for Nitrogen
    source DatesSemantics.cs;
  }
}
"#;

#[test]
fn dates_example_resolves() {
    let dir = directory();
    touch(
        &dir,
        &["Dates.ngr", "Dates.ncatalog", "Dates.Reference.ngr", "DatesSemantics.cs"],
    );
    let language = read_language(&source(dir.join("Dates.nproj"), DATES), &exists).unwrap();
    assert_eq!(language.path, dir.join("Dates.nproj"));
    assert_eq!(language.name, "Dates");
    assert_eq!(language.extensions, [".dates"]);
    assert_eq!(language.grammars, [dir.join("Dates.ngr")]);
    assert_eq!(language.start, "Dates.Program");
    assert_eq!(&DATES[language.start_span[0]..language.start_span[1]], "Dates.Program");
    assert_eq!(language.catalogs, [dir.join("Dates.ncatalog")]);
    assert!(language.typed());
    let nitrogen = language.nitrogen.unwrap();
    assert_eq!(nitrogen.namespace, "Nitride.Reference.Dates");
    assert_eq!(nitrogen.grammars, [dir.join("Dates.Reference.ngr")]);
    assert_eq!(nitrogen.sources, [dir.join("DatesSemantics.cs")]);
}

#[test]
fn nitrogen_without_grammar_inherits_outer_list() {
    let dir = directory();
    touch(&dir, &["A.ngr"]);
    let text = "language A { grammar A.ngr; start A.S; nitrogen { namespace N.A; } }";
    let language = read_language(&source(dir.join("A.nproj"), text), &exists).unwrap();
    let nitrogen = language.nitrogen.unwrap();
    assert_eq!(nitrogen.grammars, [dir.join("A.ngr")]);
    assert!(nitrogen.sources.is_empty());
}

#[test]
fn repeated_lists_append_in_order() {
    let dir = directory();
    touch(&dir, &["A.ngr", "B.ngr", "C.ngr", "sub/D.ngr"]);
    let text = "language A { extensions .a; extensions .b, .c; grammar A.ngr; grammar B.ngr, C.ngr, sub/D.ngr; start A.S; }";
    let language = read_language(&source(dir.join("A.nproj"), text), &exists).unwrap();
    assert_eq!(language.extensions, [".a", ".b", ".c"]);
    assert_eq!(
        language.grammars,
        [dir.join("A.ngr"), dir.join("B.ngr"), dir.join("C.ngr"), dir.join("sub/D.ngr")]
    );
}

#[test]
fn no_catalog_means_syntax() {
    let dir = directory();
    touch(&dir, &["A.ngr"]);
    let text = "language A { grammar A.ngr; start A.S; }";
    let language = read_language(&source(dir.join("A.nproj"), text), &exists).unwrap();
    assert!(!language.typed());
    assert!(language.extensions.is_empty());
    assert!(language.nitrogen.is_none());
}

#[test]
fn errors_carry_nproj_spans() {
    let dir = directory();
    touch(&dir, &["A.ngr"]);
    let (at, error) = language_error(&dir, "language A { grammar A.ngr, Missing.ngr; start A.S; }");
    assert_eq!(at, "Missing.ngr");
    assert!(error.message.contains("file not found"), "{error}");
    assert!(error.path.ends_with("X.nproj"));

    let (at, error) = language_error(&dir, "language A { grammar A.ngr; start A.S; start A.T; }");
    assert_eq!(at, "start A.T;");
    assert!(error.message.contains("repeated start"), "{error}");

    let (at, error) = language_error(&dir, "language A { grammar A.ngr; start A.S; nitrogen { } }");
    assert_eq!(at, "nitrogen");
    assert!(error.message.contains("nitrogen block needs a namespace"), "{error}");

    let (at, error) = language_error(&dir, "language A { grammar A.ngr; }");
    assert_eq!(at, "A");
    assert!(error.message.contains("language needs a start rule"), "{error}");

    let (_, error) = language_error(&dir, "workspace { include a; }");
    assert!(error.message.contains("expected a language"), "{error}");

    let (_, error) = language_error(&dir, "language A { grammar A.ngr; start A.S; mode typed; }");
    assert!(error.path.ends_with("X.nproj"));
}

#[test]
fn workspace_include_errors() {
    let dir = directory();
    touch(&dir, &["good/A.ngr", "two/One.nproj", "two/Two.nproj"]);
    std::fs::write(
        dir.join("good/A.nproj"),
        "language A { extensions .a; grammar A.ngr; start A.S; }",
    )
    .unwrap();
    let text = "workspace { include missing; include two; include good; }";
    let workspace = read_workspace(&source(dir.join("nitride.nproj"), text), &read, &exists);
    assert_eq!(workspace.languages.len(), 1);
    assert_eq!(workspace.languages[0].name, "A");
    let errors: Vec<_> = workspace
        .errors
        .iter()
        .map(|e| (&text[e.span[0]..e.span[1]], e.message.as_str()))
        .collect();
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert_eq!(errors[0].0, "missing");
    assert!(errors[0].1.contains("no .nproj in"), "{errors:?}");
    assert_eq!(errors[1].0, "two");
    assert!(errors[1].1.contains("several .nproj in"), "{errors:?}");
}

#[test]
fn duplicate_extension_across_languages() {
    let dir = directory();
    touch(&dir, &["a/A.ngr", "b/B.ngr"]);
    std::fs::write(dir.join("a/A.nproj"), "language A { extensions .x; grammar A.ngr; start A.S; }").unwrap();
    let b = "language B { extensions .x, .y; grammar B.ngr; start B.S; }";
    std::fs::write(dir.join("b/B.nproj"), b).unwrap();
    let text = "workspace { include a; include b; }";
    let workspace = read_workspace(&source(dir.join("nitride.nproj"), text), &read, &exists);
    assert_eq!(workspace.languages.len(), 2);
    assert_eq!(workspace.languages[0].extensions, [".x"]);
    assert_eq!(workspace.languages[1].extensions, [".y"]);
    assert_eq!(workspace.errors.len(), 1, "{:?}", workspace.errors);
    let error = &workspace.errors[0];
    assert!(error.path.ends_with("B.nproj"));
    assert_eq!(&b[error.span[0]..error.span[1]], ".x");
    assert!(error.message.contains("extension .x is already claimed by A"), "{error}");
}

#[test]
fn included_language_errors_are_reported_against_their_file() {
    let dir = directory();
    std::fs::create_dir_all(dir.join("a")).unwrap();
    std::fs::write(dir.join("a/A.nproj"), "language A { grammar Gone.ngr; start A.S; }").unwrap();
    let workspace = read_workspace(
        &source(dir.join("nitride.nproj"), "workspace { include a; }"),
        &read,
        &exists,
    );
    assert!(workspace.languages.is_empty());
    assert_eq!(workspace.errors.len(), 1);
    assert!(workspace.errors[0].path.ends_with("A.nproj"));
}
