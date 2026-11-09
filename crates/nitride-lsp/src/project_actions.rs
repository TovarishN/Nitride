//! Project actions are rederived from current structured context on resolve.
use crate::{
    document::Document,
    lines::LineIndex,
    registry::{Registry, same_file},
    server::range,
    uri,
};
use nitride_prepare::{
    Source,
    authoring::{ProjectIssueKind, missing_file_scaffold, project_issues, workspace_target},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::{Component, Path, PathBuf},
};

pub struct Context<'a> {
    pub documents: &'a HashMap<String, Document>,
    pub registry: &'a Registry,
    pub roots: &'a [PathBuf],
    pub create: bool,
    pub resolve: bool,
}
fn normalized(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}
fn reference_target(path: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    // An open workspace may itself use a platform alias (/var on macOS).
    // Check symlink components only below that trusted root, before folding '..'.
    let root = roots.iter().find(|root| path.starts_with(root))?;
    let mut prefix = root.clone();
    for part in path.strip_prefix(root).ok()?.components() {
        prefix.push(part);
        if !normalized(&prefix).starts_with(root)
            || std::fs::symlink_metadata(&prefix).is_ok_and(|m| m.file_type().is_symlink())
        {
            return None;
        }
    }
    workspace_target(&normalized(path), roots).ok()
}
fn fingerprint(text: &str) -> String {
    let mut hash = std::hash::DefaultHasher::new();
    text.hash(&mut hash);
    format!("{:x}", hash.finish())
}
impl Context<'_> {
    fn open(&self, path: &Path) -> Option<(&str, &Document)> {
        self.documents
            .iter()
            .find(|(u, _)| uri::to_path(u).is_some_and(|p| same_file(&p, path)))
            .map(|(u, d)| (u.as_str(), d))
    }
    fn source(&self, path: &Path) -> Option<(Source, Option<i32>)> {
        let (text, version) = if let Some((_, doc)) = self.open(path) {
            (doc.text.clone(), Some(doc.version))
        } else {
            (std::fs::read_to_string(path).ok()?, None)
        };
        Some((
            Source {
                path: path.display().to_string(),
                text,
            },
            version,
        ))
    }
    pub fn actions(&self, document_uri: &str, start: u32, end: u32, only: &[String]) -> Vec<Value> {
        if !only.is_empty() && !only.iter().any(|k| k == "quickfix" || k.is_empty()) {
            return Vec::new();
        }
        self.candidates(document_uri)
            .into_iter()
            .filter(|(a, _)| {
                let span = &a["data"]["span"];
                span[0].as_u64().unwrap_or(0) <= end as u64
                    && start as u64 <= span[1].as_u64().unwrap_or(0)
            })
            .map(|(a, _)| a)
            .collect()
    }
    pub fn resolve_action(&self, action: &Value) -> Value {
        let Some(document_uri) = action["data"]["uri"].as_str() else {
            return action.clone();
        };
        for (mut candidate, edit) in self.candidates(document_uri) {
            let mut requested = action["data"].clone();
            let pin = requested["code"] == "NLIB0004";
            // A client may open the unchanged project while resolving a pin.
            if pin && requested["projectVersion"].is_null() {
                requested["projectVersion"] = candidate["data"]["projectVersion"].clone();
            }
            if candidate["data"] == requested && (!pin || !requested["projectVersion"].is_null()) {
                candidate["edit"] = edit;
                return candidate;
            }
        }
        let mut stale = action.clone();
        if let Some(object) = stale.as_object_mut() {
            object.remove("edit");
        }
        stale["disabled"] = json!({"reason":"Project context changed or target is no longer safe"});
        stale
    }
    fn candidates(&self, document_uri: &str) -> Vec<(Value, Value)> {
        if !self.resolve {
            return Vec::new();
        }
        let Some(path) = uri::to_path(document_uri) else {
            return Vec::new();
        };
        let Some((source, version)) = self.source(&path) else {
            return Vec::new();
        };
        let mut result = Vec::new();
        if path.extension().is_some_and(|e| e == "nproj") && self.create {
            let issues = project_issues(&source, &|p| self.open(p).is_some() || p.exists());
            let missing_count = issues
                .iter()
                .filter(|i| i.kind == ProjectIssueKind::MissingGrammar)
                .count();
            let language = nitride_prepare::read_language(&source, &|_| true).ok();
            for issue in issues {
                let Some(target) =
                    reference_target(issue.target.as_ref().expect("missing file"), self.roots)
                else {
                    continue;
                };
                if target.symlink_metadata().is_ok() || self.open(&target).is_some() {
                    continue;
                }
                let start = language
                    .as_ref()
                    .filter(|_| {
                        missing_count == 1 && issue.kind == ProjectIssueKind::MissingGrammar
                    })
                    .and_then(|l| {
                        let index = crate::grammar_index::build(
                            &l.grammars,
                            &|p| self.open(p).map(|(_, d)| d.text.clone()),
                            &crate::grammar_index::default_uri,
                        );
                        (index.definition(&l.start, "").is_none()
                            && index.unreadable.iter().all(|p| same_file(p, &target))
                            && index.files.iter().all(|f| !f.damaged))
                        .then_some(l.start.as_str())
                    });
                let Ok(contents) = missing_file_scaffold(&issue.kind, &target, start) else {
                    continue;
                };
                let target_uri = uri::from_path(&target);
                let span = issue.span.map(|b| b as u32);
                let lines = LineIndex::new(&source.text);
                let data = json!({"uri":document_uri,"version":version,"hash":fingerprint(&source.text),"code":issue.code,"span":issue.span,"target":target_uri});
                let action = json!({"title":format!("Create {}",target.file_name().unwrap_or_default().to_string_lossy()),"kind":"quickfix","diagnostics":[{"range":range(&lines,span),"message":"Referenced project file is missing","code":issue.code}],"data":data});
                let zero = json!({"line":0,"character":0});
                let edit = json!({"documentChanges":[{"kind":"create","uri":target_uri,"options":{"overwrite":false,"ignoreIfExists":false}},{"textDocument":{"uri":target_uri,"version":null},"edits":[{"range":{"start":zero,"end":zero},"newText":contents}]}]});
                result.push((action, edit));
            }
        }
        // Only explicit qualified lexical uses and exactly one owning project.
        let users = self.registry.using_grammar(&path).collect::<Vec<_>>();
        if users.len() == 1
            && users[0]
                .entry
                .libraries
                .iter()
                .all(|r| r.name != "Nitride.Lexical")
        {
            let owner = &users[0].manifest;
            let Some((project, project_version)) = self.source(owner) else {
                return result;
            };
            // VS Code opens this buffer before resolve; no edit is emitted
            // until the server has a matching, versioned project document.
            let project_uri = self
                .open(owner)
                .map(|(uri, _)| uri.to_owned())
                .unwrap_or_else(|| crate::grammar_index::default_uri(owner));
            if workspace_target(&normalized(owner), self.roots).is_err() {
                return result;
            }
            let Ok(language) = nitride_prepare::read_language(&project, &|_| true) else {
                return result;
            };
            if language
                .libraries
                .iter()
                .any(|r| r.name == "Nitride.Lexical")
            {
                return result;
            }
            let used = nitride_prepare::grammar_references(&source)
                .unwrap_or_default()
                .into_iter()
                .find(|r| {
                    r.kind == "rule"
                        && r.name.starts_with("Nitride.Lexical.")
                        && nitride_prepare::stdlib::libraries()[0]
                            .exports
                            .contains(&r.name.rsplit('.').next().unwrap_or_default())
                });
            if let Some(used) = used {
                let Some(insert) = nitride_prepare::authoring::library_pin_insertion(&project)
                else {
                    return result;
                };
                let lines = LineIndex::new(&project.text);
                let point = range(&lines, [insert as u32; 2]);
                let data = json!({"uri":document_uri,"version":version,"hash":fingerprint(&source.text),"code":"NLIB0004","span":used.span,"project":project_uri,"projectVersion":project_version,"projectHash":fingerprint(&project.text)});
                let action = json!({"title":"Pin bundled Nitride.Lexical@1.0.0","kind":"quickfix","data":data});
                let edit = json!({"documentChanges":[{"textDocument":{"uri":project_uri,"version":project_version},"edits":[{"range":point,"newText":"\n  library Nitride.Lexical version 1.0.0;\n"}]}]});
                result.push((action, edit));
            }
        }
        result
    }
}
