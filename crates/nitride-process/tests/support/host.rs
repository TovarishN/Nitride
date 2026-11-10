use std::io::{BufRead, Write};
fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap();
    let log = args.next().unwrap();
    let mut log = std::fs::OpenOptions::new().create(true).append(true).open(log).unwrap();
    let mut output = std::io::stdout().lock();
    for (index, line) in std::io::stdin().lock().lines().enumerate() {
        let line = line.unwrap();
        writeln!(log, "{line}").unwrap();
        log.flush().unwrap();
        let id = index + 1;
        let response = match mode.as_str() {
            "eof" => return,
            "malformed" => "{".to_owned(),
            "wrong-id" => format!(r#"{{"jsonrpc":"2.0","id":{},"result":42}}"#, id + 1),
            "version" => format!(r#"{{"jsonrpc":"1.0","id":{id},"result":42}}"#),
            "both" => format!(r#"{{"jsonrpc":"2.0","id":{id},"result":42,"error":{{"code":1,"message":"bad","data":{{"failure":"bad-input"}}}}}}"#),
            "bad-error" => format!(r#"{{"jsonrpc":"2.0","id":{id},"error":{{"message":"bad","data":{{"failure":"bad-input"}}}}}}"#),
            "failure" => format!(r#"{{"jsonrpc":"2.0","id":{id},"error":{{"code":1,"message":"remote wording","data":{{"failure":"bad-input"}}}}}}"#),
            "numeric-failure" => format!(r#"{{"jsonrpc":"2.0","id":{id},"error":{{"code":1,"message":"bad","data":{{"failure":"bad-input2"}}}}}}"#),
            "unknown-failure" => format!(r#"{{"jsonrpc":"2.0","id":{id},"error":{{"code":1,"message":"remote failure","data":{{"failure":"unknown"}}}}}}"#),
            "record" => format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{{"count":7,"tags":["é","😀"]}}}}"#),
            _ => format!(r#"{{"jsonrpc":"2.0","id":{id},"result":42}}"#),
        };
        writeln!(output, "{response}").unwrap();
        output.flush().unwrap();
    }
}
