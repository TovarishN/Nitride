pub mod functions {
    include!(concat!(env!("OUT_DIR"), "/functions.rs"));
}
pub mod csharp4 {
    include!(concat!(env!("OUT_DIR"), "/csharp4.rs"));
}
pub mod rewrites {
    include!(concat!(env!("OUT_DIR"), "/rewrites.rs"));
}
pub mod dates {
    include!(concat!(env!("OUT_DIR"), "/dates.rs"));
}
pub mod calendar {
    include!(concat!(env!("OUT_DIR"), "/calendar.rs"));
    include!(concat!(env!("OUT_DIR"), "/calendar_interfaces.rs"));

    /// The fixture's `Gregorian`: proleptic Gregorian day numbers from
    /// 0001-01-01 through 9999-12-31.
    pub struct FixtureCalendar;

    use gregorian::GregorianFailure as Failure;

    /// 9999-12-31.
    const LAST: i64 = 3_652_058;
    /// 0001-01-01 was a Monday.
    const WEEKDAYS: [&str; 7] = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];

    /// `date` moved by a whole number of days.
    fn shift(date: i64, days: f64) -> Result<i64, Failure> {
        if days.fract() != 0.0 {
            return Err(Failure::PartialDay);
        }
        let day = date + days as i64;
        if (0..=LAST).contains(&day) {
            Ok(day)
        } else {
            Err(Failure::OutOfRange)
        }
    }

    /// The year a day number falls in.
    fn year(date: i64) -> i64 {
        let start = |y: i64| nitride_core::iso_date_day_number(&format!("{y:04}-01-01")).unwrap();
        let (mut low, mut high) = (1, 9999);
        while low < high {
            let mid = (low + high + 1) / 2;
            if start(mid) <= date {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        low
    }

    impl gregorian::Gregorian for FixtureCalendar {
        fn parse_date(&self, text: &str) -> Result<i64, Failure> {
            nitride_core::iso_date_day_number(text).ok_or(Failure::InvalidDate)
        }
        fn later(&self, date: i64, duration: f64) -> Result<i64, Failure> {
            shift(date, duration)
        }
        fn earlier(&self, date: i64, duration: f64) -> Result<i64, Failure> {
            shift(date, -duration)
        }
        fn between(&self, date_1: i64, date_2: i64) -> Result<f64, Failure> {
            Ok((date_1 - date_2) as f64)
        }
        fn day_of_week(&self, date: i64) -> Result<String, Failure> {
            Ok(WEEKDAYS[date.rem_euclid(7) as usize].into())
        }
        fn is_leap_year(&self, date: i64) -> Result<bool, Failure> {
            let y = year(date);
            Ok(y % 4 == 0 && (y % 100 != 0 || y % 400 == 0))
        }
        fn mondays(&self, span: gregorian::Span) -> Result<Vec<i64>, Failure> {
            if span.start < 0 || span.end > LAST {
                return Err(Failure::OutOfRange);
            }
            // Day numbers that are multiples of 7 are Mondays.
            let first = span.start + (7 - span.start.rem_euclid(7)) % 7;
            Ok((first..=span.end).step_by(7).collect())
        }
    }
}
pub mod calls {
    include!(concat!(env!("OUT_DIR"), "/calls.rs"));
}
include!(concat!(env!("OUT_DIR"), "/language.rs"));
pub mod inference {
    include!(concat!(env!("OUT_DIR"), "/inference.rs"));
}
pub mod namespaces {
    include!(concat!(env!("OUT_DIR"), "/namespaces.rs"));
}
pub mod xml {
    include!(concat!(env!("OUT_DIR"), "/xml.rs"));
}
pub mod composition {
    include!(concat!(env!("OUT_DIR"), "/composition.rs"));
}
pub mod power {
    include!(concat!(env!("OUT_DIR"), "/power.rs"));
}

use nitride_core::*;
use serde_json::{Value, json};
mod recovery;
pub use recovery::{recovery_language, recovery_observe};

/// The compiled-in language of a `languages/<profile>` project.
pub fn static_language(profile: &str) -> Option<&'static Language<'static>> {
    recovery_language(profile)
}
pub mod grammar_fixture {
    include!(concat!(env!("OUT_DIR"), "/grammar.rs"));
}

pub fn syntax_observe(language: &'static Language<'static>, source: &str) -> Value {
    let tree = match parse(language, source, Limits::default()) {
        Ok(tree) => tree,
        Err(error) => {
            return json!({"ok":false,"error":error.category,"span":error.span,"semantic_validation":false});
        }
    };
    fn alias(expr: &Expr) -> bool {
        match expr {
            Expr::Ref(_) => true,
            Expr::Label(_, inner) => alias(inner),
            Expr::Choice(items) => !items.is_empty() && items.iter().all(alias),
            _ => false,
        }
    }
    let mut nodes = Vec::new();
    let mut stack = vec![(tree.root, None::<usize>)];
    while let Some((id, parent)) = stack.pop() {
        let node = tree.nodes[id as usize];
        let rule = &language.rules[node.rule as usize];
        // Nitrogen (EmitModel.IsAlias): a plain syntax rule of literals and
        // references, or a choice of them, has no node; neither does a
        // transparent alternative. Alternatives and tokens keep theirs.
        fn literals_and_references(expr: &Expr) -> bool {
            match expr {
                Expr::Ref(_) | Expr::Literal(_) => true,
                Expr::Choice(items) => items
                    .iter()
                    .all(|i| matches!(i, Expr::Ref(_) | Expr::Literal(_))),
                _ => false,
            }
        }
        let plain_alias =
            rule.owner.is_none() && !rule.lexical && literals_and_references(&rule.expr);
        let next_parent = if !alias(&rule.expr) && !plain_alias && !rule.transparent {
            let index = nodes.len();
            nodes.push(json!({"rule":rule.name,"span":node.span,"parent":parent}));
            Some(index)
        } else {
            parent
        };
        // Tokens are leaves in Nitrogen: rules matched inside a token have no nodes.
        if rule.lexical {
            continue;
        }
        stack.extend(
            tree.children(id)
                .iter()
                .rev()
                .map(|edge| (edge.node, next_parent)),
        );
    }
    json!({"ok":true,"profile":"syntax-only","semantic_validation":false,"nodes":nodes})
}
pub fn observe_power(source: &str) -> Value {
    observe_language(&power::LANGUAGE, source)
}

pub fn fixture_bindings() -> Vec<HostBinding<'static>> {
    fixture_bindings_for(&LANGUAGE)
}

