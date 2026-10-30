fn main() {
    let manifest = std::path::Path::new("../../languages/csharp4/project.json");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let prepared = nitride_prepare::prepare_project(manifest).expect("prepare C# grammar");
    for input in &prepared.inputs {
        println!("cargo:rerun-if-changed={}", input.display());
    }
    let generated = nitride_codegen::generate(&prepared.data.to_string()).expect("generate parser");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("csharp4.rs"), generated).unwrap();
}
