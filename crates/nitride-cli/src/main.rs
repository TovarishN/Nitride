use nitride_core::*;

fn main() -> std::process::ExitCode {
    match run() {
        Ok(ok) => {
            if ok {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool, Box<dyn std::error::Error>> {
    // `--grant <effect>`, wherever it appears, grants a catalog effect (such
    // as `Calendar.Clock`) to run, conform and --calendar; none is granted
    // otherwise.
    let mut args = Vec::new();
    let mut granted: Vec<String> = Vec::new();
    let mut given = std::env::args().skip(1);
    while let Some(arg) = given.next() {
        if arg == "--grant" {
            granted.push(given.next().ok_or("--grant requires an effect")?);
        } else {
            args.push(arg);
        }
    }
    if args.as_slice() == ["lsp"] {
        nitride_lsp::serve(
            std::io::BufReader::new(std::io::stdin()),
            std::io::stdout().lock(),
            nitride_lsp::ServeOptions::default(),
        )?;
        return Ok(true);
    }
    if args.as_slice() == ["--recover-batch"] {
        use std::io::{BufRead, Write};
        let mut out = std::io::BufWriter::new(std::io::stdout().lock());
        for line in std::io::stdin().lock().lines() {
            let request: serde_json::Value = serde_json::from_str(&line?)?;
            let profile = request["profile"]
                .as_str()
                .ok_or("missing recovery profile")?;
            let language =
                nitride_cli::recovery_language(profile).ok_or("unknown recovery profile")?;
            let source = request["source"]
                .as_str()
                .ok_or("missing recovery source")?;
            writeln!(out, "{}", nitride_cli::recovery_observe(language, source))?;
            out.flush()?;
        }
        return Ok(true);
    }
    if let [flag, profile, path] = args.as_slice() {
        if flag == "--recover" {
            let language =
                nitride_cli::recovery_language(profile).ok_or("unknown recovery profile")?;
            let result = nitride_cli::recovery_observe(language, &std::fs::read_to_string(path)?);
            println!("{result}");
            return Ok(result["ok"] == true);
        }
    }
    if args.first().is_some_and(|a| a == "prepare") {
        let (manifest, output) = match args.as_slice() {
            [_, manifest] => (manifest, None),
            [_, manifest, flag, output] if flag == "--output" => (manifest, Some(output)),
            _ => {
                return Err(
                    "usage: nitride prepare <project.nproj|project.json> [--output <language.json>]".into(),
                );
            }
        };
        let prepared = nitride_prepare::prepare_project(std::path::Path::new(manifest))?;
        if let Some(output) = output {
            nitride_prepare::write_prepared(&prepared, std::path::Path::new(output))?;
        } else {
            println!("{}", serde_json::to_string_pretty(&prepared.data)?);
        }
        return Ok(true);
    }
    if args.first().is_some_and(|a| a == "catalog") {
        // The lowered catalog JSON, for tools that read catalogs as JSON; no
        // files give the empty catalog of a syntax-only profile.
        let sources = args[1..]
            .iter()
            .map(|path| {
                Ok(nitride_prepare::Source {
                    text: std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?,
                    path: path.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        println!("{}", nitride_prepare::catalog(&sources)?.json);
        return Ok(true);
    }
    if args.first().is_some_and(|a| a == "interface") {
        // The project's catalog interfaces for implementers: Rust traits,
        // binders and conformance runs, or WIT worlds for components.
        let (target, manifest) = match args.as_slice() {
            [_, manifest] => ("rust", manifest),
            [_, flag, target, manifest] if flag == "--target" => (target.as_str(), manifest),
            _ => {
                return Err(
                    "usage: nitride interface [--target rust|wit|wit-binding] <project.nproj|project.json>"
                        .into(),
                );
            }
        };
        let prepared = nitride_prepare::prepare_project(std::path::Path::new(manifest))?;
        let data = prepared.data.to_string();
        match target {
            "rust" => print!("{}", nitride_codegen::interfaces(&data)?),
            "wit" => {
                let documents = nitride_codegen::wit(&data)?;
                let texts: Vec<_> = documents.into_iter().map(|(_, wit)| wit).collect();
                print!("{}", texts.join("\n"));
            }
            // What each binding's component must export, with its renames.
            "wit-binding" => {
                let documents = nitride_codegen::binding_wit(&data)?;
                let texts: Vec<_> = documents.into_iter().map(|(_, wit)| wit).collect();
                print!("{}", texts.join("\n"));
            }
            other => return Err(format!("unknown interface target '{other}'").into()),
        }
        return Ok(true);
    }
    if let [command, verb, manifest, interface, component] = args.as_slice()
        && command == "binding"
        && verb == "draft"
    {
        // A `.nbind` drafted from what a component exports.
        let prepared = nitride_prepare::prepare_project(std::path::Path::new(manifest))?;
        print!(
            "{}",
            nitride_wasm::draft(
                &prepared.data,
                interface,
                std::path::Path::new(component),
                component,
            )?
        );
        return Ok(true);
    }
    if let [command, manifest, source] = args.as_slice()
        && command == "run"
    {
        // Any project: the engine's handlers and its bindings' components.
        let result = nitride_cli::run_project(
            std::path::Path::new(manifest),
            &std::fs::read_to_string(source)?,
            &granted,
        )?;
        println!("{result}");
        return Ok(result["ok"] == true);
    }
    if let [command, manifest] = args.as_slice()
        && command == "conform"
        && manifest != "calendar"
    {
        // Each bound interface's examples, run against its component.
        let reports = nitride_cli::conform_project(std::path::Path::new(manifest), &granted)?;
        println!("{reports}");
        return Ok(reports
            .as_array()
            .into_iter()
            .flatten()
            .all(|r| r["failed"].as_array().is_some_and(Vec::is_empty)));
    }
    if args.first().is_some_and(|a| a == "conform") {
        // The interface examples, run through the implementations this
        // executable links.
        if args.len() != 2 || args[1] != "calendar" {
            return Err("usage: nitride conform calendar|<project.nproj|project.json>".into());
        }
        use nitride_cli::calendar::{FixtureCalendar, LANGUAGE, clock, gregorian};
        let report = |contract: &str, examples: usize, outcome: nitride_core::host::Conformance| {
            serde_json::json!({
                "interface": contract,
                "examples": examples,
                "skipped": outcome.skipped,
                "failed": outcome
                    .failed
                    .iter()
                    .map(|(example, why)| serde_json::json!({"example": example, "why": why}))
                    .collect::<Vec<_>>(),
            })
        };
        let reports = [
            report(
                gregorian::CONTRACT,
                gregorian::EXAMPLES.len(),
                gregorian::conformance(&LANGUAGE, std::sync::Arc::new(FixtureCalendar), &granted),
            ),
            report(
                clock::CONTRACT,
                clock::EXAMPLES.len(),
                clock::conformance(&LANGUAGE, std::sync::Arc::new(FixtureCalendar), &granted),
            ),
        ];
        println!("{}", serde_json::json!(reports));
        return Ok(reports
            .iter()
            .all(|r| r["failed"].as_array().is_some_and(Vec::is_empty)));
    }
    if let [flag, path] = args.as_slice()
        && flag == "--calendar"
    {
        let result = nitride_cli::observe_calendar(
            &std::fs::read_to_string(path)?,
            std::sync::Arc::new(nitride_cli::calendar::FixtureCalendar),
            &granted,
        );
        println!("{result}");
        return Ok(result["ok"] == true);
    }
    if args.len() == 1 && args[0] == "--version" {
        println!("Nitride {}", env!("CARGO_PKG_VERSION"));
        return Ok(true);
    }
    let dates = match args.as_slice() {
        [flag, path] if flag == "--dates" || flag == "--rewrites" => {
            Some((flag, path, nitride_cli::utc_today()))
        }
        [flag, path, today_flag, today]
            if (flag == "--dates" || flag == "--rewrites") && today_flag == "--today" =>
        {
            Some((
                flag,
                path,
                iso_date_day_number(today).ok_or("--today requires a yyyy-MM-dd calendar date")?
                    as f64,
            ))
        }
        _ => None,
    };
    if let Some((flag, path, today)) = dates {
        let source = std::fs::read_to_string(path)?;
        let result = if flag == "--rewrites" {
            nitride_cli::observe_rewrites(&source, today)
        } else {
            nitride_cli::observe_dates(&source, today)
        };
        println!("{result}");
        return Ok(result["ok"] == true);
    }
    if let [flag, path] = args.as_slice() {
        let result = match flag.as_str() {
            "--functions" => Some(nitride_cli::observe_functions(&std::fs::read_to_string(
                path,
            )?)),
            "--calls" => Some(nitride_cli::observe_calls(&std::fs::read_to_string(path)?)),
            "--inference" => Some(nitride_cli::observe_inference(&std::fs::read_to_string(
                path,
            )?)),
            "--namespaces" => Some(nitride_cli::observe_namespaces(&std::fs::read_to_string(
                path,
            )?)),
            "--power" => Some(nitride_cli::observe_power(&std::fs::read_to_string(path)?)),
            "--parse-xml" => Some(nitride_cli::syntax_observe(
                &nitride_cli::xml::LANGUAGE,
                &std::fs::read_to_string(path)?,
            )),
            "--parse-csharp4" => Some(nitride_cli::syntax_observe(
                &nitride_cli::csharp4::LANGUAGE,
                &std::fs::read_to_string(path)?,
            )),
            "--parse-composition" => Some(nitride_cli::syntax_observe(
                &nitride_cli::composition::LANGUAGE,
                &std::fs::read_to_string(path)?,
            )),
            _ => None,
        };
        if let Some(result) = result {
            println!("{result}");
            return Ok(result["ok"] == true);
        }
    }
    if args.len() == 3 && args[0] == "--bench" {
        let source = std::fs::read_to_string(&args[1])?;
        let iterations: u32 = args[2].parse()?;
        if iterations == 0 {
            return Err("iterations must be positive".into());
        }
        let bindings = nitride_cli::fixture_bindings();
        let start = std::time::Instant::now();
        let mut syntax_bytes = 0;
        let mut hir_bytes = 0;
        for _ in 0..iterations {
            let tree =
                parse(&nitride_cli::LANGUAGE, &source, Limits::default()).map_err(|e| e.message)?;
            let program = compile(&nitride_cli::LANGUAGE, &tree, Limits::default())
                .map_err(|e| e[0].message.clone())?;
            syntax_bytes = tree.retained_bytes();
            hir_bytes = program.nodes.capacity() * std::mem::size_of::<HirNode>()
                + program.args.capacity() * 4
                + program.statements.capacity() * std::mem::size_of::<Statement>()
                + program.bindings.capacity() * std::mem::size_of::<Binding>();
            std::hint::black_box(
                execute(&program, &bindings, Limits::default())
                    .map_err(|e| e.diagnostic.message)?,
            );
        }
        println!(
            "{}",
            serde_json::json!({"iterations":iterations,"elapsed_us":start.elapsed().as_micros(),"syntax_retained_capacity_bytes":syntax_bytes,"hir_retained_capacity_bytes":hir_bytes})
        );
        return Ok(true);
    }
    let path = match args.as_slice() { [path] => path, [flag, path] if flag == "--observe" => path, _ => return Err("usage: nitride prepare <project.nproj|project.json> [--output <language.json>] | catalog [<file.ncatalog>...] | interface [--target rust|wit|wit-binding] <project.nproj|project.json> | conform calendar|<project.nproj|project.json> | run <project.nproj|project.json> <source> | binding draft <project.nproj|project.json> <interface> <component.wasm> | [--grant <effect>]... | --calendar <source> | <source.portable> | --observe <source> | --functions <source> | --calls <source> | --dates <source> [--today yyyy-MM-dd] | --rewrites <source> [--today yyyy-MM-dd] | --inference <source> | --namespaces <source> | --power <source> | --parse-xml <source> | --parse-composition <source> | --parse-csharp4 <source> | --bench <source> <iterations> | --recover <profile> <source> | --recover-batch | lsp | --version".into()) };
    let result = nitride_cli::observe(&std::fs::read_to_string(path)?);
    println!("{result}");
    Ok(result["ok"] == true)
}
