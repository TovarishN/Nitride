//! Deterministic generated-parser observations for production PGO training.
use nitride_core::*;

pub fn observe(profile: &str, source: &str) -> Result<serde_json::Value, String> {
    let parser = match profile {
        "portable" => &crate::PARSER,
        "power" => &crate::power::PARSER,
        "composition" => &crate::composition::PARSER,
        "xml" => &crate::xml::PARSER,
        "namespaces" => &crate::namespaces::PARSER,
        "inference" => &crate::inference::PARSER,
        "calls" => &crate::calls::PARSER,
        "functions" => &crate::functions::PARSER,
        "dates" => &crate::dates::PARSER,
        "calendar" => &crate::calendar::PARSER,
        "rewrites" => &crate::rewrites::PARSER,
        "csharp4" => &crate::csharp4::PARSER,
        "grammar" => &crate::grammar_fixture::PARSER,
        _ => return Err(format!("unknown generated parser: {profile}")),
    };
    let snapshot = parse_snapshot_generated(
        parser,
        source,
        Limits::default(),
        SnapshotMode::Recovering(RecoveryOptions::default()),
    )
    .map_err(|d| d.message)?;
    let native_calls = snapshot.stats().native_syntax_calls;
    let original = shape(&snapshot)?;
    // Exercise the interpreted incremental path as well as generated cold parsing.
    let at = source.len() as u32;
    let edited = snapshot
        .edit(
            &TextEdit {
                range: [at, at],
                replacement: " ".into(),
            },
            Limits::default(),
        )
        .map_err(|d| d.message)?;
    Ok(
        serde_json::json!({"original":original,"edited":shape(&edited)?,"native_calls":native_calls}),
    )
}

fn shape(snapshot: &ParseSnapshot) -> Result<serde_json::Value, String> {
    let tree = snapshot.materialize().map_err(|d| d.message)?;
    let nodes: Vec<_> = tree
        .nodes
        .iter()
        .enumerate()
        .map(|(id, n)| {
            let children: Vec<_> = tree
                .children(id as u32)
                .iter()
                .map(|e| (tree.label(e), e.node))
                .collect();
            serde_json::json!([n.rule, n.span, children])
        })
        .collect();
    Ok(
        serde_json::json!({"root":tree.root,"source":tree.source,"nodes":nodes,
        "diagnostics":format!("{:?}",snapshot.diagnostics()),"recovery":format!("{:?}",tree.recovery())}),
    )
}
