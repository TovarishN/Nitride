use serde_json::Value;

/// Nitride's CSharp4 trees equal actual Nitrogen's on valid C#; both reject the same inputs.
#[test]
fn csharp4_syntax_matches_real_nitrogen() {
    let cases: Value =
        serde_json::from_str(include_str!("../../../fixtures/csharp4-reference.json")).unwrap();
    let mut mismatches = Vec::new();
    for case in cases.as_array().unwrap() {
        let actual = nitride_cli::syntax_observe(
            &nitride_cli::csharp4::LANGUAGE,
            case["source"].as_str().unwrap(),
        );
        let expected = &case["observation"];
        let same = if expected["ok"] == true && actual["ok"] == true {
            actual["nodes"] == expected["nodes"]
        } else {
            actual["ok"] == expected["ok"]
        };
        if !same {
            mismatches.push(case["name"].as_str().unwrap().to_owned());
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mismatches: {mismatches:?}",
        mismatches.len()
    );
}

/// Deterministic C# of roughly `bytes` bytes: classes of methods, properties,
/// lambdas, queries and nested statements, as the corpus uses them.
pub fn generated_csharp(bytes: usize) -> String {
    let mut source = String::from("using System;\nusing System.Linq;\n");
    let mut index = 0;
    while source.len() < bytes {
        source.push_str(&format!(
            "namespace N{index} {{ public class C{index}<T> where T : class {{\n\
             int f{index} = {index};\n\
             public string Name {{ get {{ return \"n{index}\"; }} set {{ }} }}\n\
             int F(int x) {{ if (x > {index}) {{ return x * 2 + f{index}; }} else {{ for (int i = 0; i < x; i++) {{ x += i; }} return x; }} }}\n\
             void Q() {{ var xs = from a in new[] {{ {index}, 2, 3 }} where a > 1 select a * 2; Func<int, int> g = y => y + {index}; g(1); }}\n\
             }} }}\n"
        ));
        index += 1;
    }
    source
}

fn parse_seconds(source: &str) -> f64 {
    let start = std::time::Instant::now();
    let tree = nitride_core::parse(
        &nitride_cli::csharp4::LANGUAGE,
        source,
        nitride_core::Limits {
            work: u64::MAX,
            ..nitride_core::Limits::default()
        },
    )
    .unwrap();
    std::hint::black_box(tree.nodes.len());
    start.elapsed().as_secs_f64()
}

#[test]
#[ignore = "release-mode scaling measurement"]
fn csharp4_parse_scales_linearly() {
    let small = generated_csharp(256 * 1024);
    let large = generated_csharp(4 * 1024 * 1024);
    let best = |s: &str| (0..3).map(|_| parse_seconds(s)).fold(f64::MAX, f64::min);
    let (small_rate, large_rate) = (
        best(&small) / small.len() as f64,
        best(&large) / large.len() as f64,
    );
    assert!(
        large_rate <= 2.0 * small_rate,
        "time per byte grew from {small_rate:e} to {large_rate:e}"
    );
    let deep = format!(
        "class C {{ int F() {{ return {}1{}; }} }}",
        "(".repeat(300),
        ")".repeat(300)
    );
    // Nesting that parses (300 levels) stays cheap.
    assert!(parse_seconds(&deep) < 1.0);
}

#[test]
fn tokens_are_leaves_unless_requested() {
    let source = "class Abcdef { int ghijkl; }";
    let tree = nitride_core::parse(
        &nitride_cli::csharp4::LANGUAGE,
        source,
        nitride_core::Limits::default(),
    )
    .unwrap();
    let rules = nitride_cli::csharp4::LANGUAGE.rules;
    for (id, node) in tree.nodes.iter().enumerate() {
        if rules[node.rule as usize].lexical {
            assert!(
                tree.children(id as u32).is_empty(),
                "token {} has children",
                rules[node.rule as usize].name
            );
        }
    }
    let detailed = nitride_core::parse_with_options(
        &nitride_cli::csharp4::LANGUAGE,
        source,
        nitride_core::Limits::default(),
        nitride_core::ParseOptions {
            token_children: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(detailed.nodes.len() > tree.nodes.len());
}

#[test]
fn default_limits_parse_large_files() {
    let source = generated_csharp(256 * 1024);
    let observation = nitride_cli::syntax_observe(&nitride_cli::csharp4::LANGUAGE, &source);
    assert_eq!(observation["ok"], true, "{observation}");
}

fn parses_with_default_limits(source: &str) -> bool {
    nitride_core::parse(
        &nitride_cli::csharp4::LANGUAGE,
        source,
        nitride_core::Limits::default(),
    )
    .is_ok()
}

#[test]
fn nested_expressions_parse_in_linear_time() {
    let start = std::time::Instant::now();
    for depth in [8, 20] {
        let open = "(".repeat(depth);
        let close = ")".repeat(depth);
        for body in [
            format!("return {open}1{close};"),
            format!("return {}1{};", "(int)(".repeat(depth), close),
            format!(
                "System.Func<int, int> f = {}x{};",
                "x => (".repeat(depth),
                close
            ),
            format!("return {}a ? b : c{};", "(a ? b : ".repeat(depth), close),
            format!("return {}0{};", "a[".repeat(depth), "]".repeat(depth)),
            format!(
                "object v = new {}int{}();",
                "G<".repeat(depth),
                ">".repeat(depth)
            ),
        ] {
            let source = format!("class C {{ object M(int x) {{ {body} }} }}");
            assert!(parses_with_default_limits(&source), "{source}");
        }
    }
    assert!(
        start.elapsed().as_secs() < 20,
        "nested parsing took {:?}",
        start.elapsed()
    );
}

#[test]
fn long_else_if_chains_and_deep_blocks_parse() {
    let chain: String = (0..1000)
        .map(|i| format!("if (x == {i}) return {i}; else "))
        .collect();
    let blocks = format!("{}x++;{}", "{ ".repeat(100), " }".repeat(100));
    let classes = format!("{}{}", "class C { ".repeat(50), "}".repeat(50));
    for source in [
        format!("class C {{ int M(int x) {{ {chain}return -1; }} }}"),
        format!("class C {{ void M(int x) {{ {blocks} }} }}"),
        classes,
    ] {
        assert!(parses_with_default_limits(&source), "{}", &source[..80]);
    }
}

/// Deep input on a small (2 MiB) thread never overflows: the measured stack
/// budget spills to a large-stack thread, and without a spill it is a `limit`.
#[test]
fn deep_csharp_is_stack_safe_on_small_threads() {
    let chain: String = (0..2000)
        .map(|i| format!("if (x == {i}) return {i}; else "))
        .collect();
    let chain = format!("class C {{ int M(int x) {{ {chain}return -1; }} }}");
    let deep = format!(
        "class C {{ int F() {{ return {}1{}; }} }}",
        "(".repeat(20_000),
        ")".repeat(20_000)
    );
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            let language = &nitride_cli::csharp4::LANGUAGE;
            assert!(nitride_core::parse(language, &chain, nitride_core::Limits::default()).is_ok());
            let no_spill = nitride_core::Limits {
                parser_spill_stack_bytes: 0,
                ..nitride_core::Limits::default()
            };
            assert_eq!(
                nitride_core::parse(language, &chain, no_spill)
                    .unwrap_err()
                    .category,
                "limit"
            );
            let small_spill = nitride_core::Limits {
                parser_spill_stack_bytes: 8 * 1024 * 1024,
                ..nitride_core::Limits::default()
            };
            assert_eq!(
                nitride_core::parse(language, &deep, small_spill)
                    .unwrap_err()
                    .category,
                "limit"
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
