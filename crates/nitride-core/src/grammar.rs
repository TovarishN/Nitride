#[derive(Debug)]
pub enum Expr<'g> {
    Literal(&'g str),
    Class(&'g [(char, char)]),
    NotClass(&'g [(char, char)]),
    Any,
    Predicate {
        positive: bool,
        inner: &'g Expr<'g>,
    },
    Ref(u16),
    Sequence(&'g [Expr<'g>]),
    Choice(&'g [Expr<'g>]),
    Label(&'g str, &'g Expr<'g>),
    Repeat {
        inner: &'g Expr<'g>,
        min: u32,
        max: u32,
    },
    Pratt {
        prefixes: &'g [u16],
        infixes: &'g [u16],
        /// Alternatives can begin alike, so results are memoized by position.
        memoize: bool,
    },
}

#[derive(Debug)]
pub enum Action<'g> {
    None,
    Literal {
        ty: &'g str,
        field: &'g str,
    },
    Reference {
        kinds: &'g [&'g str],
        field: &'g str,
        values: &'g [BuiltinValue<'g>],
    },
    Operation {
        id: u16,
        fields: &'g [&'g str],
        /// Positions passing their field's exact source spelling as `Core.Text`.
        texts: &'g [bool],
    },
    Overload {
        candidates: &'g [u16],
        fields: &'g [&'g str],
    },
    Call {
        kinds: &'g [&'g str],
        field: &'g str,
        fields: &'g [&'g str],
        builtins: &'g [Builtin<'g>],
    },
    Template {
        kind: &'g str,
        field: &'g str,
        body: &'g str,
        parameters: &'g str,
    },
    Parameter {
        kind: &'g str,
        field: &'g str,
        annotation: &'g str,
    },
    Alias {
        kind: &'g str,
        field: &'g str,
    },
    Expand {
        kinds: &'g [&'g str],
        field: &'g str,
        arguments: &'g str,
    },
    Declare {
        kind: &'g str,
        field: &'g str,
        annotation: Option<&'g str>,
        value: &'g str,
    },
    Show {
        field: &'g str,
    },
    /// Replaces the use with its helper's template, holes filled by `fields`.
    Rewrite {
        helper: u16,
        fields: &'g [&'g str],
        name: &'g str,
    },
}

#[derive(Debug)]
pub struct Builtin<'g> {
    pub kind: &'g str,
    pub name: &'g str,
    pub candidates: &'g [u16],
}

/// A host-supplied built-in value, typed by the catalog. The handler names
/// what the value is in the host vocabulary (`pi`, `today`), as an
/// operation's handler does, so hosts need not know the language's names.
#[derive(Debug, PartialEq, Eq)]
pub struct BuiltinValue<'g> {
    pub kind: &'g str,
    pub name: &'g str,
    pub ty: &'g str,
    pub handler: &'g str,
}

/// A fixed engine predicate over a field's source spelling; never translated C#.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Predicate {
    IsoDate,
}

#[derive(Debug)]
pub struct Check<'g> {
    pub rule: u16,
    pub field: &'g str,
    pub predicate: Predicate,
    pub code: &'g str,
    pub message: &'g str,
}

/// A `$Name` template hole and the syntax category it stands for.
#[derive(Debug)]
pub struct Hole<'g> {
    pub name: &'g str,
    pub category: u16,
}

/// An ordered match arm: captures, the pattern quote and the result quote.
#[derive(Debug)]
pub struct Arm<'g> {
    pub captures: &'g [Hole<'g>],
    pub pattern: &'g str,
    pub result: &'g str,
}

/// A typed quote helper; inline rewrites are synthesized helpers whose
/// parameters are the use's syntax fields. A match helper's body is its fallback.
#[derive(Debug)]
pub struct Helper<'g> {
    pub name: &'g str,
    pub parameters: &'g [Hole<'g>],
    pub result: u16,
    pub body: &'g str,
    pub selector: Option<u8>,
    pub arms: &'g [Arm<'g>],
}

/// How a rule check reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// What a rule check tests on each node of its rule.
#[derive(Debug)]
pub enum RuleTest<'g> {
    /// Holds when the two fields' source texts are equal (or differ).
    Compare {
        left: &'g str,
        right: &'g str,
        equal: bool,
    },
    /// Reports when the node matches the quote, parsed as the rule. `$name`
    /// is one node of its capture's category; `..$name` any run of siblings.
    Match {
        captures: &'g [Hole<'g>],
        pattern: &'g str,
    },
}

/// A diagnostic a syntax rule declares for its own nodes: a `check` reports
/// when its comparison fails, a `match` when its pattern matches. The message
/// fills `{Field.Text}` and `{capture}` from the node.
#[derive(Debug)]
pub struct RuleCheck<'g> {
    pub rule: u16,
    pub code: &'g str,
    pub severity: Severity,
    pub test: RuleTest<'g>,
    pub message: &'g str,
    /// The field the diagnostic spans; the whole node when absent.
    pub at: Option<&'g str>,
}

#[derive(Debug)]
pub struct Rule<'g> {
    pub name: &'g str,
    pub lexical: bool,
    pub expr: Expr<'g>,
    pub except: &'g [&'g str],
    pub precedence: u32,
    pub right_associative: bool,
    /// A prefix alternative with no node of its own: its one element stands in.
    pub transparent: bool,
    /// A syntax-only rule of references and literals with no node of its own
    /// (Nitrogen's alias): its one child, if any, stands in for it.
    pub alias: bool,
    pub owner: Option<u16>,
    pub action: Action<'g>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signature<'g> {
    pub id: &'g str,
    pub inputs: &'g [&'g str],
    pub result: &'g str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Typed,
    SyntaxOnly,
}

#[derive(Debug)]
pub struct Language<'g> {
    pub profile: Profile,
    pub rules: &'g [Rule<'g>],
    pub start: u16,
    pub types: &'g [&'g str],
    pub operations: &'g [Signature<'g>],
    pub handlers: &'g [&'g str],
    pub checks: &'g [Check<'g>],
    pub helpers: &'g [Helper<'g>],
    pub rule_checks: &'g [RuleCheck<'g>],
    pub source_hash: &'g str,
}
