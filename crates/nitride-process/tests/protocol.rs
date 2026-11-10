use nitride_core::*;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

static LANGUAGE: Language<'static> = Language {
    trivia: TriviaPolicy::Nitrogen,
    profile: Profile::SyntaxOnly,
    rules: &[],
    start: 0,
    types: &[],
    operations: &[
        Signature {
            id: "Test.Number",
            inputs: &[],
            result: "Core.Scalar",
        },
        Signature {
            id: "Test.Record",
            inputs: &["Test.Record"],
            result: "Core.Scalar",
        },
        Signature {
            id: "Test.Read",
            inputs: &[],
            result: "Test.Record",
        },
        Signature {
            id: "Test.Primitives",
            inputs: &["Core.Scalar", "Test.Count", "Test.Flag", "Test.Tag"],
            result: "Core.Scalar",
        },
        Signature {
            id: "Test.Text",
            inputs: &["Core.Text"],
            result: "Core.Scalar",
        },
    ],
    handlers: &["Test.API@fixture"; 5],
    checks: &[],
    helpers: &[],
    rule_checks: &[],
    patterns: &[],
    source_hash: "protocol-test",
};

struct Fixture {
    root: PathBuf,
    binary: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "nitride-process-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let binary = root.join(if cfg!(windows) { "host.exe" } else { "host" });
        let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let result = std::process::Command::new(compiler)
            .args(["--edition=2024", "--crate-name", "nitride_protocol_fixture"])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/host.rs"))
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        Self { root, binary }
    }
    fn binding(&self, mode: &str, op: &str) -> HostBinding<'static> {
        let prepared = json!({"catalog":{
            "representations":{"Test.Record":{"record":[["count","Test.Count"],["tags","Test.Tags"]]},"Test.Count":"i64","Test.Tags":{"list":"Test.Tag"},"Test.Tag":"text","Test.Flag":"bool"},
            "operations":[{"id":op,"fails":["Test.BadInput","Test.Bad__Input_2"]}],
            "failures":{"Test.BadInput":"declared bad input","Test.Bad__Input_2":"declared numeric suffix"}
        }});
        let binding = json!({"program":self.binary,"args":[mode,self.root.join(format!("{mode}.log"))],"contract":"Test.API@fixture","exports":{op:"%invoke"}});
        nitride_process::bindings(&LANGUAGE, &prepared, &binding, &self.root)
            .unwrap()
            .remove(0)
    }
    fn requests(&self, mode: &str) -> Vec<Value> {
        let text =
            std::fs::read_to_string(self.root.join(format!("{mode}.log"))).unwrap_or_default();
        text.lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn requests_keep_ids_methods_unicode_text_and_record_shapes() {
    let fixture = Fixture::new();
    {
        let binding = fixture.binding("valid", "Test.Record");
        let value = Datum::Record(
            vec![
                Datum::Integer(7),
                Datum::List(vec![Datum::Text("é".into()), Datum::Text("😀".into())].into()),
            ]
            .into(),
        );
        for _ in 0..2 {
            assert_eq!(
                binding
                    .call(std::slice::from_ref(&value), None, &mut Vec::new())
                    .unwrap(),
                42.0
            );
        }
    }
    let requests = fixture.requests("valid");
    assert_eq!(requests.len(), 2);
    for (index, request) in requests.iter().enumerate() {
        assert_eq!(
            request,
            &json!({"jsonrpc":"2.0","id":index+1,"method":"invoke","params":[{"count":7,"tags":["é","😀"]}]})
        );
    }
    {
        let binding = fixture.binding("text", "Test.Text");
        assert_eq!(
            binding.call(&[], Some("é\n😀"), &mut Vec::new()).unwrap(),
            42.0
        );
    }
    assert_eq!(fixture.requests("text")[0]["params"], json!(["é\n😀"]));
    {
        let binding = fixture.binding("primitives", "Test.Primitives");
        let values = [
            Datum::Number(2.5),
            Datum::Integer(-7),
            Datum::Boolean(true),
            Datum::Text("é😀".into()),
        ];
        assert_eq!(binding.call(&values, None, &mut Vec::new()).unwrap(), 42.0);
    }
    assert_eq!(
        fixture.requests("primitives")[0]["params"],
        json!([2.5, -7, true, "é😀"])
    );
}

#[test]
fn malformed_replies_are_host_failures_not_declared_failures() {
    let fixture = Fixture::new();
    for mode in [
        "eof",
        "malformed",
        "wrong-id",
        "version",
        "both",
        "bad-error",
        "unknown-failure",
    ] {
        let binding = fixture.binding(mode, "Test.Number");
        let error = binding.call(&[], None, &mut Vec::new()).expect_err(mode);
        assert_eq!(error.failure.as_deref(), Some("host.process"), "{mode}");
    }
    let binding = fixture.binding("failure", "Test.Number");
    let error = binding.call(&[], None, &mut Vec::new()).unwrap_err();
    assert_eq!(error.failure.as_deref(), Some("Test.BadInput"));
    assert_eq!(error.message, "declared bad input");
}

#[test]
fn nested_reply_values_keep_the_declared_representations() {
    let fixture = Fixture::new();
    let binding = fixture.binding("record", "Test.Read");
    assert_eq!(
        binding.call(&[], None, &mut Vec::new()).unwrap(),
        Datum::Record(
            vec![
                Datum::Integer(7),
                Datum::List(vec![Datum::Text("é".into()), Datum::Text("😀".into())].into())
            ]
            .into()
        )
    );
}

#[test]
fn incomplete_outgoing_records_fail_before_a_request_is_sent() {
    let fixture = Fixture::new();
    let binding = fixture.binding("valid", "Test.Record");
    for values in [
        vec![],
        vec![Datum::Record(Vec::new().into())],
        vec![Datum::Record(vec![Datum::Integer(7)].into())],
        vec![Datum::Number(f64::NAN)],
        vec![Datum::Boolean(true)],
    ] {
        let error = binding.call(&values, None, &mut Vec::new()).unwrap_err();
        assert_eq!(error.failure.as_deref(), Some("host.process"));
    }
    assert!(fixture.requests("valid").is_empty());
}

#[test]
fn failure_names_match_generated_wit_numeric_suffixes() {
    let fixture = Fixture::new();
    let binding = fixture.binding("numeric-failure", "Test.Number");
    let error = binding.call(&[], None, &mut Vec::new()).unwrap_err();
    assert_eq!(error.failure.as_deref(), Some("Test.Bad__Input_2"));
    assert_eq!(error.message, "declared numeric suffix");
}
