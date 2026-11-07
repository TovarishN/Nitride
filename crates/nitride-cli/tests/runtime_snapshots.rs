use nitride_codegen::{LoadedLanguage, RuntimeSnapshot};
use nitride_core::{Limits, SnapshotMode, ParseOptions, TextEdit};
use std::sync::Arc;

fn owner() -> Arc<LoadedLanguage> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../languages/portable/project.json");
    let prepared = nitride_prepare::prepare_project(&root).unwrap();
    Arc::new(LoadedLanguage::load(&prepared.data.to_string()).unwrap())
}

#[test]
fn edited_snapshots_retain_owner_and_release_it_with_last_handle() {
    let owner = owner();
    let weak = Arc::downgrade(&owner);
    let original = RuntimeSnapshot::parse(owner.clone(), "let x : Scalar = 1; x;", Limits::default(), SnapshotMode::Strict(ParseOptions::default())).unwrap();
    drop(owner);
    let next = original.edit(&TextEdit { range: [17,18], replacement: "2".into() }, Limits::default()).unwrap();
    let clone = next.clone();
    assert_eq!(original.with_tree(|tree| tree.source.to_string()).unwrap(), "let x : Scalar = 1; x;");
    assert_eq!(next.with_tree(|tree| tree.source.to_string()).unwrap(), "let x : Scalar = 2; x;");
    next.with_tree(|tree| {
        assert!(tree.nodes.len() > 1);
        for id in 0..tree.nodes.len() {
            for edge in tree.children(id as u32) {
                let _ = tree.label(edge);
            }
        }
    }).unwrap();
    drop(original);
    drop(next);
    assert!(weak.upgrade().is_some());
    drop(clone);
    assert!(weak.upgrade().is_none());
}

#[test]
fn old_readers_survive_language_reload_and_concurrent_edits() {
    let first_owner = owner();
    let original = RuntimeSnapshot::parse(first_owner, "let x : Scalar = 1; x;", Limits::default(), SnapshotMode::Strict(ParseOptions::default())).unwrap();
    let reloaded = RuntimeSnapshot::parse(owner(), "2;", Limits::default(), SnapshotMode::Strict(ParseOptions::default())).unwrap();
    let reader = original.clone();
    std::thread::scope(|scope| {
        let thread = scope.spawn(move || reader.with_tree(|tree| tree.source.to_string()).unwrap());
        let edited = original.edit(&TextEdit { range: [17,18], replacement: "3".into() }, Limits::default()).unwrap();
        assert_eq!(edited.with_tree(|tree| tree.source.to_string()).unwrap(), "let x : Scalar = 3; x;");
        assert_eq!(thread.join().unwrap(), "let x : Scalar = 1; x;");
    });
    assert_eq!(reloaded.with_tree(|tree| tree.source.to_string()).unwrap(), "2;");
}

#[test]
fn runtime_checks_preserve_structural_diagnostics_and_allow_reentrant_reads() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../languages/xml/project.json");
    let prepared = nitride_prepare::prepare_project(&root).unwrap();
    let owner = Arc::new(LoadedLanguage::load(&prepared.data.to_string()).unwrap());
    let snapshot = RuntimeSnapshot::parse(owner, "<a></b>", Limits::default(), SnapshotMode::Recovering(Default::default())).unwrap();
    snapshot.with_tree(|_| {
        assert!(snapshot.with_tree(|tree| tree.nodes.len()).unwrap() > 1);
    }).unwrap();
    snapshot.with_compiled(Limits::default(), |checked| {
        assert!(checked.diagnostics.iter().any(|d| d.code.as_deref() == Some("XML0001")), "{:?}", checked.diagnostics);
    }).unwrap();
}
