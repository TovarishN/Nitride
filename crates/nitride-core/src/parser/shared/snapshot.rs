//! Snapshot parsing, editing, publication and stack spill lifecycle.
use super::*;

pub fn parse_snapshot(
    language: &'static Language<'static>,
    source: &str,
    limits: Limits,
    mode: SnapshotMode,
) -> Result<ParseSnapshot, Diagnostic> {
    crate::cancellation::checkpoint([0, 0])?;
    parse_borrowed_with_tables(
        language,
        source,
        limits,
        mode,
        Some(LanguageTables::of(language)),
    )
}
pub fn parse_borrowed_snapshot<'g>(
    language: &'g Language<'g>,
    source: &str,
    limits: Limits,
    mode: SnapshotMode,
) -> Result<BorrowedParseSnapshot<'g>, Diagnostic> {
    parse_borrowed_with_tables(language, source, limits, mode, None)
}
fn parse_borrowed_with_tables<'g>(
    language: &'g Language<'g>,
    source: &str,
    limits: Limits,
    mode: SnapshotMode,
    tables: Option<std::sync::Arc<LanguageTables<'g>>>,
) -> Result<BorrowedParseSnapshot<'g>, Diagnostic> {
    crate::cancellation::checkpoint([0, 0])?;
    let text = SourceText::new(source)?;
    match mode {
        SnapshotMode::Strict(options) => parse_with_spill(
            language,
            Input {
                tables: tables.clone(),
                compiled: None,
                lexers: None,
                text: &text,
                contiguous: Some(source),
            },
            limits,
            options,
            None,
            Index::default(),
            None,
        ),
        SnapshotMode::Recovering(_) => recover_cold(
            language,
            Input {
                tables: tables.clone(),
                compiled: None,
                lexers: None,
                text: &text,
                contiguous: Some(source),
            },
            limits,
            mode,
        ),
    }
}
/// Cold recovering parse: a strict attempt first, then recovery seeded with
/// that attempt's recognition facts instead of recognizing the prefix again.
fn recover_cold<'g>(
    language: &'g Language<'g>,
    input: Input<'_, 'g>,
    limits: Limits,
    mode: SnapshotMode,
) -> Result<BorrowedParseSnapshot<'g>, Diagnostic> {
    let SnapshotMode::Recovering(options) = mode else {
        unreachable!("cold recovery requires recovering mode")
    };
    let mut seed = None;
    let error = match parse_with_spill(
        language,
        input.clone(),
        limits,
        ParseOptions {
            strict_comments: true,
            ..Default::default()
        },
        None,
        Index::default(),
        Some(&mut seed),
    ) {
        Ok(mut snapshot) => {
            snapshot.mode = mode;
            return Ok(snapshot);
        }
        Err(error) => error,
    };
    crate::recovery::incremental::recover_snapshot(
        language,
        input.text,
        limits,
        options,
        None,
        Some(error),
        seed.as_ref(),
    )
}
/// Use generated lexical scanners for cold contiguous input. Scanner errors
/// rerun the ordinary parser, retaining its established diagnostics/recovery.
pub fn parse_snapshot_compiled(
    lexers: &'static crate::lexical::CompiledLexers,
    source: &str,
    limits: Limits,
    mode: SnapshotMode,
) -> Result<ParseSnapshot, Diagnostic> {
    parse_snapshot_specialized(lexers, None, source, limits, mode)
}
/// Use generated structural syntax bodies and lexical scanners for cold input.
/// Tree construction, ranking and recovery semantics remain in the runtime.
pub fn parse_snapshot_generated(
    compiled: &'static CompiledParser,
    source: &str,
    limits: Limits,
    mode: SnapshotMode,
) -> Result<ParseSnapshot, Diagnostic> {
    parse_snapshot_specialized(compiled.lexers, Some(compiled), source, limits, mode)
}
fn parse_snapshot_specialized(
    lexers: &'static crate::lexical::CompiledLexers,
    compiled: Option<&'static CompiledParser>,
    source: &str,
    limits: Limits,
    mode: SnapshotMode,
) -> Result<ParseSnapshot, Diagnostic> {
    crate::cancellation::checkpoint([0, 0])?;
    let language = lexers.language;
    let tables = LanguageTables::of(language);
    if lexers.scanners.len() != language.rules.len()
        || compiled.is_some_and(|p| {
            p.bodies.len() != language.rules.len() || p.tails.len() != language.rules.len()
        })
    {
        return parse_borrowed_with_tables(language, source, limits, mode, Some(tables.clone()));
    }
    let text = SourceText::new(source)?;
    let options = match mode {
        SnapshotMode::Strict(options) => options,
        SnapshotMode::Recovering(_) => ParseOptions {
            strict_comments: true,
            ..Default::default()
        },
    };
    let mut seed = None;
    let result = parse_with_spill(
        language,
        Input {
            tables: Some(tables.clone()),
            lexers: Some(lexers),
            compiled,
            text: &text,
            contiguous: Some(source),
        },
        limits,
        options,
        None,
        Index::default(),
        matches!(mode, SnapshotMode::Recovering(_)).then_some(&mut seed),
    );
    match (result, mode) {
        (Ok(mut snapshot), _) => {
            snapshot.mode = mode;
            Ok(snapshot)
        }
        // A syntax failure is the ordinary recovery trigger. Seed recovery with
        // the generated attempt's facts instead of a second interpreted attempt.
        (Err(error), SnapshotMode::Recovering(options)) if error.category == "syntax" => {
            crate::recovery::incremental::recover_snapshot(
                language,
                &text,
                limits,
                options,
                None,
                Some(error),
                seed.as_ref(),
            )
        }
        (Err(error), _) if error.category == "cancelled" => Err(error),
        (Err(_), _) => {
            drop(text);
            parse_borrowed_with_tables(language, source, limits, mode, Some(tables.clone()))
        }
    }
}
impl<'g> BorrowedParseSnapshot<'g> {
    pub fn edit(&self, edit: &TextEdit, limits: Limits) -> Result<Self, Diagnostic> {
        crate::cancellation::checkpoint(edit.range)?;
        let text = self.source.replace(edit)?;
        let delta = edit.replacement.len() as i64 - i64::from(edit.range[1] - edit.range[0]);
        let (index, _) = self.recognitions.edit(edit.range, delta);
        match self.mode {
            SnapshotMode::Strict(options) => parse_with_spill(
                self.language,
                Input {
                    tables: None,
                    compiled: None,
                    lexers: None,
                    text: &text,
                    contiguous: None,
                },
                limits,
                options,
                Some(self),
                index,
                None,
            )
            .map(|mut snapshot| {
                snapshot.stats.source_bytes_copied = edit.replacement.len() as u64;
                snapshot
            }),
            SnapshotMode::Recovering(options) => {
                let failure = {
                    match parse_with_spill(
                        self.language,
                        Input {
                            tables: None,
                            compiled: None,
                            lexers: None,
                            text: &text,
                            contiguous: None,
                        },
                        limits,
                        ParseOptions {
                            strict_comments: true,
                            ..Default::default()
                        },
                        Some(self),
                        index,
                        None,
                    ) {
                        Ok(mut snapshot) => {
                            snapshot.mode = self.mode;
                            snapshot.stats.source_bytes_copied = edit.replacement.len() as u64;
                            return Ok(snapshot);
                        }
                        Err(error) => Some(error),
                    }
                };
                crate::recovery::incremental::recover_snapshot(
                    self.language,
                    &text,
                    limits,
                    options,
                    Some((self, edit)),
                    failure,
                    None,
                )
            }
        }
    }
}
fn parse_with_spill<'a, 'g>(
    language: &'g Language<'g>,
    text: Input<'a, 'g>,
    limits: Limits,
    options: ParseOptions,
    base: Option<&BorrowedParseSnapshot<'g>>,
    index: Index,
    mut seed: Option<&mut Option<Seed<'a, 'g>>>,
) -> Result<BorrowedParseSnapshot<'g>, Diagnostic> {
    let result = parse_source(
        language,
        text.clone(),
        limits,
        options,
        limits.parser_stack_bytes,
        base,
        index.clone(),
        seed.as_deref_mut(),
    );
    if result
        .as_ref()
        .is_err_and(|e| e.message == crate::parser::STACK_EXHAUSTED)
        && limits.parser_spill_stack_bytes > limits.parser_stack_bytes
    {
        let cancellation = crate::cancellation::Context::capture();
        std::thread::scope(|scope| {
            let budget = limits.parser_spill_stack_bytes - limits.parser_spill_stack_bytes / 8;
            std::thread::Builder::new()
                .name("nitride-shared-parse".into())
                .stack_size(limits.parser_spill_stack_bytes)
                .spawn_scoped(scope, move || {
                    cancellation.run(|| {
                        parse_source(language, text, limits, options, budget, base, index, seed)
                    })
                })
                .map_err(|_| Diagnostic::new("limit", [0, 0], "cannot start a deep-parse thread"))?
                .join()
                .unwrap_or_else(|_| Err(Diagnostic::new("limit", [0, 0], "deep parse failed")))
        })
    } else {
        result
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn parse_source<'a, 'g>(
    language: &'g Language<'g>,
    text: Input<'a, 'g>,
    limits: Limits,
    options: ParseOptions,
    stack_budget: usize,
    base: Option<&BorrowedParseSnapshot<'g>>,
    index: Index,
    seed: Option<&mut Option<Seed<'a, 'g>>>,
) -> Result<BorrowedParseSnapshot<'g>, Diagnostic> {
    let input = text;
    let text = input.text;
    let mut parser = Parser::new(language, input, limits, options, stack_budget, base, index);
    let root = parser.rule(language.start, 0, 0)?;
    parser.trivia()?;
    if root.is_none() || parser.position != text.len() as usize {
        let at = parser.furthest.max(parser.position) as u32;
        if let Some(slot) = seed {
            // Seeding is an optional accelerator; recovery runs either way.
            *slot = parser.into_seed().ok();
        }
        return Err(Diagnostic::new("syntax", [at, at], "invalid source"));
    }
    publish(
        parser,
        text,
        root.unwrap(),
        base,
        SnapshotMode::Strict(options),
    )
}
pub(super) fn publish<'g>(
    mut parser: Parser<'_, 'g>,
    text: &SourceText,
    root: Handle,
    base: Option<&BorrowedParseSnapshot<'g>>,
    mode: SnapshotMode,
) -> Result<BorrowedParseSnapshot<'g>, Diagnostic> {
    crate::cancellation::checkpoint([parser.position as u32; 2])?;
    parser.arena.retain(root.node);
    parser.clear_memo(0);
    parser.clear_memo(1);
    for hit in parser.facts.values() {
        parser.arena.release_cache(hit.root);
    }
    parser.arena.restore(0);
    parser
        .facts
        .retain_entries(|hit| hit.root == SubtreeId::NONE || parser.arena.node(hit.root).is_some());
    drop(parser.memo);
    drop(parser.edge_buffers);
    let copied_bytes = parser.arena.copied_bytes();
    let allocation_charge = parser.arena.publication_allocation_bound();
    // Releases free most failed attempts during the parse, and every live node
    // is now reachable from the root. A cold strict graph with few dead slots
    // is published in place: compaction would copy it all to drop little.
    // Recovery publishes over the failed attempt's shared seed chunks, where
    // compaction rebuilds a graph laid out for the edits that follow.
    let (alive, slots) = parser.arena.occupancy();
    let sparse = slots - alive > slots / 8;
    let maintenance_debt = base
        .map_or(0, |b| b.maintenance_debt)
        .saturating_add(allocation_charge);
    let mut arena = parser.arena.finish();
    arena.labels = parser.labels.into();
    // A cold parse's facts are packed after compaction below, sorted by
    // position in linear time. A cold seed's facts are already in the index;
    // compaction below keeps those still reachable from the published root.
    let mut cold = None;
    if base.is_none() && parser.index.is_empty() {
        cold = Some(std::mem::take(&mut parser.facts).into_entries());
    } else if base.is_none() {
        parser.index = parser
            .index
            .merged(std::mem::take(&mut parser.facts).into_entries());
    } else {
        for (at, key, hit) in std::mem::take(&mut parser.facts).into_entries() {
            parser.index.insert(at, key, hit);
        }
    }
    let mut relocated = vec![root.node];

    // New live ancestors count toward the allocation debt too. Use the
    // approved half-fresh-output slack before evacuating an entire graph.
    let allowance =
        (8_usize * 1024 * 1024).saturating_add(base.map_or(0, |b| b.maintenance_base / 2));
    let compact = match base {
        None => sparse || matches!(mode, SnapshotMode::Recovering(_)),
        Some(b) => {
            maintenance_debt > allowance
                || u64::from(text.len()) * 3 < u64::from(b.source.len()) * 2
        }
    };
    let root = if compact {
        let (next, mapping) = arena.compact(&mut relocated)?;
        arena = next;
        parser.stats.reclaimed_syntax_bytes = mapping.removed_bytes as u64;
        parser.stats.syntax_bytes_copied += mapping.copied_bytes as u64;
        parser.stats.maintenance_passes = u64::from(base.is_some());
        let map = |id| {
            if id == SubtreeId::NONE {
                Some(id)
            } else {
                mapping.get(id)
            }
        };
        match &mut cold {
            Some(facts) => remap_facts(facts, map),
            None => parser.index = parser.index.retain_mapped(&map),
        }
        Handle {
            node: relocated[0],
            ..root
        }
    } else {
        root
    };
    let mut snapshot = BorrowedParseSnapshot::from_parts(
        parser.language,
        text.clone(),
        arena,
        root.node,
        root.start,
        mode,
    );
    // A cold graph published in place starts debt-free, as a compacted one.
    if !compact && base.is_some() {
        snapshot.maintenance_debt = maintenance_debt;
        snapshot.maintenance_base = base.map_or(snapshot.maintenance_base, |b| b.maintenance_base);
    }
    snapshot.stats = parser.stats;
    snapshot.stats.work = parser.work;
    snapshot.recognitions = match cold {
        Some(facts) => Index::from_unsorted(facts, text.len()),
        None => parser.index,
    };
    snapshot.tables = Some(parser.tables);
    snapshot.dispatch = Some(parser.dispatch);
    parser.reused_ranges.sort_unstable();
    let mut covered = 0_u64;
    let mut end = 0_u32;
    for range in parser.reused_ranges {
        if range[1] > end {
            covered += u64::from(range[1] - end.max(range[0]));
            end = range[1];
        }
    }
    snapshot.stats.reparsed_bytes = u64::from(text.len()).saturating_sub(covered);
    snapshot.stats.syntax_bytes_copied += copied_bytes;
    snapshot.stats.source_bytes_copied = if base.is_none() {
        u64::from(text.len())
    } else {
        0
    };
    crate::cancellation::checkpoint([text.len(); 2])?;
    Ok(snapshot)
}
/// Points each fact at its syntax's new id after compaction, dropping facts
/// whose syntax was reclaimed, as [`Index::retain_mapped`] does.
pub(super) fn remap_facts(
    facts: &mut Vec<(u32, Key, Hit)>,
    map: impl Fn(SubtreeId) -> Option<SubtreeId>,
) {
    facts.retain_mut(|(.., hit)| map(hit.root).map(|root| hit.root = root).is_some());
}
