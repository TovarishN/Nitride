//! `Content-Length` framed JSON-RPC messages.
use serde_json::Value;
use std::io::{self, BufRead, Write};

/// Reads one message. `None` at end of input; `Some(Err)` for a frame whose
/// body is not JSON, after which the stream stays usable.
pub fn read(input: &mut impl BufRead) -> io::Result<Option<Result<Value, String>>> {
    let mut length = None;
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let header = line.trim_end();
        if header.is_empty() {
            if length.is_some() {
                break;
            }
            continue;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                length = value.trim().parse::<usize>().ok();
            }
        }
    }
    let mut body = vec![0; length.unwrap_or(0)];
    input.read_exact(&mut body)?;
    Ok(Some(
        serde_json::from_slice(&body).map_err(|e| e.to_string()),
    ))
}

pub fn write(output: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = message.to_string();
    write!(output, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    output.flush()
}