pub fn fixture_bindings_for(language: &'static Language<'static>) -> Vec<HostBinding<'static>> {
    host::bindings(language)
}

/// Days since 0001-01-01 for the current UTC date; the fixture has no time-zone policy.
pub fn utc_today() -> f64 {
    host::utc_today()
}

/// The fixture host's built-in values for `language`, by catalog handler,
/// with `today` as the given day number. Names stay borrowed from the
/// static language, unlike `host::host_values`.
pub fn fixture_values_for(language: &'static Language<'static>, today: f64) -> Vec<HostValue> {
    let mut values: Vec<HostValue> = Vec::new();
    let declared = language.rules.iter().flat_map(|r| match r.action {
        Action::Reference { values, .. } => values,
        _ => &[],
    });
    for value in declared {
        if values
            .iter()
            .any(|v| v.kind == value.kind && v.name == value.name)
        {
            continue;
        }
        let constant = match host::value(value.handler).map(|h| h.read) {
            Some(host::Read::Constant(number)) => number,
            Some(host::Read::Today) => today,
            None => continue,
        };
        values.push(HostValue::new(
            value.kind,
            value.name,
            value.ty,
            move || Ok(constant),
        ));
    }
    values
}

pub fn observe_dates(source: &str, today: f64) -> Value {
    observe_with_values(
        &dates::LANGUAGE,
        source,
        true,
        &fixture_values_for(&dates::LANGUAGE, today),
    )
}

/// A runtime value as JSON: numbers (integers too) as numbers, booleans,
/// strings, and records (fields in order) and lists as arrays.
pub fn datum(value: &Datum) -> Value {
    match value {
        Datum::Number(n) => json!(n),
        Datum::Integer(i) => json!(i),
        Datum::Boolean(b) => json!(b),
        Datum::Text(t) => json!(t),
        Datum::Record(items) | Datum::List(items) => {
            Value::Array(items.iter().map(datum).collect())
        }
    }
}

/// Runs Calendar with the built-in handlers and `implementation` of its
/// `Gregorian` interface.
pub fn observe_calendar(
    source: &str,
    implementation: std::sync::Arc<dyn calendar::gregorian::Gregorian>,
) -> Value {
    let mut bindings = fixture_bindings_for(&calendar::LANGUAGE);
    bindings.extend(calendar::gregorian::bindings(
        &calendar::LANGUAGE,
        implementation,
    ));
    observe_with(&calendar::LANGUAGE, source, true, &[], &bindings)
}

pub fn observe_rewrites(source: &str, today: f64) -> Value {
    observe_with_values(
        &rewrites::LANGUAGE,
        source,
        true,
        &fixture_values_for(&rewrites::LANGUAGE, today),
    )
}

pub fn observe(source: &str) -> Value {
    observe_language(&LANGUAGE, source)
}

pub fn observe_functions(source: &str) -> Value {
    observe_with_kinds(&functions::LANGUAGE, source, true)
}

pub fn observe_calls(source: &str) -> Value {
    observe_with_kinds(&calls::LANGUAGE, source, true)
}

pub fn observe_inference(source: &str) -> Value {
    observe_with_kinds(&inference::LANGUAGE, source, true)
}

pub fn observe_namespaces(source: &str) -> Value {
    observe_with_kinds(&namespaces::LANGUAGE, source, true)
}

pub fn observe_language(language: &'static Language<'static>, source: &str) -> Value {
    observe_with_kinds(language, source, false)
}

