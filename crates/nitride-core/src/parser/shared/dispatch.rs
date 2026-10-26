//! Conservative start sets for prepared grammar rules. No recognition decisions
//! are committed here: unknown and nullable starts always remain eligible.
use crate::syntax::memory::{self, Class, Visitor};
use crate::{Expr, Language};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Start {
    ascii: [u64; 2],
    wide: bool,
    nullable: bool,
}
impl Start {
    const UNKNOWN: Self = Self {
        ascii: [u64::MAX; 2],
        wide: true,
        nullable: true,
    };
    fn union(self, other: Self) -> Self {
        Self {
            ascii: [
                self.ascii[0] | other.ascii[0],
                self.ascii[1] | other.ascii[1],
            ],
            wide: self.wide || other.wide,
            nullable: self.nullable || other.nullable,
        }
    }
    fn character(c: char) -> Self {
        let mut result = Self::default();
        if c.is_ascii() {
            result.ascii[c as usize / 64] |= 1 << (c as usize % 64);
        } else {
            result.wide = true;
        }
        result
    }
    fn with_trivia(self) -> Self {
        // Non-ASCII is deliberately unclassified: Unicode whitespace is legal.
        self.union(Self {
            ascii: [
                0x1_0000_0000
                    | (1 << 9)
                    | (1 << 10)
                    | (1 << 11)
                    | (1 << 12)
                    | (1 << 13)
                    | (1 << 47),
                0,
            ],
            wide: true,
            nullable: false,
        })
    }
    pub(super) fn allows(self, value: Option<char>) -> bool {
        self.nullable
            || value.is_some_and(|c| {
                if c.is_ascii() {
                    self.ascii[c as usize / 64] & (1 << (c as usize % 64)) != 0
                } else {
                    self.wide
                }
            })
    }
    pub(super) fn nullable(self) -> bool {
        self.nullable
    }
}

/// Eligibility routes retain ordering; uncertain/nullable alternatives appear in
/// every route. The caller runs leading uncertain alternatives before peeking.
pub(super) struct ChoicePlan {
    routes: Box<[[u64; 4]; 130]>,
    first: usize,
}
impl ChoicePlan {
    pub(super) fn new(starts: &[Start]) -> Option<Self> {
        if starts.len() > 256 {
            return None;
        }
        let first = starts.iter().position(|start| !start.nullable())?;
        let mut routes = Box::new([[0u64; 4]; 130]);
        for (index, start) in starts.iter().enumerate() {
            for (byte, route) in routes[..128].iter_mut().enumerate() {
                if start.allows(Some(byte as u8 as char)) {
                    route[index / 64] |= 1 << (index % 64);
                }
            }
            if start.allows(Some('\u{80}')) {
                routes[128][index / 64] |= 1 << (index % 64);
            }
            if start.allows(None) {
                routes[129][index / 64] |= 1 << (index % 64);
            }
        }
        Some(Self { routes, first })
    }
    pub(super) fn first(&self) -> usize {
        self.first
    }
    pub(super) fn candidates(&self, value: Option<char>, from: usize) -> ChoiceCandidates {
        let row = value.map_or(129, |c| if c.is_ascii() { c as usize } else { 128 });
        let mut words = self.routes[row];
        if from >= 256 {
            words = [0; 4];
        } else {
            words[..from / 64].fill(0);
            words[from / 64] &= u64::MAX << (from % 64);
        }
        ChoiceCandidates { words, word: 0 }
    }
}
pub(super) struct ChoiceCandidates {
    words: [u64; 4],
    word: usize,
}
impl Iterator for ChoiceCandidates {
    type Item = usize;
    #[inline]
    fn next(&mut self) -> Option<usize> {
        while self.word < 4 {
            let bits = self.words[self.word];
            if bits != 0 {
                self.words[self.word] &= bits - 1;
                return Some(self.word * 64 + bits.trailing_zeros() as usize);
            }
            self.word += 1;
        }
        None
    }
}

