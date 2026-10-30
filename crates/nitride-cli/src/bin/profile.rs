use nitride_core::*;
#[cfg(not(feature = "uninstrumented-profiler"))]
use std::alloc::{GlobalAlloc, Layout, System};
const COUNT_ALLOCATIONS: bool = !cfg!(feature = "uninstrumented-profiler");
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

#[cfg(not(feature = "uninstrumented-profiler"))]
struct CountingSystem;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

#[cfg(not(feature = "uninstrumented-profiler"))]
fn added(bytes: usize) {
    let live = LIVE.fetch_add(bytes, SeqCst) + bytes;
    PEAK.fetch_max(live, SeqCst);
    ALLOCATIONS.fetch_add(1, SeqCst);
}

// Instrument the unchanged System allocator in this executable only. No allocation,
// formatting or locks occur in these hooks. Only successful allocations are counted.
#[cfg(not(feature = "uninstrumented-profiler"))]
unsafe impl GlobalAlloc for CountingSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            added(layout.size());
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            added(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe {
            System.dealloc(pointer, layout);
        }
        LIVE.fetch_sub(layout.size(), SeqCst);
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(pointer, layout, size) };
        if !next.is_null() {
            if size >= layout.size() {
                added(size - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - size, SeqCst);
                ALLOCATIONS.fetch_add(1, SeqCst);
            }
        }
        next
    }
}

#[cfg(not(feature = "uninstrumented-profiler"))]
#[global_allocator]
static ALLOCATOR: CountingSystem = CountingSystem;

struct Phase {
    baseline_live_bytes: usize,
    retained_bytes: usize,
    peak_bytes: usize,
    allocation_count: usize,
    elapsed_ns: u128,
    net_retained_bytes: i64,
}

impl Phase {
    fn json(&self) -> serde_json::Value {
        serde_json::json!({"allocation_instrumented":COUNT_ALLOCATIONS,"baseline_live_bytes":COUNT_ALLOCATIONS.then_some(self.baseline_live_bytes),"retained_bytes":COUNT_ALLOCATIONS.then_some(self.retained_bytes),
            "peak_bytes":COUNT_ALLOCATIONS.then_some(self.peak_bytes),"allocation_count":COUNT_ALLOCATIONS.then_some(self.allocation_count),"elapsed_ns":self.elapsed_ns,"net_retained_bytes":COUNT_ALLOCATIONS.then_some(self.net_retained_bytes)})
    }
}

// Parser spill threads are joined before a phase returns. Snapshot all phases
// before constructing observation JSON so serializer storage is excluded.
fn measure<T>(action: impl FnOnce() -> T) -> (T, Phase) {
    let baseline = LIVE.load(SeqCst);
    let allocations = ALLOCATIONS.load(SeqCst);
    PEAK.store(baseline, SeqCst);
    let start = std::time::Instant::now();
    let result = action();
    let elapsed_ns = start.elapsed().as_nanos();
    let retained = LIVE.load(SeqCst);
    let peak = PEAK.load(SeqCst);
    let count = ALLOCATIONS.load(SeqCst);
    (
        result,
        Phase {
            baseline_live_bytes: baseline,
            retained_bytes: retained.saturating_sub(baseline),
            peak_bytes: peak.saturating_sub(baseline),
            allocation_count: count - allocations,
            elapsed_ns,
            net_retained_bytes: (retained as i128 - baseline as i128) as i64,
        },
    )
}

