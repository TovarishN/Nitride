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
    },
    Operation {
        id: u16,
        fields: &'static [&'static str],
    },
    Overload {
        candidates: &'static [u16],
        fields: &'static [&'static str],
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
    pub source_hash: &'static str,
}
