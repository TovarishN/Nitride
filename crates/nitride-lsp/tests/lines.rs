use nitride_lsp::lines::LineIndex;

#[test]
fn utf16_columns_count_surrogate_pairs() {
    let index = LineIndex::new("a😀b");
    assert_eq!(index.position(5), (0, 3));
    assert_eq!(index.offset(0, 3), 5);
}

#[test]
fn crlf_lines() {
    let index = LineIndex::new("a\r\nb");
    assert_eq!(index.position(3), (1, 0));
    assert_eq!(index.offset(0, 5), 1);
    assert_eq!(index.offset(1, 0), 3);
}

#[test]
fn offset_roundtrip() {
    let text = "let é = 1;\n// 日本 😀 x\r\nend";
    let index = LineIndex::new(text);
    for (byte, _) in text.char_indices() {
        // No position lies between the \r and \n of a CRLF.
        if text[..byte].ends_with('\r') {
            continue;
        }
        let (line, column) = index.position(byte as u32);
        assert_eq!(index.offset(line, column), byte as u32, "byte {byte}");
    }
}

#[test]
fn inside_a_character_clamps_to_its_start() {
    let index = LineIndex::new("😀");
    assert_eq!(index.position(2), (0, 0));
    assert_eq!(index.offset(0, 1), 0);
}
