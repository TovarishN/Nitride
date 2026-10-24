//! Comprehension ranges are produced lazily: a large range filtered down to a
//! few elements never materializes its values.
use nitride_prepare::{Source, prepare_sources};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Peak;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Peak {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let now = CURRENT.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
        PEAK.fetch_max(now, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Peak = Peak;

#[test]
fn large_filtered_range_is_not_materialized() {
    let body = "function Few(n: Grammar.Int) : Grammar.Rule = choice i in 0 .. n { where i < 2 : \"a\" };\ntoken T = Few(2000000);\nsyntax Start = Value:T;";
    let sources = [Source {
        path: "test.ngr".into(),
        text: format!("syntax module Test {{\n{body}\n}}\n"),
    }];
    let before = CURRENT.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    prepare_sources(
        &sources,
        "{\"types\":[],\"operations\":[],\"statements\":{}}",
        "Test.Start",
        "syntax",
    )
    .unwrap();
    let grown = PEAK.load(Ordering::Relaxed) - before;
    assert!(grown < 16 * 1024 * 1024, "peak grew by {grown} bytes");
}
