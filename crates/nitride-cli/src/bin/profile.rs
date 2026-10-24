use nitride_core::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

struct CountingSystem;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

fn added(bytes: usize) {
    let live = LIVE.fetch_add(bytes, SeqCst) + bytes;
    PEAK.fetch_max(live, SeqCst);
    ALLOCATIONS.fetch_add(1, SeqCst);
}

// Instrument the unchanged System allocator in this executable only. No allocation,
// formatting or locks occur in these hooks. Only successful allocations are counted.
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

#[global_allocator]
static ALLOCATOR: CountingSystem = CountingSystem;

struct Phase {
    baseline_live_bytes: usize,
    retained_bytes: usize,
    peak_bytes: usize,
    allocation_count: usize,
    elapsed_ns: u128,
}

impl Phase {
    fn json(&self) -> serde_json::Value {
        serde_json::json!({"baseline_live_bytes":self.baseline_live_bytes,"retained_bytes":self.retained_bytes,
            "peak_bytes":self.peak_bytes,"allocation_count":self.allocation_count,"elapsed_ns":self.elapsed_ns})
    }
}

// The fixture hosts are synchronous and do not create threads. Snapshot all phases
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
        },
    )
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

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [profile, path] = args.as_slice() else {
        return Err(
            "usage: nitride-profile <portable|power|xml|composition|namespaces|inference> <source>"
                .into(),
        );
    };
    let language = match profile.as_str() {
        "portable" => &nitride_cli::LANGUAGE,
        "inference" => &nitride_cli::inference::LANGUAGE,
        "namespaces" => &nitride_cli::namespaces::LANGUAGE,
        "power" => &nitride_cli::power::LANGUAGE,
        "xml" => &nitride_cli::xml::LANGUAGE,
        "composition" => &nitride_cli::composition::LANGUAGE,
        _ => return Err("unknown profiling language".into()),
    };
    let source = std::fs::read_to_string(path)?;
    let bindings = nitride_cli::fixture_bindings_for(language);
    let (tree, parse_phase) = measure(|| parse(language, &source, Limits::default()));
    let tree = tree.map_err(|e| e.message)?;
    let mut hir_phase = None;
    let mut execution_phase = None;
    let mut program = None;
    let mut execution = None;
    if language.profile == Profile::Typed {
        let (compiled, phase) = measure(|| compile(language, &tree, Limits::default()));
        let compiled = compiled.map_err(|e| e[0].message.clone())?;
        hir_phase = Some(phase);
        let (result, phase) = measure(|| execute(&compiled, &bindings, Limits::default()));
        execution = Some(result.map_err(|e| e.diagnostic.message)?);
        execution_phase = Some(phase);
        program = Some(compiled);
    }
    println!(
        "{}",
        serde_json::json!({"language":profile,"semantic_validation":language.profile == Profile::Typed,
        "source_bytes":source.len(),"caller_source_capacity_bytes":source.capacity(),"syntax_nodes":tree.nodes.len(),
        "syntax_capacity_bytes":tree.retained_bytes(),"hir_nodes":program.as_ref().map(|p| p.nodes.len()),
        "parse":parse_phase.json(),"hir":hir_phase.as_ref().map(Phase::json),"execution":execution_phase.as_ref().map(Phase::json),
        "values":execution.as_ref().map(|e| &e.values),"metric":"requested bytes through System; excludes allocator overhead, static language data and OS mappings"})
    );
    Ok(())
}
