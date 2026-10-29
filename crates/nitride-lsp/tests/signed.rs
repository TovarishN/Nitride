use nitride_lsp::signed::{Mode, read};

#[test]
fn language_path_and_name() {
    let header = read("#!nitrogen language \"./Calc.syntax\"\nlet a=1;");
    assert_eq!(header.signatures.len(), 1, "{:?}", header.diagnostics);
    let reference = header.signatures[0].reference.as_ref().unwrap();
    assert_eq!(
        (reference.value.as_str(), reference.is_path),
        ("./Calc.syntax", true)
    );
    assert_eq!(header.signatures[0].mode, Mode::Language);

    let header = read("#!nitrogen language Calc\nlet a=1;");
    let reference = header.signatures[0].reference.as_ref().unwrap();
    assert_eq!(
        (reference.value.as_str(), reference.is_path),
        ("Calc", false)
    );
}

#[test]
fn grammar_signature() {
    let header = read("#!nitrogen grammar\nlanguage Calc { start Calc.Program; }\n");
    assert_eq!(header.signatures.len(), 1);
    assert_eq!(header.signatures[0].mode, Mode::Grammar);
    assert!(header.signatures[0].reference.is_none());
}

#[test]
fn header_variants_keep_offsets() {
    let variants = [
        "\u{feff}#!nitrogen language Calc\nlet a=1;",
        "\n\n  #!nitrogen language Calc\nlet a=1;",
        "// #!nitrogen language Calc\nlet a=1;",
        "/* prose here\n#!nitrogen language Calc\n*/\nlet a=1;",
        "<!-- #!nitrogen language Calc -->\nlet a=1;",
        "#!nitrogen language Calc\r\nlet a=1;",
    ];
    for text in variants {
        let header = read(text);
        assert_eq!(
            header.signatures.len(),
            1,
            "{text:?}: {:?}",
            header.diagnostics
        );
        assert_eq!(header.masked.len(), text.len(), "{text:?}");
        let body = text.find("let").unwrap();
        assert_eq!(&header.masked[body..], &text[body..], "{text:?}");
        for (masked, original) in header.masked[..body].chars().zip(text[..body].chars()) {
            assert!(
                masked == original
                    && (original == '\n' || original == '\r' || original.is_whitespace())
                    || masked == ' ',
                "{text:?}"
            );
        }
        assert!(!header.masked.contains("#!nitrogen"), "{text:?}");
    }
}

#[test]
fn errors_are_ngr0401() {
    let header = read("#!nitrogen frob\nlet a=1;");
    assert_eq!(header.diagnostics.len(), 1);
    assert_eq!(
        header.diagnostics[0].1,
        "expected grammar, language, or augment after #!nitrogen"
    );
    let header = read("#!nitrogen language \"unterminated\nlet a=1;");
    assert_eq!(
        header.diagnostics[0].1,
        "expected a non-empty quoted language path"
    );
    let header = read("#!nitrogen language Calc extra\nlet a=1;");
    assert_eq!(
        header.diagnostics[0].1,
        "unexpected text after Nitrogen signature"
    );
    let header = read("/* #!nitrogen language Calc\nlet a=1;");
    assert!(
        header
            .diagnostics
            .iter()
            .any(|d| d.1 == "unterminated signature comment")
    );
}

#[test]
fn grammar_cannot_combine() {
    let header = read("#!nitrogen grammar\n#!nitrogen language X\n");
    assert_eq!(header.signatures.len(), 1);
    assert_eq!(
        header.diagnostics[0].1,
        "incompatible or duplicate Nitrogen document signature"
    );
}

#[test]
fn augment_is_ignored() {
    let header = read("// #!nitrogen augment \"./Checks.syntax\" document\nclass X {}");
    assert!(header.signatures.is_empty());
    assert!(header.diagnostics.is_empty());
}

#[test]
fn only_leading_trivia() {
    let header = read("let a=1;\n#!nitrogen language X\n");
    assert!(header.signatures.is_empty());
    assert_eq!(header.masked, "let a=1;\n#!nitrogen language X\n");
}
