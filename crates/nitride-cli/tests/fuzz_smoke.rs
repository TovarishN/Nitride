#[path = "../../../fuzz/support/incremental.rs"]
mod incremental;

#[test]
fn fuzz_corpus_exercises_strict_recovering_generated_and_retained_readers() {
    let mut configurations = 0_u16;
    for entry in std::fs::read_dir(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fuzz/corpus/incremental_edits"
    ))
    .unwrap()
    {
        let entry = entry.unwrap();
        let bytes = std::fs::read(entry.path()).unwrap();
        configurations |= 1 << (bytes[0] & 15);
        assert!(incremental::exercise(&bytes) >= 2, "{:?}", entry.path());
    }
    assert_eq!(
        configurations,
        u16::MAX,
        "corpus must cover all sixteen configurations"
    );
    for bytes in [&[][..], &[0], &[0, 0], &[255, 255, 255]] {
        incremental::exercise(bytes);
    }
}
