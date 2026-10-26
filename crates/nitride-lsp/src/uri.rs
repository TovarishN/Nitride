//! `file:` URIs ⇄ paths.
use std::path::{Path, PathBuf};

pub fn to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let decoded = decode(rest)?;
    // file:///C:/x → C:/x on Windows.
    let bytes = decoded.as_bytes();
    if cfg!(windows) && bytes.len() > 2 && bytes[0] == b'/' && bytes[2] == b':' {
        return Some(PathBuf::from(&decoded[1..]));
    }
    Some(PathBuf::from(decoded))
}

pub fn from_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut uri = String::from("file://");
    if !text.starts_with('/') {
        uri.push('/');
    }
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~:".contains(&b) {
            uri.push(b as char);
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}