fn snapshot_report(snapshot: &ParseSnapshot) -> serde_json::Value {
    let stats = snapshot.stats();
    let storage = snapshot.storage_stats();
    let known = stats.full_parse_fallbacks == 0;
    serde_json::json!({"storage":{"representation":"shared","physical_retained_bytes":storage.physical_retained_bytes,
        "syntax_payload_bytes":storage.syntax_payload_bytes,"source_buffer_bytes":storage.source_buffer_bytes,"chunks":storage.chunks,
        "syntax_bytes":storage.syntax_bytes,"source_bytes":storage.source_bytes,"index_bytes":storage.index_bytes,"sidecar_bytes":storage.sidecar_bytes,"unique_allocations":storage.unique_allocations},
        "diagnostics":snapshot.diagnostic_count(),
        "stats":{"work":stats.work,"memo_hits":stats.memo_hits,"lexical_cache_hits":stats.lexical_cache_hits,"choice_plan_builds":stats.choice_plan_builds,"choice_skips":stats.choice_skips,"native_lexical_calls":stats.native_lexical_calls,"native_lexical_work":stats.native_lexical_work,"native_syntax_calls":stats.native_syntax_calls,"native_syntax_tails":stats.native_syntax_tails,"trivia_fast_skips":stats.trivia_fast_skips,"memo_descendants_copied":known.then_some(stats.memo_descendants_copied),
            "winner_descendants_copied":known.then_some(stats.winner_descendants_copied),"probe_descendants_copied":known.then_some(stats.probe_descendants_copied),
            "export_nodes_copied":stats.export_nodes_copied,"reused_subtrees":stats.reused_subtrees,"reparsed_bytes":stats.reparsed_bytes,
            "source_bytes_copied":known.then_some(stats.source_bytes_copied),"syntax_bytes_copied":known.then_some(stats.syntax_bytes_copied),
            "full_parse_fallbacks":stats.full_parse_fallbacks,"recovery_batches":stats.recovery_batches,"recovery_metadata_work":stats.recovery_metadata_work,"recovery_child_bytes":stats.recovery_child_bytes,"recovery_coverage_bytes":stats.recovery_coverage_bytes,"cache_evictions":stats.cache_evictions,"maintenance_passes":stats.maintenance_passes,"reclaimed_syntax_bytes":stats.reclaimed_syntax_bytes},
        "counter_scope":if known {"shared recognition; syntax copies count published child payload, not newly constructed records; reparsed bytes exclude reused recognition ranges"}else{"full legacy recovery fallback; legacy copy counts are uninstrumented and reported as null; work is the legacy recovery VM/probe count"}})
}
fn same_tree(a: &SyntaxTree, b: &SyntaxTree) -> bool {
    a.source == b.source
        && a.root == b.root
        && a.nodes.len() == b.nodes.len()
        && a.nodes
            .iter()
            .zip(&b.nodes)
            .enumerate()
            .all(|(id, (a_node, b_node))| {
                a_node.rule == b_node.rule
                    && a_node.span == b_node.span
                    && a.children(id as u32)
                        .iter()
                        .map(|e| (a.label(e), e.node))
                        .eq(b.children(id as u32).iter().map(|e| (b.label(e), e.node)))
            })
        && a.recovery()
            .map(|r| (&r.repairs, &r.coverage, &r.damaged_nodes))
            == b.recovery()
                .map(|r| (&r.repairs, &r.coverage, &r.damaged_nodes))
}
fn validate_snapshot(snapshot: &ParseSnapshot, language: &'static Language, recover: bool) -> bool {
    let Ok(flat) = snapshot.materialize() else {
        return false;
    };
    if recover {
        let full = parse_recovering(
            language,
            &flat.source,
            Limits::default(),
            RecoveryOptions::default(),
        );
        snapshot.diagnostics() == full.diagnostics && same_tree(&flat, &full.tree)
    } else {
        parse(language, &flat.source, Limits::default()).is_ok_and(|full| same_tree(&flat, &full))
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::from(2)
        }
    }
}

