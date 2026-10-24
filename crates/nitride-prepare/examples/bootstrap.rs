//! Regenerate the checked-in grammar parser, or verify it with --check.
use std::path::Path;
fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let prepared = nitride_prepare::prepare_project(&root.join("languages/grammar/project.json"))
        .expect("prepare grammar frontend");
    let generated =
        nitride_codegen::generate(&prepared.data.to_string()).expect("generate frontend");
    let path = root.join("crates/nitride-prepare/src/grammar_seed.rs");
    let args: Vec<_> = std::env::args().skip(1).collect();
    let check = match args.as_slice() {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => {
            eprintln!("usage: bootstrap [--check]");
            std::process::exit(2);
        }
    };
    if check {
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            generated,
            "stale grammar seed"
        );
        println!("grammar bootstrap fixed point verified");
    } else {
        std::fs::write(path, generated).unwrap();
        println!("grammar seed regenerated; rebuild and run with --check");
    }
}