pub(crate) struct Dispatch {
    starts: Vec<Start>,
    tails: Vec<Option<Start>>,
}
impl Dispatch {
    pub(crate) fn new(language: &Language) -> Self {
        let mut starts = vec![Start::default(); language.rules.len()];
        let mut budget = 1_000_000usize;
        // Monotone iteration; an exhausted preparation budget disables filtering.
        let solve = |starts: &mut Vec<Start>, budget: &mut usize| -> Option<()> {
            loop {
                let previous = starts.clone();
                for (id, rule) in language.rules.iter().enumerate() {
                    starts[id] =
                        previous[id].union(first(&rule.expr, &previous, language, budget, 0)?);
                }
                if *starts == previous {
                    return Some(());
                }
            }
        };
        let cycles = solve(&mut starts, &mut budget)
            .and_then(|()| leading_cycles(language, &starts, &mut budget));
        let completed = cycles.is_some();
        if let Some(cycles) = cycles {
            for (id, cyclic) in cycles.into_iter().enumerate() {
                if cyclic {
                    starts[id] = Start::UNKNOWN;
                }
            }
        }
        if completed {
            // Unproductive recursive starts must still reach the recognizer's
            // ordinary depth/grammar checks, rather than become early failures.
            for start in &mut starts {
                if *start == Start::default() {
                    *start = Start::UNKNOWN;
                }
            }
        }
        if !completed || solve(&mut starts, &mut budget).is_none() {
            return Self {
                starts: vec![Start::UNKNOWN; language.rules.len()],
                tails: vec![None; language.rules.len()],
            };
        }
        let tails = language
            .rules
            .iter()
            .map(|rule| {
                let Expr::Sequence(parts) = &rule.expr else {
                    return None;
                };
                let tail = parts.get(1..)?;
                if !tail.first().is_some_and(ordinary_trivia_entry) {
                    return None;
                }
                sequence(tail, &starts, language, &mut budget, 0)
            })
            .collect();
        Self { starts, tails }
    }
    pub(super) fn choice_start(&self, mut expr: &Expr, language: &Language) -> Start {
        // Only inspect a first primitive. In particular, don't jump across a
        // nullable repetition, predicate or embedded Pratt grammar check.
        for _ in 0..128 {
            match expr {
                Expr::Label(_, inner) => expr = inner,
                Expr::Sequence(parts) => {
                    let Some(first) = parts.first() else {
                        return Start::UNKNOWN;
                    };
                    expr = first;
                }
                Expr::Literal(_) | Expr::Any | Expr::Ref(_) => {
                    return first(expr, &self.starts, language, &mut 4096, 0)
                        .unwrap_or(Start::UNKNOWN);
                }
                Expr::Class(ranges) | Expr::NotClass(ranges) if ranges.len() <= 4096 => {
                    return first(expr, &self.starts, language, &mut 4096, 0)
                        .unwrap_or(Start::UNKNOWN);
                }
                _ => return Start::UNKNOWN,
            }
        }
        Start::UNKNOWN
    }
    pub(super) fn start(&self, id: u16) -> Start {
        self.starts
            .get(id as usize)
            .copied()
            .unwrap_or(Start::UNKNOWN)
    }
    pub(super) fn tail(&self, id: u16) -> Option<Start> {
        self.tails.get(id as usize).copied().flatten()
    }
    pub(crate) fn visit_allocations(&self, visitor: &mut Visitor<'_>) {
        if let Some((id, bytes)) = memory::vector(&self.starts) {
            visitor(Class::Index, id, bytes);
        }
        if let Some((id, bytes)) = memory::vector(&self.tails) {
            visitor(Class::Index, id, bytes);
        }
    }
}
// Rules reaching a cycle or fatal construct before mandatory consumption keep
// ordinary recognition. Synthetic self-edges mark fatal leading constructs.
fn leading_cycles(language: &Language, starts: &[Start], budget: &mut usize) -> Option<Vec<bool>> {
    let mut reverse = vec![Vec::<usize>::new(); language.rules.len()];
    let mut remaining = vec![0usize; language.rules.len()];
    for (id, rule) in language.rules.iter().enumerate() {
        let mut calls = Vec::new();
        leading_calls(&rule.expr, starts, language, budget, 0, &mut calls, id)?;
        calls.sort_unstable();
        calls.dedup();
        remaining[id] = calls.len();
        for callee in calls {
            reverse[callee].push(id);
        }
    }
    let mut leaves: Vec<_> = remaining
        .iter()
        .enumerate()
        .filter_map(|(id, count)| (*count == 0).then_some(id))
        .collect();
    while let Some(id) = leaves.pop() {
        for parent in &reverse[id] {
            remaining[*parent] -= 1;
            if remaining[*parent] == 0 {
                leaves.push(*parent);
            }
        }
    }
    Some(remaining.into_iter().map(|count| count != 0).collect())
}
fn leading_calls(
    expr: &Expr,
    starts: &[Start],
    language: &Language,
    budget: &mut usize,
    depth: usize,
    out: &mut Vec<usize>,
    owner: usize,
) -> Option<()> {
    if depth > 128 || *budget == 0 {
        return None;
    }
    *budget -= 1;
    match expr {
        Expr::Ref(id) => {
            if (*id as usize) < starts.len() {
                out.push(*id as usize);
            }
        }
        Expr::Sequence(parts) => {
            for part in *parts {
                leading_calls(part, starts, language, budget, depth + 1, out, owner)?;
                if !first(part, starts, language, budget, depth + 1)?.nullable {
                    break;
                }
            }
        }
        Expr::Choice(parts) => {
            for part in *parts {
                leading_calls(part, starts, language, budget, depth + 1, out, owner)?;
            }
        }
        Expr::Label(_, inner) | Expr::Predicate { inner, .. } => {
            leading_calls(inner, starts, language, budget, depth + 1, out, owner)?
        }
        Expr::Repeat { inner, max, .. } if *max != 0 => {
            if *max > 1 && first(inner, starts, language, budget, depth + 1)?.nullable {
                out.push(owner);
            }
            leading_calls(inner, starts, language, budget, depth + 1, out, owner)?
        }
        Expr::Pratt { prefixes, .. } => {
            if depth != 0 {
                out.push(owner);
            } // embedded Pratt expressions are grammar errors
            for id in *prefixes {
                if (*id as usize) < starts.len() {
                    out.push(*id as usize);
                }
            }
        }
        _ => (),
    }
    Some(())
}

