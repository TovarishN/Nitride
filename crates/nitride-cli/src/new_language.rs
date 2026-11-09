use nitride_prepare::authoring::{TemplateKind, TemplateOptions, create_language, plan_language};
use serde_json::json;
use std::path::PathBuf;
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let [command, kind, name, tail @ ..] = args else {
        return Err("usage: nitride new language NAME --directory PATH --extension .suffix [--kind syntax|typed] [--dry-run] [--json]".into());
    };
    if command != "new" || kind != "language" {
        return Err("expected new language".into());
    }
    let (mut directory, mut extension, mut kind, mut dry, mut json_output) =
        (None, None, TemplateKind::Typed, false, false);
    let mut seen = std::collections::HashSet::new();
    let mut tail = tail.iter();
    while let Some(flag) = tail.next() {
        if !seen.insert(flag) {
            return Err(format!("repeated option {flag}").into());
        }
        match flag.as_str() {
            "--directory" => {
                directory = Some(PathBuf::from(
                    tail.next().ok_or("--directory requires a path")?,
                ))
            }
            "--extension" => {
                extension = Some(tail.next().ok_or("--extension requires a suffix")?.clone())
            }
            "--kind" => {
                kind = match tail.next().map(String::as_str) {
                    Some("typed") => TemplateKind::Typed,
                    Some("syntax") => TemplateKind::Syntax,
                    _ => return Err("--kind must be syntax or typed".into()),
                }
            }
            "--dry-run" => dry = true,
            "--json" => json_output = true,
            _ => return Err(format!("unknown option {flag}").into()),
        }
    }
    let directory = directory.ok_or("--directory is required")?;
    let plan = plan_language(&TemplateOptions {
        name: name.clone(),
        extension: extension.ok_or("--extension is required")?,
        kind,
    })?;
    if dry {
        if json_output {
            println!(
                "{}",
                json!({"directory":directory,"files":plan.files.iter().map(|f|json!({"path":f.relative_path,"contents":f.contents,"kind":f.kind})).collect::<Vec<_>>()})
            );
        } else {
            for file in &plan.files {
                println!("create {}", directory.join(&file.relative_path).display());
            }
        }
    } else {
        let report = create_language(&directory, &plan)?;
        if json_output {
            println!("{}", json!({"created":report.created}));
        } else {
            println!("Created {}", directory.display());
        }
    }
    Ok(())
}
