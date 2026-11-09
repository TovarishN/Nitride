use nitride_testing::{Report, Runner, Snapshot, discover, identity};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    io::{Read, Write},
    path::PathBuf,
};
pub fn run(args: &[String]) -> Result<bool, Box<dyn std::error::Error>> {
    let (mut paths, mut selectors, mut json_output, mut events, mut list, mut stdin) =
        (Vec::new(), Vec::new(), false, false, false, false);
    let mut given = args.iter();
    let mut flags = HashSet::new();
    while let Some(arg) = given.next() {
        match arg.as_str() {
            "--case" => selectors.push(given.next().ok_or("--case needs a name")?.clone()),
            "--json" | "--events" | "--list" | "--snapshot-stdin" => {
                if !flags.insert(arg) {
                    return Err(format!("repeated {arg}").into());
                }
                match arg.as_str() {
                    "--json" => json_output = true,
                    "--events" => events = true,
                    "--list" => list = true,
                    _ => stdin = true,
                }
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown test option {other}").into());
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    if json_output && events || list && events {
        return Err("--events cannot combine with --json or --list".into());
    }
    if paths.is_empty() {
        paths.push("tests.ntest".into());
    }
    if !selectors.is_empty() && (paths.len() != 1 || list) {
        return Err("--case requires one executing suite".into());
    }
    let mut identities = HashSet::new();
    for path in &paths {
        if !identities.insert(identity(path)?) {
            return Err("repeated suite path".into());
        }
    }
    let snapshot = if stdin {
        let mut text = String::new();
        std::io::stdin()
            .take(64 * 1024 * 1024 + 1)
            .read_to_string(&mut text)?;
        if text.len() > 64 * 1024 * 1024 {
            return Err("snapshot envelope exceeds64 MiB".into());
        }
        Snapshot::from_json(&serde_json::from_str::<Value>(&text)?)?
    } else {
        Snapshot::default()
    };
    if list {
        let (mut suites, mut errors) = (Vec::new(), Vec::new());
        for path in paths {
            match discover(&path, &snapshot) {
                Ok(suite) => suites.push(suite.json()),
                Err(e) => errors.push(json!({"path":path,"span":e.span,"message":e.message})),
            }
        }
        println!("{}", json!({"version":1,"suites":suites,"errors":errors}));
        if !errors.is_empty() {
            return Err("test discovery failed".into());
        }
        return Ok(true);
    }
    let mut output = std::io::BufWriter::new(std::io::stdout().lock());
    if events {
        writeln!(output, "{}", json!({"version":1,"event":"runStarted"}))?;
        output.flush()?;
    }
    let mut report = Report::default();
    let mut runner = Runner::new(&snapshot);
    let mut write_error = None;
    for path in &paths {
        let result = runner.run(path, &selectors, &mut |event| {
            if events && write_error.is_none() {
                if let Err(error) = writeln!(output, "{event}").and_then(|_| output.flush()) {
                    write_error = Some(error);
                }
            }
        });
        report.append(result);
        if write_error.is_some() {
            break;
        }
    }
    if let Some(error) = write_error {
        return Err(error.into());
    }
    let result = report.json();
    if events {
        writeln!(
            output,
            "{}",
            json!({"version":1,"event":"runFinished","report":result})
        )?;
    } else if json_output {
        writeln!(output, "{result}")?;
    } else {
        for result in &report.results {
            writeln!(
                output,
                "{} {}: {}",
                result["status"]
                    .as_str()
                    .unwrap_or("errored")
                    .to_uppercase(),
                result["name"].as_str().unwrap_or_default(),
                result["message"].as_str().unwrap_or_default()
            )?;
        }
        for error in &report.errors {
            writeln!(
                output,
                "ERROR {}",
                error["message"].as_str().unwrap_or("test configuration")
            )?;
        }
        writeln!(output, "{}", result["counts"])?;
    }
    output.flush()?;
    match report.exit_code() {
        0 => Ok(true),
        1 => Ok(false),
        _ => Err("test run has infrastructure or configuration errors".into()),
    }
}