fn observe_with_kinds(
    language: &'static Language<'static>,
    source: &str,
    include_kinds: bool,
) -> Value {
    observe_with_values(language, source, include_kinds, &[])
}

fn observe_with_values(
    language: &'static Language<'static>,
    source: &str,
    include_kinds: bool,
    values: &[HostValue],
) -> Value {
    observe_with(
        language,
        source,
        include_kinds,
        values,
        &fixture_bindings_for(language),
    )
}

fn observe_with(
    language: &'static Language<'static>,
    source: &str,
    include_kinds: bool,
    values: &[HostValue],
    bindings: &[HostBinding<'static>],
) -> Value {
    let error = |e: Diagnostic| {
        let mut observation =
            json!({"ok":false,"error":e.category,"span":e.span,"message":e.message,"trace":[]});
        if let Some(code) = e.code {
            observation["code"] = json!(code);
        }
        observation
    };
    let tree = match parse(language, source, Limits::default()) {
        Ok(t) => t,
        Err(e) => return error(e),
    };
    let program = match compile(language, &tree, Limits::default()) {
        Ok(p) => p,
        Err(e) => return error(e.into_iter().next().unwrap()),
    };
    let report = match execute_with_values(&program, bindings, values, Limits::default()) {
        Ok(r) => r,
        Err(failure) => {
            let mut observation = json!({"ok":false,"error":failure.diagnostic.category,"message":failure.diagnostic.message,"trace":failure.trace});
            if let Some(code) = failure.diagnostic.code {
                observation["code"] = json!(code);
            }
            return observation;
        }
    };
    let mut syntax = Vec::new();
    let mut stack = vec![tree.root];
    while let Some(id) = stack.pop() {
        let node = tree.nodes[id as usize];
        let rule = &language.rules[node.rule as usize];
        if !matches!(rule.action, Action::None)
            || rule.name == "Portable.Paren"
            || rule.name == "Namespaces.Paren"
            || rule.name == "Inference.Paren"
            || rule.name == "Calls.Paren"
            || rule.name == "Functions.Paren"
            || rule.name == "Dates.Paren"
            || rule.name == "Calendar.Paren"
        {
            syntax.push(json!({"kind":rule.name,"span":node.span}));
        }
        stack.extend(tree.children(id).iter().rev().map(|e| e.node));
    }
    let mut hir = Vec::<Value>::with_capacity(program.nodes.len());
    for node in &program.nodes {
        let mut observation = match &node.kind {
            HirKind::Error => json!({"kind":"error"}),
            HirKind::Constant(number) => {
                json!({"kind":"constant","type":node.ty,"span":node.span,"number":number})
            }
            HirKind::Reference(symbol) => {
                json!({"kind":"reference","type":node.ty,"span":node.span,"symbol":symbol})
            }
            HirKind::Builtin(value) => {
                json!({"kind":"reference","type":node.ty,"span":node.span,"builtin":value.name})
            }
            HirKind::Text { start, end } => {
                json!({"kind":"text","type":node.ty,"span":node.span,"text":&program.text[*start as usize..*end as usize]})
            }
            HirKind::Operation { operation, args } => {
                json!({"kind":"operation","type":node.ty,"span":node.span,"operation":language.operations[*operation as usize].id,
                "args":program.args[args.start as usize..args.end as usize].iter().map(|id| hir[*id as usize].clone()).collect::<Vec<_>>()})
            }
        };
        if std::ptr::eq(language, &functions::LANGUAGE) {
            observation["origins"] = json!(program.origins(hir.len() as u32));
        }
        if !language.helpers.is_empty() {
            // Rewrite profiles report every origin with its source, as Nitrogen does.
            let id = hir.len() as u32;
            let paths = program.origin_in_rewrite(id);
            observation.as_object_mut().unwrap().remove("span");
            observation["origins"] =
                json!(program.origins(id).iter().zip(paths).map(|(span, rewrite)| {
                json!({"path":if *rewrite {"nitrogen:rewrite"} else {"input"},"span":span})
            }).collect::<Vec<_>>());
        }
        hir.push(observation);
    }
    let mut bindings = program
        .bindings
        .iter()
        .map(|b| {
            if include_kinds {
                json!({"span":b.span,"symbol":b.symbol,"kind":b.kind})
            } else {
                json!({"span":b.span,"symbol":b.symbol})
            }
        })
        .chain(
            program
                .builtin_bindings
                .iter()
                .map(|b| json!({"span":b.span,"builtin":b.name,"kind":b.kind})),
        )
        .collect::<Vec<_>>();
    bindings.sort_by_key(|b| b["span"][0].as_u64().unwrap());
    json!({"ok":true,"syntax":syntax,"bindings":bindings,
        "hir":program.statements.iter().map(|s| &hir[s.value as usize]).collect::<Vec<_>>(),"values":report.values.iter().map(datum).collect::<Vec<_>>(),"trace":report.trace})
}