fn ordinary_trivia_entry(mut expr: &Expr) -> bool {
    for _ in 0..128 {
        match expr {
            Expr::Literal(_) | Expr::Class(_) | Expr::NotClass(_) | Expr::Any | Expr::Ref(_) => {
                return true;
            }
            Expr::Label(_, inner) => expr = inner,
            _ => return false,
        }
    }
    false
}

fn sequence(
    parts: &[Expr],
    starts: &[Start],
    language: &Language,
    budget: &mut usize,
    depth: usize,
) -> Option<Start> {
    let mut result = Start::default();
    for part in parts {
        let next = first(part, starts, language, budget, depth + 1)?;
        result = result.union(Start {
            nullable: false,
            ..next
        });
        if !next.nullable {
            return Some(result);
        }
    }
    result.nullable = true;
    Some(result)
}
fn first(
    expr: &Expr,
    starts: &[Start],
    language: &Language,
    budget: &mut usize,
    depth: usize,
) -> Option<Start> {
    if depth > 128 || *budget == 0 {
        return None;
    }
    *budget -= 1;
    Some(match expr {
        Expr::Literal(s) => s.chars().next().map_or(
            Start {
                nullable: true,
                ..Start::default()
            },
            Start::character,
        ),
        Expr::Class(ranges) | Expr::NotClass(ranges) => {
            let negative = matches!(expr, Expr::NotClass(_));
            let mut result = Start {
                wide: negative || ranges.iter().any(|(_, b)| !b.is_ascii()),
                ..Start::default()
            };
            for (a, b) in *ranges {
                if a.is_ascii() {
                    for byte in *a as u32..=(*b as u32).min(127) {
                        result.ascii[byte as usize / 64] |= 1 << (byte as usize % 64);
                    }
                }
            }
            if negative {
                result.ascii = [!result.ascii[0], !result.ascii[1]];
            }
            result
        }
        Expr::Any => Start {
            nullable: false,
            ..Start::UNKNOWN
        },
        Expr::Predicate { .. } => Start {
            nullable: true,
            ..Start::default()
        },
        Expr::Ref(id) => {
            let value = starts.get(*id as usize).copied().unwrap_or(Start::UNKNOWN);
            if value != Start::default()
                && language
                    .rules
                    .get(*id as usize)
                    .is_none_or(|rule| !rule.lexical)
            {
                value.with_trivia()
            } else {
                value
            }
        }
        Expr::Label(_, inner) => first(inner, starts, language, budget, depth + 1)?,
        Expr::Sequence(parts) => sequence(parts, starts, language, budget, depth + 1)?,
        Expr::Choice(parts) => {
            let mut result = Start::default();
            for part in *parts {
                result = result.union(first(part, starts, language, budget, depth + 1)?);
            }
            result
        }
        Expr::Repeat { inner, min, max } => {
            if *max == 0 {
                Start {
                    nullable: true,
                    ..Start::default()
                }
            } else {
                let mut value = first(inner, starts, language, budget, depth + 1)?;
                value.nullable |= *min == 0;
                value
            }
        }
        Expr::Pratt { prefixes, .. } => {
            let mut result = Start::default();
            for id in *prefixes {
                result = result.union(first(&Expr::Ref(*id), starts, language, budget, depth + 1)?);
            }
            result
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    #[test]
    fn routing_keeps_order_across_words_unicode_and_eof_and_rejects_oversize() {
        let mut starts = vec![Start::character('x'); 256];
        starts[0] = Start::UNKNOWN;
        starts[63] = Start::character('z');
        starts[64] = Start {
            nullable: true,
            ..Start::default()
        };
        starts[127] = Start::character('λ');
        starts[128] = Start::character('z');
        starts[255] = Start::character('z');
        let plan = ChoicePlan::new(&starts).unwrap();
        assert_eq!(
            plan.candidates(Some('z'), 0).collect::<Vec<_>>(),
            vec![0, 63, 64, 128, 255]
        );
        assert_eq!(
            plan.candidates(Some('z'), 64).collect::<Vec<_>>(),
            vec![64, 128, 255]
        );
        assert_eq!(
            plan.candidates(Some('界'), 0).collect::<Vec<_>>(),
            vec![0, 64, 127]
        );
        assert_eq!(plan.candidates(None, 0).collect::<Vec<_>>(), vec![0, 64]);
        assert!(plan.candidates(Some('z'), 256).next().is_none());
        starts.push(Start::character('z'));
        assert!(ChoicePlan::new(&starts).is_none());
    }
    #[test]
    fn choice_selectors_leave_uncertain_or_zero_width_prefixes_eligible() {
        let language = Language {
            profile: Profile::SyntaxOnly,
            rules: &[],
            start: 0,
            types: &[],
            operations: &[],
            handlers: &[],
            checks: &[],
            helpers: &[],
            source_hash: "choice",
        };
        let plan = Dispatch::new(&language);
        assert!(
            !plan
                .choice_start(&Expr::Literal("x"), &language)
                .allows(Some('y'))
        );
        assert!(
            !plan
                .choice_start(
                    &Expr::Sequence(&[
                        Expr::Label("value", &Expr::Literal("x")),
                        Expr::Literal("z")
                    ]),
                    &language
                )
                .allows(Some('y'))
        );
        for expr in [
            Expr::Sequence(&[
                Expr::Predicate {
                    positive: true,
                    inner: &Expr::Literal("x"),
                },
                Expr::Literal("z"),
            ]),
            Expr::Sequence(&[
                Expr::Repeat {
                    inner: &Expr::Literal(""),
                    min: 0,
                    max: 2,
                },
                Expr::Literal("z"),
            ]),
            Expr::Ref(0),
            Expr::Sequence(&[Expr::Literal(""), Expr::Literal("z")]),
        ] {
            assert!(plan.choice_start(&expr, &language).allows(Some('y')));
        }
    }
    #[test]
    fn deep_trivia_entry_analysis_disables_filtering() {
        let mut expression: &'static Expr = &Expr::Ref(0);
        for _ in 0..129 {
            expression = Box::leak(Box::new(Expr::Label("nested", expression)));
        }
        assert!(!ordinary_trivia_entry(expression));
    }
    #[test]
    fn starts_are_conservative_for_nullable_predicates_unicode_and_recursion() {
        let bodies = vec![
            Expr::Sequence(&[
                Expr::Predicate {
                    positive: false,
                    inner: &Expr::Literal("z"),
                },
                Expr::Ref(1),
                Expr::Literal("q"),
            ]),
            Expr::Choice(&[
                Expr::Class(&[('a', 'c')]),
                Expr::Literal("λ"),
                Expr::Literal(""),
            ]),
            Expr::Ref(3),
            Expr::Choice(&[Expr::Ref(2), Expr::Literal("r")]),
            Expr::Sequence(&[
                Expr::Repeat {
                    inner: &Expr::Literal("x"),
                    min: 0,
                    max: 10,
                },
                Expr::Literal("z"),
            ]),
            Expr::NotClass(&[('a', 'z')]),
            Expr::Literal(""),
            Expr::Ref(7),
        ];
        let rules = Box::leak(
            bodies
                .into_iter()
                .map(|expr| Rule {
                    name: "Test",
                    lexical: true,
                    expr,
                    except: &[],
                    precedence: 0,
                    right_associative: false,
                    transparent: false,
                    alias: false,
                    owner: None,
                    action: Action::None,
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        let language = Language {
            profile: Profile::SyntaxOnly,
            rules,
            start: 0,
            types: &[],
            operations: &[],
            handlers: &[],
            checks: &[],
            helpers: &[],
            source_hash: "starts",
        };
        let plan = Dispatch::new(&language);
        for c in ['a', 'b', 'c', 'q', 'λ', '界'] {
            assert!(plan.starts[0].allows(Some(c)));
        }
        assert!(!plan.starts[0].allows(Some('z')));
        assert!(!plan.starts[0].allows(None));
        assert!(plan.starts[2].allows(Some('r')));
        assert!(plan.starts[2].allows(Some('s'))); // productive leading cycles remain unfiltered
        for c in ['x', 'z'] {
            assert!(plan.starts[4].allows(Some(c)));
        }
        assert!(!plan.starts[4].allows(Some('q')));
        assert!(plan.starts[5].allows(Some('A')));
        assert!(!plan.starts[5].allows(Some('a')));
        assert!(plan.starts[6].allows(None));
        assert!(plan.starts[7].allows(Some('!'))); // unproductive cycles stay unfiltered
    }
}
