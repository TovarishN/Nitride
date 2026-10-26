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
    let args: Vec<_> = std::env::args().skip(1).collect();
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
                    "usage: nitride prepare <project.json> [--output <language.json>]".into(),
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
    let path = match args.as_slice() { [path] => path, [flag, path] if flag == "--observe" => path, _ => return Err("usage: nitride prepare <project.json> [--output <language.json>] | <source.portable> | --observe <source> | --functions <source> | --calls <source> | --dates <source> [--today yyyy-MM-dd] | --rewrites <source> [--today yyyy-MM-dd] | --inference <source> | --namespaces <source> | --power <source> | --parse-xml <source> | --parse-composition <source> | --parse-csharp4 <source> | --bench <source> <iterations> | --recover <profile> <source> | --recover-batch | lsp | --version".into()) };
    let result = nitride_cli::observe(&std::fs::read_to_string(path)?);
    println!("{result}");
    Ok(result["ok"] == true)
}
