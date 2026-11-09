fn main() {
    stdlib_fixtures();
    for (profile, output) in [
        ("portable", "language"),
        ("power", "power"),
        ("composition", "composition"),
        ("xml", "xml"),
        ("namespaces", "namespaces"),
        ("inference", "inference"),
        ("calls", "calls"),
        ("functions", "functions"),
        ("dates", "dates"),
        ("calendar", "calendar"),
        ("rewrites", "rewrites"),
        ("csharp4", "csharp4"),
        ("context", "context"),
        ("grammar", "grammar"),
        ("json", "json"),
    ] {
        let manifest = format!("../../languages/{profile}/project.json");
        println!("cargo:rerun-if-changed={manifest}");
        let prepared = nitride_prepare::prepare_project(std::path::Path::new(&manifest))
            .unwrap_or_else(|e| panic!("{e}"));
        for input in &prepared.inputs {
            println!("cargo:rerun-if-changed={}", input.display());
        }
        let generated = nitride_codegen::generate(&prepared.data.to_string())
            .expect("validated native language");
        let interfaces = nitride_codegen::interfaces(&prepared.data.to_string())
            .expect("validated catalog interfaces");
        let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        std::fs::write(out.join(format!("{output}.rs")), generated).unwrap();
        std::fs::write(out.join(format!("{output}_interfaces.rs")), interfaces).unwrap();
    }
}

// Compile the same lexical contracts used by the runtime tests. These files
// are included only by the integration test, not by the distributed binary.
fn stdlib_fixtures() {
    let library = nitride_prepare::stdlib::find("Nitride.Lexical", "1.0.0").unwrap();
    let mut cases: Vec<_> = library
        .exports
        .iter()
        .map(|&name| (name, name, "", false))
        .collect();
    cases.extend([
        ("Keyword", "AsciiIdentifier", "except \"let\"", false),
        ("JsonTrivia", "UnsignedInteger", "", true),
    ]);
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    for (case, token, except, json) in cases {
        let mut sources: Vec<_> = library
            .sources
            .iter()
            .map(|source| nitride_prepare::Source {
                path: source.uri.into(),
                text: source.text.into(),
            })
            .collect();
        sources.push(nitride_prepare::Source { path: "Test.ngr".into(), text: format!("syntax module Test {{ token Name = Nitride.Lexical.{token} {except}; syntax Start = Name; }}") });
        let mut data = nitride_prepare::prepare_sources(
            &sources,
            "{\"types\":[],\"operations\":[],\"statements\":{}}",
            "Test.Start",
            "syntax",
        )
        .unwrap();
        if json {
            nitride_prepare::apply_trivia(&mut data, nitride_core::TriviaPolicy::Json);
        }
        let generated = nitride_codegen::generate(&data.to_string()).unwrap();
        std::fs::write(out.join(format!("stdlib_{case}.rs")), generated).unwrap();
    }
}
