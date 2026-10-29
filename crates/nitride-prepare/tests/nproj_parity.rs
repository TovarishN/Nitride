//! The repository's `.nproj` files say what Nitrogen's JSON manifests say, and their
//! resolved snapshots (Nitrogen's parity contract) are current.
use nitride_prepare::{ProjectLanguage, Source, read_language, read_workspace, snapshot};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// `path` with `.` and `..` resolved lexically.
fn normal(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

fn normals(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths.iter().map(|p| normal(p)).collect()
}

fn json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

fn paths(base: &Path, value: &Value) -> Vec<PathBuf> {
    strings(value)
        .iter()
        .map(|p| normal(&base.join(p)))
        .collect()
}

fn workspace() -> Vec<ProjectLanguage> {
    let path = root().join("nitride.nproj");
    let source = Source {
        path: path.display().to_string(),
        text: std::fs::read_to_string(&path).unwrap(),
    };
    let workspace = read_workspace(
        &source,
        &|p: &Path| std::fs::read_to_string(p).ok(),
        &|p: &Path| p.exists(),
    );
    assert!(workspace.errors.is_empty(), "{:?}", workspace.errors);
    workspace.languages
}

/// Every `.nproj` under `languages/`, by folder.
fn folder_languages() -> Vec<(PathBuf, ProjectLanguage)> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(root().join("languages")).unwrap() {
        let folder = entry.unwrap().path();
        for file in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let path = file.path();
            if path.extension().is_some_and(|e| e == "nproj") {
                let source = Source {
                    path: path.display().to_string(),
                    text: std::fs::read_to_string(&path).unwrap(),
                };
                let language = read_language(&source, &|p: &Path| p.exists())
                    .unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
                found.push((folder.clone(), language));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

#[test]
fn nproj_matches_nitrogen_json() {
    let root = root();
    let manifest = json(&root.join("nitrogen.json"));
    let languages = workspace();
    let entries = manifest["languages"].as_array().unwrap();
    let rename = |name: &str| match name {
        "XmlReference" => "Xml".to_owned(),
        other => other.to_owned(),
    };
    // Same languages, in the same order (nitride.json, now retired, matched too).
    assert_eq!(
        languages.iter().map(|l| l.name.clone()).collect::<Vec<_>>(),
        entries
            .iter()
            .map(|e| rename(e["name"].as_str().unwrap()))
            .collect::<Vec<_>>()
    );
    for entry in entries {
        let name = &rename(entry["name"].as_str().unwrap());
        let language = languages.iter().find(|l| &l.name == name).unwrap();
        assert_eq!(language.extensions, strings(&entry["extensions"]), "{name}");
        assert_eq!(language.start, entry["start"].as_str().unwrap(), "{name}");
        let nitrogen = language
            .nitrogen
            .as_ref()
            .unwrap_or_else(|| panic!("{name}"));
        assert_eq!(
            nitrogen.namespace,
            entry["namespace"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(
            normals(&nitrogen.grammars),
            paths(&root, &entry["grammars"]),
            "{name}"
        );
        assert_eq!(
            normals(&nitrogen.sources),
            paths(&root, &entry["sources"]),
            "{name}"
        );
    }
}

#[test]
fn nproj_matches_project_json() {
    let languages = folder_languages();
    let mut compared = 0;
    for (folder, language) in &languages {
        let Ok(text) = std::fs::read_to_string(folder.join("project.json")) else {
            continue;
        };
        let manifest: Value = serde_json::from_str(&text).unwrap();
        let name = &language.name;
        assert_eq!(
            language.start,
            manifest["start"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(
            normals(&language.grammars),
            paths(folder, &manifest["grammars"]),
            "{name}"
        );
        assert_eq!(
            normals(&language.catalogs),
            paths(folder, &manifest["catalogs"]),
            "{name}"
        );
        let typed = manifest["mode"].as_str().unwrap_or("typed") == "typed";
        assert_eq!(language.typed(), typed, "{name}");
        compared += 1;
    }
    // Every language folder but `signed` has a project.json (`project`'s goes
    // once bootstrap reads .nproj).
    assert!(compared >= 13, "{compared}");
    assert_eq!(languages.len(), 15);
}

#[test]
fn snapshots_are_current() {
    let root = root();
    let fixtures = root.join("crates/nitride-prepare/tests/fixtures/nproj");
    let bless = std::env::var_os("NITRIDE_BLESS").is_some();
    for (_, language) in folder_languages() {
        let snapshot = snapshot(&language, &root);
        let path = fixtures.join(format!("{}.json", language.name));
        let text = serde_json::to_string_pretty(&snapshot).unwrap() + "\n";
        if bless {
            std::fs::create_dir_all(&fixtures).unwrap();
            std::fs::write(&path, text).unwrap();
        } else {
            let expected = std::fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("missing {}; run with NITRIDE_BLESS=1", path.display()));
            assert_eq!(text, expected, "stale snapshot {}", path.display());
        }
    }
}
