fn main() {
    for (profile, output) in [
        ("portable", "language"),
        ("power", "power"),
        ("composition", "composition"),
        ("xml", "xml"),
        ("namespaces", "namespaces"),
        ("inference", "inference"),
        ("calls", "calls"),
        ("functions", "functions"),
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
        std::fs::write(
            std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
                .join(format!("{output}.rs")),
            generated,
        )
        .unwrap();
    }
}
