fn main() -> Result<(), Box<dyn std::error::Error>> {
    nitride::build::compile_project("language/project.json", "calculator.rs")?;
    Ok(())
}
