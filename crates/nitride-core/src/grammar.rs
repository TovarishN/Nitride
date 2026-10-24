#[derive(Debug)]
pub enum Expr {
    Literal(&'static str),
    Class(&'static [(char, char)]),
    NotClass(&'static [(char, char)]),
    Any,
    Predicate {
        positive: bool,
        inner: &'static Expr,
    },
    Ref(u16),
    Sequence(&'static [Expr]),
    Choice(&'static [Expr]),
    Label(&'static str, &'static Expr),
    Repeat {
        inner: &'static Expr,
        min: u32,
        max: u32,
    },
    Pratt {
        prefixes: &'static [u16],
        infixes: &'static [u16],
    },
}

#[derive(Debug)]
pub enum Action {
    None,
    Literal {
        ty: &'static str,
        field: &'static str,
    },
    Reference {
        kinds: &'static [&'static str],
        field: &'static str,
        values: &'static [BuiltinValue],
    },
    Operation {
        id: u16,
        fields: &'static [&'static str],
        /// Positions passing their field's exact source spelling as `Core.Text`.
        texts: &'static [bool],
    },
    Overload {
        candidates: &'static [u16],
        fields: &'static [&'static str],
    },
    Call {
        kinds: &'static [&'static str],
        field: &'static str,
        fields: &'static [&'static str],
        builtins: &'static [Builtin],
    },
    Template {
        kind: &'static str,
        field: &'static str,
        body: &'static str,
        parameters: &'static str,
    },
    Parameter {
        kind: &'static str,
        field: &'static str,
        annotation: &'static str,
    },
    Alias {
        kind: &'static str,
        field: &'static str,
    },
    Expand {
        kinds: &'static [&'static str],
        field: &'static str,
        arguments: &'static str,
    },
    Declare {
        kind: &'static str,
        field: &'static str,
        annotation: Option<&'static str>,
        value: &'static str,
    },
    Show {
        field: &'static str,
    },
}

#[derive(Debug)]
pub struct Builtin {
    pub kind: &'static str,
    pub name: &'static str,
    pub candidates: &'static [u16],
}

/// A host-supplied built-in value, typed by the catalog.
#[derive(Debug, PartialEq, Eq)]
pub struct BuiltinValue {
    pub kind: &'static str,
    pub name: &'static str,
    pub ty: &'static str,
}

/// A fixed engine predicate over a field's source spelling; never translated C#.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Predicate {
    IsoDate,
}

#[derive(Debug)]
pub struct Check {
    pub rule: u16,
    pub field: &'static str,
    pub predicate: Predicate,
    pub code: &'static str,
    pub message: &'static str,
}

#[derive(Debug)]
pub struct Rule {
    pub name: &'static str,
    pub lexical: bool,
    pub expr: Expr,
    pub except: &'static [&'static str],
    pub precedence: u32,
    pub right_associative: bool,
    pub owner: Option<u16>,
    pub action: Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signature {
    pub id: &'static str,
    pub inputs: &'static [&'static str],
    pub result: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Typed,
    SyntaxOnly,
}

#[derive(Debug)]
pub struct Language {
    pub profile: Profile,
    pub rules: &'static [Rule],
    pub start: u16,
    pub types: &'static [&'static str],
    pub operations: &'static [Signature],
    pub handlers: &'static [&'static str],
    pub checks: &'static [Check],
    pub source_hash: &'static str,
}
