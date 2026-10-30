//! Regenerate the checked-in grammar, catalog and project parsers, or verify them with --check.
use std::path::Path;
fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let args: Vec<_> = std::env::args().skip(1).collect();
    let check = match args.as_slice() {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => {
            eprintln!("usage: bootstrap [--check]");
            std::process::exit(2);
        }
    };
    for (project, name, seed) in [
        ("grammar", "NitrideGrammar", "grammar_seed.rs"),
        ("catalog", "NitrideCatalog", "catalog_seed.rs"),
        ("binding", "NitrideBinding", "binding_seed.rs"),
        ("project", "NitrideProject", "project_seed.rs"),
    ] {
        let prepared = nitride_prepare::prepare_project(
            &root.join(format!("languages/{project}/{name}.nproj")),
        )
        .unwrap_or_else(|e| panic!("prepare {project} frontend: {e}"));
        let generated =
            nitride_codegen::generate(&prepared.data.to_string()).expect("generate frontend");
        let path = root.join("crates/nitride-prepare/src").join(seed);
        if check {
            assert_eq!(
                std::fs::read_to_string(path).unwrap_or_default(),
                generated,
                "stale {project} seed"
            );
            println!("{project} bootstrap fixed point verified");
        } else {
            std::fs::write(path, generated).unwrap();
            println!("{project} seed regenerated; rebuild and run with --check");
        }
    }
}