fn snapshot_parse(
    language: &'static Language,
    profile: &str,
    source: &str,
    mode: SnapshotMode,
    compiled: bool,
    generated: bool,
) -> Result<ParseSnapshot, Diagnostic> {
    if !compiled {
        return parse_snapshot(language, source, Limits::default(), mode);
    }
    let (lexers, parser) = match profile {
        "portable" => (&nitride_cli::LEXERS, &nitride_cli::PARSER),
        "power" => (&nitride_cli::power::LEXERS, &nitride_cli::power::PARSER),
        "composition" => (
            &nitride_cli::composition::LEXERS,
            &nitride_cli::composition::PARSER,
        ),
        "xml" => (&nitride_cli::xml::LEXERS, &nitride_cli::xml::PARSER),
        "namespaces" => (
            &nitride_cli::namespaces::LEXERS,
            &nitride_cli::namespaces::PARSER,
        ),
        "inference" => (
            &nitride_cli::inference::LEXERS,
            &nitride_cli::inference::PARSER,
        ),
        "calls" => (&nitride_cli::calls::LEXERS, &nitride_cli::calls::PARSER),
        "functions" => (
            &nitride_cli::functions::LEXERS,
            &nitride_cli::functions::PARSER,
        ),
        "dates" => (&nitride_cli::dates::LEXERS, &nitride_cli::dates::PARSER),
        "rewrites" => (
            &nitride_cli::rewrites::LEXERS,
            &nitride_cli::rewrites::PARSER,
        ),
        "csharp4" => (&nitride_cli::csharp4::LEXERS, &nitride_cli::csharp4::PARSER),
        "grammar" => (
            &nitride_cli::grammar_fixture::LEXERS,
            &nitride_cli::grammar_fixture::PARSER,
        ),
        _ => return parse_snapshot(language, source, Limits::default(), mode),
    };
    if generated {
        parse_snapshot_generated(parser, source, Limits::default(), mode)
    } else {
        parse_snapshot_compiled(lexers, source, Limits::default(), mode)
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize process-wide thread runtime storage before measuring the engine.
    // On musl, the first spill thread allocates std's stack-overflow thread-info
    // BTreeMap, whose empty root survives joining the thread. That is runtime
    // storage, not storage owned by the parse result.
    std::thread::Builder::new()
        .spawn(|| {})?
        .join()
        .map_err(|_| "profiler thread runtime initialization failed")?;
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let generated = args.first().is_some_and(|s| s == "--native-syntax");
    let compiled = generated || args.first().is_some_and(|s| s == "--native-lexers");
    if compiled {
        args.remove(0);
    }
    let compiled_allowed = matches!(
        args.first().map(String::as_str),
        Some("--edits" | "--snapshot" | "--snapshot-recover" | "--snapshot-drop")
    );
    if compiled && !compiled_allowed {
        return Err("native generation requires a snapshot command".into());
    }
    if let [flag, profile, path, edit_path] = args.as_slice() {
        if flag == "--edits" {
            let language =
                nitride_cli::recovery_language(profile).ok_or("unknown edit language")?;
            let source = std::fs::read_to_string(path)?;
            let config: serde_json::Value = serde_json::from_slice(&std::fs::read(edit_path)?)?;
            let recover = config["recover"].as_bool().unwrap_or(false);
            let validate = config["validate"].as_bool().unwrap_or(false);
            let retain = usize::try_from(config["retain"].as_u64().unwrap_or(0))?;
            if retain > 1024 {
                return Err("retained reader window exceeds 1024".into());
            }
            let edits = config["edits"]
                .as_array()
                .ok_or("edits must be an array")?
                .iter()
                .map(|e| -> Result<TextEdit, Box<dyn std::error::Error>> {
                    let start = u32::try_from(e["range"][0].as_u64().ok_or("invalid edit start")?)?;
                    let end = u32::try_from(e["range"][1].as_u64().ok_or("invalid edit end")?)?;
                    Ok(TextEdit {
                        range: [start, end],
                        replacement: e["replacement"]
                            .as_str()
                            .ok_or("invalid edit replacement")?
                            .into(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mode = if recover {
                SnapshotMode::Recovering(RecoveryOptions::default())
            } else {
                SnapshotMode::Strict(ParseOptions::default())
            };
            let (initial, cold_phase) =
                measure(|| snapshot_parse(language, profile, &source, mode, compiled, generated));
            let mut current = initial.map_err(|e| e.message)?;
            let cold = serde_json::json!({"phase":cold_phase.json(),"snapshot":snapshot_report(&current),
                    "equivalent":validate.then(||validate_snapshot(&current,language,recover))});
            let mut history = std::collections::VecDeque::with_capacity(retain + 1);
            let mut rows = Vec::new();
            for edit in edits {
                let (result, phase) = measure(|| {
                    let next = current.edit(&edit, Limits::default())?;
                    history.push_back(current);
                    if history.len() > retain {
                        history.pop_front();
                    }
                    Ok::<_, Diagnostic>((next, history))
                });
                let (next, next_history) = result.map_err(|e| e.message)?;
                current = next;
                history = next_history;
                let readers: Vec<_> = history.iter().chain(std::iter::once(&current)).collect();
                let shared = shared_storage_bytes(&readers);
                let logical: usize = readers
                    .iter()
                    .map(|s| s.storage_stats().physical_retained_bytes)
                    .sum();
                rows.push(serde_json::json!({"phase":phase.json(),"snapshot":snapshot_report(&current),"readers":readers.len(),
                    "shared_reader_bytes":shared,"logical_reader_bytes":logical,"equivalent":validate.then(||validate_snapshot(&current,language,recover))}));
            }
            println!(
                "{}",
                serde_json::json!({"language":profile,"compiled_lexers":compiled,"compiled_syntax":generated,"recover":recover,"source_bytes":source.len(),"cold":cold,"edits":rows,
                "metric":"edit phase includes old-reader window eviction; requested System bytes exclude caller input, reader container, JSON and OS/thread mappings; net retained bytes may be negative; independent peaks are not summed"})
            );
            return Ok(());
        }
    }
    if let [flag, profile, path] = args.as_slice() {
        if flag == "--snapshot-drop" {
            let language =
                nitride_cli::recovery_language(profile).ok_or("unknown snapshot language")?;
            let source = std::fs::read_to_string(path)?;
            let (result, lifecycle) = measure(|| -> Result<(), Diagnostic> {
                let original = snapshot_parse(
                    language,
                    profile,
                    &source,
                    SnapshotMode::Strict(ParseOptions::default()),
                    compiled,
                    generated,
                )?;
                let mut current = original.clone();
                let mut readers = std::collections::VecDeque::new();
                for _ in 0..1000 {
                    readers.push_back(current.clone());
                    if readers.len() > 8 {
                        readers.pop_front();
                    }
                    current = current.edit(
                        &TextEdit {
                            range: [0, 0],
                            replacement: String::new(),
                        },
                        Limits::default(),
                    )?;
                }
                drop(readers);
                drop(current);
                drop(original);
                Ok(())
            });
            result.map_err(|e| e.message)?;
            println!(
                "{}",
                serde_json::json!({"lifecycle":lifecycle.json(),"edits":1000,"retained_readers":8})
            );
            return Ok(());
        }
    }
    if let [flag, profile, path] = args.as_slice() {
        if flag == "--snapshot" || flag == "--snapshot-recover" {
            let language =
                nitride_cli::recovery_language(profile).ok_or("unknown snapshot language")?;
            let source = std::fs::read_to_string(path)?;
            let recover = flag == "--snapshot-recover";
            let mode = if recover {
                SnapshotMode::Recovering(RecoveryOptions::default())
            } else {
                SnapshotMode::Strict(ParseOptions::default())
            };
            let (snapshot, phase) =
                measure(|| snapshot_parse(language, profile, &source, mode, compiled, generated));
            let snapshot = snapshot.map_err(|e| e.message)?;
            let mut report = snapshot_report(&snapshot);
            report["language"] = profile.clone().into();
            report["compiled_lexers"] = compiled.into();
            report["compiled_syntax"] = generated.into();
            report["source_bytes"] = source.len().into();
            report["parse"] = phase.json();
            report["storage"]["engine_retained_bytes"] =
                serde_json::json!(COUNT_ALLOCATIONS.then_some(phase.retained_bytes));
            report["storage"]["export_retained_bytes"] = serde_json::Value::Null;
            report["metric"]="requested System bytes; no implicit flat export/semantic indexing in shared strict mode; recovery fallback is explicit; excludes caller input and OS/thread mappings".into();
            println!("{report}");
            return Ok(());
        }
    }

    if let [flag, profile, path] = args.as_slice() {
        if flag == "--recover" {
            let language =
                nitride_cli::recovery_language(profile).ok_or("unknown recovery language")?;
            let source = std::fs::read_to_string(path)?;
            let (parsed, phase) = measure(|| {
                parse_recovering(
                    language,
                    &source,
                    Limits::default(),
                    RecoveryOptions::default(),
                )
            });
            println!(
                "{}",
                serde_json::json!({"language":profile,"source_bytes":source.len(),"syntax_nodes":parsed.tree.nodes.len(),"syntax_capacity_bytes":parsed.tree.retained_bytes(),"storage":{"representation":"legacy-flat","engine_retained_bytes":parsed.tree.retained_bytes(),"export_retained_bytes":null,"physical_retained_bytes":COUNT_ALLOCATIONS.then_some(phase.retained_bytes)},"parse":phase.json(),"execution":null,"diagnostics":parsed.diagnostics.len(),"recovery":{"work":parsed.stats.work,"search_states":parsed.stats.search_states,"repair_count":parsed.stats.repair_count,"fallback_count":parsed.stats.fallback_count},"metric":"requested bytes through System; excludes allocator overhead, static tables and thread stack/OS mappings"})
            );
            return Ok(());
        }
    }
    let [profile, path] = args.as_slice() else {
        return Err(
            "usage: nitride-profile <portable|power|xml|composition|namespaces|inference|calls|functions|dates|rewrites|csharp4> <source>"
                .into(),
        );
    };
    let language = match profile.as_str() {
        "portable" => &nitride_cli::LANGUAGE,
        "inference" => &nitride_cli::inference::LANGUAGE,
        "calls" => &nitride_cli::calls::LANGUAGE,
        "functions" => &nitride_cli::functions::LANGUAGE,
        "dates" => &nitride_cli::dates::LANGUAGE,
        "rewrites" => &nitride_cli::rewrites::LANGUAGE,
        "csharp4" => &nitride_cli::csharp4::LANGUAGE,
        "namespaces" => &nitride_cli::namespaces::LANGUAGE,
        "power" => &nitride_cli::power::LANGUAGE,
        "xml" => &nitride_cli::xml::LANGUAGE,
        "composition" => &nitride_cli::composition::LANGUAGE,
        _ => return Err("unknown profiling language".into()),
    };
    let source = std::fs::read_to_string(path)?;
    let bindings = nitride_cli::fixture_bindings_for(language);
    // A fixed 2026-10-24 keeps `today` measurements reproducible.
    let values = nitride_cli::fixture_values_for(language, 739_912.0);
    // Large generated inputs exceed the default work budget; parsing is measured, not bounded.
    let limits = Limits {
        work: u64::MAX,
        ..Limits::default()
    };
    let (tree, parse_phase) = measure(|| parse(language, &source, limits));
    let tree = tree.map_err(|e| e.message)?;
    let mut hir_phase = None;
    let mut execution_phase = None;
    let mut program = None;
    let mut execution = None;
    if language.profile == Profile::Typed {
        let (compiled, phase) = measure(|| compile(language, &tree, Limits::default()));
        let compiled = compiled.map_err(|e| e[0].message.clone())?;
        hir_phase = Some(phase);
        let (result, phase) =
            measure(|| execute_with_values(&compiled, &bindings, &values, Limits::default()));
        execution = Some(result.map_err(|e| e.diagnostic.message)?);
        execution_phase = Some(phase);
        program = Some(compiled);
    }
    println!(
        "{}",
        serde_json::json!({"language":profile,"semantic_validation":language.profile == Profile::Typed,
        "source_bytes":source.len(),"caller_source_capacity_bytes":source.capacity(),"syntax_nodes":tree.nodes.len(),
        "syntax_capacity_bytes":tree.retained_bytes(),"storage":{"representation":"legacy-flat","engine_retained_bytes":tree.retained_bytes(),"export_retained_bytes":null,"physical_retained_bytes":COUNT_ALLOCATIONS.then_some(parse_phase.retained_bytes)},"hir_nodes":program.as_ref().map(|p| p.nodes.len()),
        "parse":parse_phase.json(),"hir":hir_phase.as_ref().map(Phase::json),"execution":execution_phase.as_ref().map(Phase::json),
        "values":execution.as_ref().map(|e| e.values.iter().map(nitride_cli::datum).collect::<Vec<_>>()),"metric":"requested bytes through System; excludes allocator overhead, static language data and OS mappings"})
    );
    Ok(())
}
