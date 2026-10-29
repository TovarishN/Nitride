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
        ("dates", "dates"),
        ("calendar", "calendar"),
        ("rewrites", "rewrites"),
        ("csharp4", "csharp4"),
        ("grammar", "grammar"),
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
