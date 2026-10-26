mod support;
use serde_json::{Value, json};
use support::*;

#[test]
fn initialize_shutdown_exit() {
    let messages = Session::run(&scratch("lifecycle"), vec![]);
    let init = response(&messages, 1);
    assert_eq!(init["result"]["serverInfo"]["name"], "nitride");
    assert_eq!(init["result"]["capabilities"]["textDocumentSync"], 1);
    assert_eq!(init["result"]["capabilities"]["positionEncoding"], "utf-16");
    assert_eq!(response(&messages, 9999)["result"], Value::Null);
}

#[test]
fn unknown_request_is_method_not_found() {
    let messages = Session::run(
        &scratch("unknown"),
        vec![request(7, "nitride/nothing", json!({}))],
    );
    assert_eq!(response(&messages, 7)["error"]["code"], -32601);
}

#[test]
fn malformed_frame_is_parse_error() {
    let root = scratch("malformed");
    let mut input = frame(&initialize(1, &root));
    let bad = b"{not json";
    input.extend(format!("Content-Length: {}\r\n\r\n", bad.len()).into_bytes());
    input.extend(bad);
    input.extend(frame(&request(2, "shutdown", Value::Null)));
    input.extend(frame(&notification("exit", Value::Null)));
    let messages = Session::raw(input);
    assert!(messages.iter().any(|m| m["error"]["code"] == -32700));
    assert_eq!(response(&messages, 2)["result"], Value::Null);
}
