use nitride_core::{Limits, compile, execute, parse};

fn main() {
    let tree = parse(
        &nitride_cli::LANGUAGE,
        "let x : Scalar = 6; x * 7;",
        Limits::default(),
    )
    .unwrap();
    let program = compile(&nitride_cli::LANGUAGE, &tree, Limits::default()).unwrap();
    let result = execute(
        &program,
        &nitride_cli::fixture_bindings(),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(result.values, [42.0]);
    println!("{}", result.values[0]);
}
