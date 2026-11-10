fn main() {
    let manifest = std::path::Path::new("../../languages/testing/NitrideTests.nproj");
    let project = nitride_prepare::prepare_project(manifest).expect("test language frontend");
    for input in project.inputs {
        println!("cargo:rerun-if-changed={}", input.display());
    }
    let generated =
        nitride_codegen::generate(&project.data.to_string()).expect("generate test language");
    std::fs::write(
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("testing.rs"),
        generated,
    )
    .unwrap();
}
