use nitride_core::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
};
struct Allocator;
static ENABLED: AtomicBool = AtomicBool::new(false);
static MAX: AtomicUsize = AtomicUsize::new(0);
static TOTAL: AtomicUsize = AtomicUsize::new(0);
/// The counters are process-wide, so tests that read them run one at a time.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn allocated(size: usize) {
    if ENABLED.load(Relaxed) {
        MAX.fetch_max(size, Relaxed);
        TOTAL.fetch_add(size, Relaxed);
    }
}
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        allocated(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        allocated(size);
        unsafe { System.realloc(pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
fn rule(name: &'static str, expr: Expr<'static>, lexical: bool) -> Rule<'static> {
    Rule {
        name,
        expr,
        lexical,
        precedence: 0,
        owner: None,
        except: &[],
        right_associative: false,
        transparent: false,
        alias: false,
        origin: None,
        action: Action::None,
    }
}
#[test]
fn local_edit_does_not_allocate_a_temporary_wide_child_list() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for variant in 0..3 {
        let root = match variant {
            0 => Expr::Label(
                "Items",
                &Expr::Repeat {
                    inner: &Expr::Ref(1),
                    min: 1,
                    max: u32::MAX,
                },
            ),
            1 => Expr::Sequence(&[Expr::Literal("("), Expr::Ref(3), Expr::Literal(")")]),
            _ => Expr::Repeat {
                inner: &Expr::Label("Items", &Expr::Ref(1)),
                min: 1,
                max: u32::MAX,
            },
        };
        let language = Box::leak(Box::new(Language {trivia: crate::TriviaPolicy::Nitrogen,
            profile: Profile::SyntaxOnly,
            start: 0,
            types: &[],
            operations: &[],
            handlers: &[],
            checks: &[],
            helpers: &[],
            rule_checks: &[],
            patterns: &[],
            source_hash: "wide-allocations",
            rules: Box::leak(
                vec![
                    rule("Root", root, false),
                    rule(
                        "Item",
                        Expr::Sequence(&[Expr::Ref(2), Expr::Literal(";")]),
                        false,
                    ),
                    rule("Number", Expr::Class(&[('0', '9')]), true),
                    rule(
                        "Nested",
                        Expr::Label(
                            "Items",
                            &Expr::Repeat {
                                inner: &Expr::Ref(1),
                                min: 1,
                                max: u32::MAX,
                            },
                        ),
                        false,
                    ),
                ]
                .into_boxed_slice(),
            ),
        }));
        let items = "1;".repeat(32768);
        let source = if variant == 1 {
            format!("({items})")
        } else {
            items
        };
        let old = parse_snapshot(
            language,
            &source,
            Limits::default(),
            SnapshotMode::Strict(ParseOptions::default()),
        )
        .unwrap();
        let offset = u32::from(variant == 1);
        for (case, edit) in [
            TextEdit {
                range: [offset, offset + 1],
                replacement: "2".into(),
            },
            TextEdit {
                range: [offset + 2, offset + 2],
                replacement: "3;".into(),
            },
            TextEdit {
                range: [offset, offset + 4],
                replacement: String::new(),
            },
        ]
        .iter()
        .enumerate()
        {
            MAX.store(0, Relaxed);
            TOTAL.store(0, Relaxed);
            ENABLED.store(true, Relaxed);
            let result = old.edit(edit, Limits::default());
            ENABLED.store(false, Relaxed);
            let new = result.unwrap();
            assert!(new.stats().reused_subtrees >= 32764);
            assert!(
                MAX.load(Relaxed) < 64 * 1024,
                "variant {variant} case {case} temporary allocation: {} bytes",
                MAX.load(Relaxed)
            );
            assert!(
                TOTAL.load(Relaxed) < 256 * 1024,
                "variant {variant} case {case} total local-edit allocation: {} bytes",
                TOTAL.load(Relaxed)
            );
            println!(
                "wide variant={variant} case={case} max={} total={} copied={}",
                MAX.load(Relaxed),
                TOTAL.load(Relaxed),
                new.stats().syntax_bytes_copied
            );
            let flat = new.materialize().unwrap();
            let full = parse(language, &flat.source, Limits::default()).unwrap();
            assert!(
                flat.nodes
                    .iter()
                    .map(|n| (n.rule, n.span, n.child_count))
                    .eq(full.nodes.iter().map(|n| (n.rule, n.span, n.child_count)))
            );
            for id in 0..flat.nodes.len() as u32 {
                assert!(
                    flat.children(id)
                        .iter()
                        .map(|e| (flat.label(e), e.node))
                        .eq(full.children(id).iter().map(|e| (full.label(e), e.node)))
                );
            }
        }
        if variant == 0 {
            let mut damaged = source.clone();
            damaged.remove(damaged.len() / 2 + 1);
            let recovered = parse_snapshot(
                language,
                &damaged,
                Limits::default(),
                SnapshotMode::Recovering(RecoveryOptions::default()),
            )
            .unwrap();
            let edited = recovered
                .edit(
                    &TextEdit {
                        range: [0, 1],
                        replacement: "2".into(),
                    },
                    Limits::default(),
                )
                .unwrap();
            assert_eq!(edited.stats().full_parse_fallbacks, 0);
            assert!(
                edited.stats().recovery_coverage_bytes < 4096,
                "distant unchanged damage forced {} coverage bytes",
                edited.stats().recovery_coverage_bytes
            );
            assert!(
                edited.stats().recovery_child_bytes < 4096,
                "recovery child scratch bytes: {}",
                edited.stats().recovery_child_bytes
            );
            assert!(
                edited.stats().syntax_bytes_copied < 4096,
                "separated dirty regions copied {} bytes",
                edited.stats().syntax_bytes_copied
            );
            println!(
                "recovery scratch={} copied={}",
                edited.stats().recovery_child_bytes,
                edited.stats().syntax_bytes_copied
            );
            let full = parse_snapshot(
                language,
                &edited.materialize().unwrap().source,
                Limits::default(),
                SnapshotMode::Recovering(RecoveryOptions::default()),
            )
            .unwrap();
            assert_eq!(
                edited.recovery().unwrap().repairs(),
                full.recovery().unwrap().repairs()
            );
            assert_eq!(
                edited.recovery().unwrap().coverage().collect::<Vec<_>>(),
                full.recovery().unwrap().coverage().collect::<Vec<_>>()
            );
            assert!(
                edited
                    .materialize()
                    .unwrap()
                    .nodes
                    .iter()
                    .map(|n| (n.rule, n.span, n.child_count))
                    .eq(full.materialize().unwrap().nodes.iter().map(|n| (
                        n.rule,
                        n.span,
                        n.child_count
                    )))
            );
        }
        assert_eq!(old.materialize().unwrap().source, source);
    }
}
#[test]
fn cold_parse_sorts_facts_without_a_per_byte_table() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let language = Box::leak(Box::new(Language {trivia: crate::TriviaPolicy::Nitrogen,
        profile: Profile::SyntaxOnly,
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        patterns: &[],
        source_hash: "sparse-facts",
        rules: Box::leak(
            vec![
                rule(
                    "Root",
                    Expr::Repeat {
                        inner: &Expr::Ref(1),
                        min: 1,
                        max: u32::MAX,
                    },
                    false,
                ),
                rule(
                    "Item",
                    Expr::Sequence(&[Expr::Ref(2), Expr::Literal(";")]),
                    false,
                ),
                rule("Number", Expr::Class(&[('0', '9')]), true),
            ]
            .into_boxed_slice(),
        ),
    }));
    // Few facts across a megabyte of trivia: sorting them must not allocate
    // a table as large as the text.
    let source = format!("1;{}2;", " ".repeat(1 << 20));
    MAX.store(0, Relaxed);
    ENABLED.store(true, Relaxed);
    let snapshot = parse_snapshot(
        language,
        &source,
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    );
    ENABLED.store(false, Relaxed);
    snapshot.unwrap();
    assert!(
        MAX.load(Relaxed) <= 2 * source.len(),
        "largest allocation {} bytes for {} bytes of source",
        MAX.load(Relaxed),
        source.len()
    );
}
#[test]
fn edit_near_the_start_of_a_large_file_allocates_for_the_edit() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let language = Box::leak(Box::new(Language {trivia: crate::TriviaPolicy::Nitrogen,
        profile: Profile::SyntaxOnly,
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        rule_checks: &[],
        patterns: &[],
        source_hash: "paste-near-start",
        rules: Box::leak(
            vec![
                rule(
                    "Root",
                    Expr::Repeat {
                        inner: &Expr::Ref(1),
                        min: 1,
                        max: u32::MAX,
                    },
                    false,
                ),
                rule(
                    "Item",
                    Expr::Sequence(&[Expr::Ref(2), Expr::Literal(";")]),
                    false,
                ),
                rule("Number", Expr::Class(&[('0', '9')]), true),
            ]
            .into_boxed_slice(),
        ),
    }));
    let source = "1;".repeat(1 << 19);
    let old = parse_snapshot(
        language,
        &source,
        Limits::default(),
        SnapshotMode::Strict(ParseOptions::default()),
    )
    .unwrap();
    // A few kilobytes pasted past the first 4 KiB of a megabyte file: the
    // reparse must not size its fact log as if the whole prefix were parsed.
    let edit = TextEdit {
        range: [6000, 6000],
        replacement: "3;".repeat(3000),
    };
    TOTAL.store(0, Relaxed);
    ENABLED.store(true, Relaxed);
    let result = old.edit(&edit, Limits::default());
    ENABLED.store(false, Relaxed);
    result.unwrap();
    assert!(
        // The edit itself costs about 23 MiB here (source and index copies);
        // sizing the log from the absolute position added 13 MiB more.
        TOTAL.load(Relaxed) < 28 << 20,
        "edit allocated {} bytes",
        TOTAL.load(Relaxed)
    );
}
