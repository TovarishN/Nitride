//! Conservative start sets for prepared grammar rules. No recognition decisions
//! are committed here: unknown and nullable starts always remain eligible.
use crate::syntax::memory::{self, Class, Visitor};
use crate::{Expr, Language, Rule};

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
/// Leads, when present, further exclude alternatives by their first token.
pub(super) struct ChoicePlan {
    routes: Option<Routes>,
    first: usize,
    len: usize,
    leads: Box<[Lead]>,
}
impl ChoicePlan {
    pub(super) fn new(starts: &[Start], leads: Box<[Lead]>) -> Option<Self> {
        if starts.len() > 256 {
            return None;
        }
        let first = starts.iter().position(|start| !start.nullable())?;
        let leads = if leads.iter().any(Option::is_some) {
            leads
        } else {
            Box::default()
        };
        if starts.len() < 4 && leads.is_empty() {
            return None;
        }
        let routes = (starts.len() >= 4).then(|| routes(starts));
        Some(Self {
            routes,
            first,
            len: starts.len(),
            leads,
        })
    }
    pub(super) fn first(&self) -> usize {
        self.first
    }
    pub(super) fn lead(&self, index: usize) -> Option<&[Step]> {
        self.leads.get(index)?.as_deref()
    }
    /// Requested bytes owned by this plan, excluding its shared allocation.
    pub(super) fn bytes(&self) -> usize {
        self.routes.as_ref().map_or(0, Routes::bytes)
            + std::mem::size_of_val::<[Lead]>(&self.leads)
            + self
                .leads
                .iter()
                .flatten()
                .map(|lead| std::mem::size_of_val::<[Step]>(lead))
                .sum::<usize>()
    }
    pub(super) fn candidates(&self, value: Option<char>, from: usize) -> ChoiceCandidates {
        let mut words = if let Some(routes) = &self.routes {
            routes.get(value)
        } else {
            let mut words = [0u64; 4];
            for (index, word) in words.iter_mut().enumerate() {
                let bits = self.len.saturating_sub(index * 64).min(64);
                *word = if bits == 64 {
                    u64::MAX
                } else {
                    (1 << bits) - 1
                };
            }
            words
        };
        if from >= 256 {
            words = [0; 4];
        } else {
            words[..from / 64].fill(0);
            words[from / 64] &= u64::MAX << (from % 64);
        }
        ChoiceCandidates { words, word: 0 }
    }
}
/// Per-character eligibility bit sets over at most 256 ordered alternatives:
/// 128 ASCII rows, one for other characters and one for the end of input,
/// each only as many words wide as the alternatives need.
pub(super) struct Routes {
    words: usize,
    bits: Box<[u64]>,
}
impl Routes {
    fn get(&self, value: Option<char>) -> [u64; 4] {
        let row = value.map_or(129, |c| if c.is_ascii() { c as usize } else { 128 });
        let mut words = [0u64; 4];
        words[..self.words].copy_from_slice(&self.bits[row * self.words..][..self.words]);
        words
    }
    fn bytes(&self) -> usize {
        std::mem::size_of_val::<[u64]>(&self.bits)
    }
}
fn routes(starts: &[Start]) -> Routes {
    let words = starts.len().div_ceil(64).max(1);
    let mut bits = vec![0u64; 130 * words].into_boxed_slice();
    for (index, start) in starts.iter().enumerate() {
        let (word, bit) = (index / 64, 1 << (index % 64));
        for byte in 0..128u8 {
            if start.allows(Some(byte as char)) {
                bits[byte as usize * words + word] |= bit;
            }
        }
        if start.allows(Some('\u{80}')) {
            bits[128 * words + word] |= bit;
        }
        if start.allows(None) {
            bits[129 * words + word] |= bit;
        }
    }
    Routes { words, bits }
}

pub(super) struct Prefix {
    pub(super) id: u16,
    pub(super) power: u32,
    pub(super) precedence: u32,
}
pub(super) struct Infix {
    pub(super) id: u16,
    pub(super) precedence: u32,
    pub(super) power: u32,
    pub(super) label: &'static str,
    pub(super) parts: &'static [Expr<'static>],
    pub(super) owner: Option<u16>,
}
/// Precomputed dispatch for a non-lexical Pratt point: validated operator
/// metadata plus character routes. Callers peek once after trivia, attempt
/// only routed operators in grammar order and charge the rest as skipped.
pub(super) struct PrattPlan {
    pub(super) prefixes: Box<[Prefix]>,
    prefix_routes: Routes,
    pub(super) infixes: Box<[Infix]>,
    infix_routes: Routes,
    /// Ascending precedences with the infixes at or above each.
    floors: Box<[(u32, [u64; 4])]>,
}
impl PrattPlan {
    fn new(
        point: &Rule<'static>,
        language: &Language<'static>,
        starts: &[Start],
        tails: &[Option<Start>],
    ) -> Option<Self> {
        let Expr::Pratt {
            prefixes, infixes, ..
        } = point.expr
        else {
            return None;
        };
        // Few alternatives gain nothing from routes worth their tables.
        if point.lexical
            || prefixes.len() + infixes.len() < 4
            || prefixes.len() > 256
            || infixes.len() > 256
        {
            return None;
        }
        let prefix_starts: Vec<_> = prefixes
            .iter()
            .map(|id| {
                let start = *starts.get(*id as usize)?;
                Some(if start.nullable() {
                    Start::UNKNOWN
                } else {
                    start
                })
            })
            .collect::<Option<_>>()?;
        let prefixes = prefixes
            .iter()
            .map(|id| {
                let rule = language.rules.get(*id as usize)?;
                Some(Prefix {
                    id: *id,
                    power: rule.precedence.checked_add(1)?,
                    precedence: rule.precedence,
                })
            })
            .collect::<Option<Box<[_]>>>()?;
        let mut infix_starts = Vec::with_capacity(infixes.len());
        let infixes = infixes
            .iter()
            .map(|id| {
                let rule = language.rules.get(*id as usize)?;
                let Expr::Sequence(parts) = &rule.expr else {
                    return None;
                };
                let label = match parts.first() {
                    Some(Expr::Label(label, _)) => *label,
                    Some(Expr::Ref(_)) => "",
                    _ => return None,
                };
                infix_starts.push(
                    tails
                        .get(*id as usize)
                        .copied()
                        .flatten()
                        .filter(|start| !start.nullable())
                        .unwrap_or(Start::UNKNOWN),
                );
                Some(Infix {
                    id: *id,
                    precedence: rule.precedence,
                    power: rule
                        .precedence
                        .checked_add(u32::from(!rule.right_associative))?,
                    label,
                    parts,
                    owner: rule.owner,
                })
            })
            .collect::<Option<Box<[_]>>>()?;
        let mut precedences: Vec<_> = infixes.iter().map(|infix| infix.precedence).collect();
        precedences.sort_unstable();
        precedences.dedup();
        let floors = precedences
            .into_iter()
            .map(|floor| {
                let mut mask = [0u64; 4];
                for (index, infix) in infixes.iter().enumerate() {
                    if infix.precedence >= floor {
                        mask[index / 64] |= 1 << (index % 64);
                    }
                }
                (floor, mask)
            })
            .collect();
        Some(Self {
            prefix_routes: routes(&prefix_starts),
            prefixes,
            infix_routes: routes(&infix_starts),
            infixes,
            floors,
        })
    }
    pub(super) fn prefix_candidates(&self, value: Option<char>) -> ChoiceCandidates {
        ChoiceCandidates {
            words: self.prefix_routes.get(value),
            word: 0,
        }
    }
    /// Routed infixes at or above `minimum`, and how many eligible ones the
    /// route excludes.
    pub(super) fn infix_candidates(
        &self,
        value: Option<char>,
        minimum: u32,
    ) -> (ChoiceCandidates, u32) {
        let index = self.floors.partition_point(|(floor, _)| *floor < minimum);
        let eligible = self.floors.get(index).map_or([0; 4], |(_, mask)| *mask);
        let routed = self.infix_routes.get(value);
        let mut words = [0u64; 4];
        let mut excluded = 0;
        for word in 0..4 {
            words[word] = eligible[word] & routed[word];
            excluded += (eligible[word] & !routed[word]).count_ones();
        }
        (ChoiceCandidates { words, word: 0 }, excluded)
    }
    pub(super) fn has_infixes(&self, minimum: u32) -> bool {
        self.floors
            .last()
            .is_some_and(|(floor, _)| *floor >= minimum)
    }
    fn visit(&self, visitor: &mut Visitor<'_>) {
        for (id, bytes) in [
            (
                self.prefix_routes.bits.as_ptr() as usize,
                self.prefix_routes.bytes(),
            ),
            (
                self.infix_routes.bits.as_ptr() as usize,
                self.infix_routes.bytes(),
            ),
            (
                self.prefixes.as_ptr() as usize,
                std::mem::size_of_val::<[Prefix]>(&self.prefixes),
            ),
            (
                self.infixes.as_ptr() as usize,
                std::mem::size_of_val::<[Infix]>(&self.infixes),
            ),
            (
                self.floors.as_ptr() as usize,
                std::mem::size_of_val::<[(u32, [u64; 4])]>(&self.floors),
            ),
        ] {
            if bytes != 0 {
                visitor(Class::Index, id, bytes);
            }
        }
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

/// A token read after trivia: a literal with keyword boundaries or a
/// complete lexical rule match.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(super) enum Terminal {
    Literal(&'static str),
    Lexical(u16),
}
/// A possible beginning of a non-empty match: its first token, as an index
/// into the grammar's sorted terminal table, and what may come next.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(super) struct Step {
    pub(super) first: u16,
    next: u16,
}
impl Step {
    /// The match may be this one token; whatever follows it comes next.
    /// Analysis extends it with the following tokens.
    const OPEN: u16 = u16::MAX;
    /// Anything may come next.
    const ANY: u16 = u16::MAX - 1;
    /// The required second token, if any.
    pub(super) fn second(self) -> Option<u16> {
        (self.next < Self::ANY).then_some(self.next)
    }
}
/// Possible beginnings, or `None` when a match may begin otherwise.
pub(super) type Lead = Option<Box<[Step]>>;
/// Per-rule leads in one allocation; unknown ranges start at `u32::MAX`.
#[derive(Default)]
struct Leads {
    steps: Vec<Step>,
    ranges: Vec<[u32; 2]>,
}
impl Leads {
    fn push(&mut self, lead: Option<&[Step]>) {
        self.ranges.push(match lead {
            Some(lead) => {
                let start = self.steps.len() as u32;
                self.steps.extend_from_slice(lead);
                [start, self.steps.len() as u32]
            }
            None => [u32::MAX; 2],
        });
    }
    fn get(&self, id: u16) -> Option<&[Step]> {
        let [start, end] = *self.ranges.get(id as usize)?;
        self.steps.get(start as usize..end as usize)
    }
    fn visit(&self, visitor: &mut Visitor<'_>) {
        if let Some((id, bytes)) = memory::vector(&self.steps) {
            visitor(Class::Index, id, bytes);
        }
        if let Some((id, bytes)) = memory::vector(&self.ranges) {
            visitor(Class::Index, id, bytes);
        }
    }
}
const LEAD_LIMIT: usize = 48;

pub(crate) struct Dispatch {
    starts: Vec<Start>,
    tails: Vec<Option<Start>>,
    leads: Leads,
    tail_leads: Leads,
    pratts: Vec<Option<Box<PrattPlan>>>,
    /// Sorted; steps index into it.
    terminals: Vec<Terminal>,
    /// Per terminal: an identifier-like literal, which needs a word boundary.
    keywords: Vec<bool>,
    /// Per terminal: a literal's ASCII first byte; `LEXICAL` or `WIDE` otherwise.
    first_bytes: Vec<u8>,
}
impl Dispatch {
    pub(crate) fn new(language: &Language<'static>) -> Self {
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
                leads: Leads::default(),
                tail_leads: Leads::default(),
                pratts: language.rules.iter().map(|_| None).collect(),
                terminals: Vec::new(),
                keywords: Vec::new(),
                first_bytes: Vec::new(),
            };
        }
        let tails: Vec<Option<Start>> = language
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
        let pratts = language
            .rules
            .iter()
            .map(|rule| PrattPlan::new(rule, language, &starts, &tails[..]).map(Box::new))
            .collect();
        // Every grammar literal and lexical rule, so later choice leads find theirs.
        let mut terminals = Vec::new();
        for (id, rule) in language.rules.iter().enumerate() {
            if rule.lexical {
                terminals.push(Terminal::Lexical(id as u16));
            } else {
                collect_literals(&rule.expr, 0, &mut terminals);
            }
        }
        terminals.sort_unstable();
        terminals.dedup();
        terminals.shrink_to_fit();
        let keywords = terminals
            .iter()
            .map(|terminal| match terminal {
                Terminal::Literal(text) => {
                    text.starts_with(|c: char| c.is_alphabetic() || c == '_')
                        && text.chars().all(|c| c.is_alphanumeric() || c == '_')
                }
                Terminal::Lexical(_) => false,
            })
            .collect();
        let first_bytes = terminals
            .iter()
            .map(|terminal| match terminal {
                Terminal::Literal(text) => match text.as_bytes()[0] {
                    byte if byte.is_ascii() => byte,
                    _ => Self::WIDE,
                },
                Terminal::Lexical(_) => Self::LEXICAL,
            })
            .collect();
        let mut dispatch = Self {
            starts,
            tails,
            leads: Leads::default(),
            tail_leads: Leads::default(),
            pratts,
            terminals,
            keywords,
            first_bytes,
        };
        if dispatch.terminals.len() >= Step::ANY as usize {
            dispatch.terminals = Vec::new();
            dispatch.keywords = Vec::new();
            dispatch.first_bytes = Vec::new();
            return dispatch;
        }
        let mut analysis = Analysis {
            language,
            starts: &dispatch.starts,
            terminals: &dispatch.terminals,
            budget,
        };
        let leads = analysis.solve();
        let mut infixes = vec![false; language.rules.len()];
        for rule in language.rules {
            if let Expr::Pratt { infixes: ids, .. } = rule.expr {
                for id in ids {
                    if let Some(infix) = infixes.get_mut(*id as usize) {
                        *infix = true;
                    }
                }
            }
        }
        let mut tail_leads = Leads::default();
        for (rule, infix) in language.rules.iter().zip(&infixes) {
            let tail = match &rule.expr {
                Expr::Sequence(parts) if *infix && !rule.lexical => {
                    parts.get(1..).filter(|tail| !tail.is_empty())
                }
                _ => None,
            };
            let found = tail
                .and_then(|tail| analysis.sequence(tail, Table::Solving(&leads), 0))
                .filter(|found| !found.nullable);
            tail_leads.push(found.as_ref().map(|found| &found.steps[..]));
        }
        let mut rule_leads = Leads::default();
        for lead in &leads {
            rule_leads.push(lead.as_deref());
        }
        rule_leads.steps.shrink_to_fit();
        tail_leads.steps.shrink_to_fit();
        dispatch.leads = rule_leads;
        dispatch.tail_leads = tail_leads;
        dispatch
    }
    pub(super) fn terminal(&self, id: u16) -> Terminal {
        self.terminals[id as usize]
    }
    pub(super) fn keyword(&self, id: u16) -> bool {
        self.keywords[id as usize]
    }
    pub(super) const LEXICAL: u8 = 0x80;
    pub(super) const WIDE: u8 = 0x81;
    /// A literal's ASCII first byte, `WIDE` for other literals, `LEXICAL`.
    pub(super) fn first_byte(&self, id: u16) -> u8 {
        self.first_bytes[id as usize]
    }
    /// Possible beginnings of a non-lexical alternative that cannot match empty.
    pub(super) fn alternative_lead(
        &self,
        expr: &'static Expr<'static>,
        language: &Language<'static>,
    ) -> Lead {
        let mut analysis = Analysis {
            language,
            starts: &self.starts,
            terminals: &self.terminals,
            budget: 4096,
        };
        let found = analysis.lead(expr, Table::Final(&self.leads), 0)?;
        (!found.nullable).then(|| found.steps.into())
    }
    /// Possible beginnings of a non-lexical rule called after trivia.
    pub(super) fn lead(&self, id: u16) -> Option<&[Step]> {
        if self.starts.get(id as usize)?.nullable() {
            return None;
        }
        self.leads.get(id)
    }
    pub(super) fn pratt(&self, id: u16) -> Option<&PrattPlan> {
        self.pratts.get(id as usize)?.as_deref()
    }
    pub(super) fn tail_lead(&self, id: u16) -> Option<&[Step]> {
        self.tail_leads.get(id)
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
    /// Whether a call of rule `id` at a position whose character is `c` may
    /// match: the start set a reference sees, including leading trivia for a
    /// syntax rule. Conservative.
    pub(crate) fn may_begin(&self, id: u16, lexical: bool, c: Option<char>) -> bool {
        let start = self.start(id);
        if lexical || start == Start::default() {
            start
        } else {
            start.with_trivia()
        }
        .allows(c)
    }
    /// Whether a non-empty match of rule `id` may begin with `c`; conservative.
    pub(crate) fn may_consume(&self, id: u16, c: char) -> bool {
        Start {
            nullable: false,
            ..self.start(id)
        }
        .allows(Some(c))
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
        if let Some((id, bytes)) = memory::vector(&self.terminals) {
            visitor(Class::Index, id, bytes);
        }
        if let Some((id, bytes)) = memory::vector(&self.keywords) {
            visitor(Class::Index, id, bytes);
        }
        if let Some((id, bytes)) = memory::vector(&self.first_bytes) {
            visitor(Class::Index, id, bytes);
        }
        if let Some((id, bytes)) = memory::vector(&self.pratts) {
            visitor(Class::Index, id, bytes);
        }
        for plan in self.pratts.iter().flatten() {
            visitor(
                Class::Index,
                &**plan as *const PrattPlan as usize,
                std::mem::size_of::<PrattPlan>(),
            );
            plan.visit(visitor);
        }
        self.leads.visit(visitor);
        self.tail_leads.visit(visitor);
    }
}

fn collect_literals(expr: &'static Expr<'static>, depth: usize, into: &mut Vec<Terminal>) {
    if depth > 64 {
        return;
    }
    match expr {
        Expr::Literal(text) if !text.is_empty() => into.push(Terminal::Literal(text)),
        Expr::Label(_, inner) | Expr::Repeat { inner, .. } | Expr::Predicate { inner, .. } => {
            collect_literals(inner, depth + 1, into)
        }
        Expr::Sequence(parts) | Expr::Choice(parts) => {
            for part in *parts {
                collect_literals(part, depth + 1, into);
            }
        }
        _ => (),
    }
}
#[derive(Clone, Copy)]
enum Table<'a> {
    Solving(&'a [Option<Vec<Step>>]),
    Final(&'a Leads),
}
impl<'a> Table<'a> {
    fn get(self, id: u16) -> Option<&'a [Step]> {
        match self {
            Self::Solving(leads) => leads.get(id as usize)?.as_deref(),
            Self::Final(leads) => leads.get(id),
        }
    }
}
/// Beginnings of the non-empty matches of an expression, and whether it may
/// also match empty.
struct Found {
    steps: Vec<Step>,
    nullable: bool,
}
impl Found {
    const EMPTY: Self = Self {
        steps: Vec::new(),
        nullable: true,
    };
    const NONE: Self = Self {
        steps: Vec::new(),
        nullable: false,
    };
    fn token(first: u16, nullable: bool) -> Self {
        Self {
            steps: vec![Step {
                first,
                next: Step::OPEN,
            }],
            nullable,
        }
    }
    /// What follows is no longer known: open tokens admit anything.
    fn close(mut self) -> Self {
        for step in &mut self.steps {
            if step.next == Step::OPEN {
                step.next = Step::ANY;
            }
        }
        canonicalize(&mut self.steps);
        self
    }
}
/// Sorted, without duplicates, and minimal: `ANY` subsumes every beginning
/// with its first token. An open token does not: analysis may still extend it.
fn canonicalize(steps: &mut Vec<Step>) {
    steps.sort_unstable();
    steps.dedup();
    let (mut read, mut write) = (0, 0);
    while read < steps.len() {
        let first = steps[read].first;
        let end = read
            + steps[read..]
                .iter()
                .take_while(|step| step.first == first)
                .count();
        if steps[read..end].iter().any(|step| step.next == Step::ANY) {
            steps[write] = Step {
                first,
                next: Step::ANY,
            };
            write += 1;
        } else {
            steps.copy_within(read..end, write);
            write += end - read;
        }
        read = end;
    }
    steps.truncate(write);
}
/// Canonical form; oversized sets keep only first tokens, then give up.
fn normalize(mut steps: Vec<Step>) -> Option<Vec<Step>> {
    canonicalize(&mut steps);
    if steps.len() > LEAD_LIMIT {
        for step in &mut steps {
            step.next = Step::ANY;
        }
        steps.dedup();
    }
    (steps.len() <= LEAD_LIMIT).then_some(steps)
}
fn union(into: &mut Found, from: Found) -> Option<()> {
    into.steps.extend(from.steps);
    into.nullable |= from.nullable;
    into.steps = normalize(std::mem::take(&mut into.steps))?;
    Some(())
}
/// `x` followed by `y`; an unknown `y` closes the open tokens of `x`.
fn concat(x: Found, y: Option<Found>) -> Option<Found> {
    let Some(y) = y else {
        return (!x.nullable).then(|| x.close());
    };
    let mut steps = Vec::new();
    for step in &x.steps {
        if step.next != Step::OPEN {
            steps.push(*step);
            continue;
        }
        steps.extend(y.steps.iter().map(|next| Step {
            first: step.first,
            next: next.first,
        }));
        if y.nullable {
            steps.push(*step);
        }
    }
    if x.nullable {
        steps.extend_from_slice(&y.steps);
    }
    Some(Found {
        steps: normalize(steps)?,
        nullable: x.nullable && y.nullable,
    })
}
/// Lead analysis over one grammar and its terminal table.
struct Analysis<'a> {
    language: &'a Language<'static>,
    starts: &'a [Start],
    terminals: &'a [Terminal],
    budget: usize,
}
impl Analysis<'_> {
    fn terminal(&self, terminal: Terminal) -> Option<u16> {
        self.terminals
            .binary_search(&terminal)
            .ok()
            .map(|id| id as u16)
    }
    // Least fixpoint from empty sets; unknown absorbs. Rules with an unknown
    // start (cycles, unproductive or unanalyzed) stay unknown, so recognition
    // still reaches their ordinary depth and grammar checks.
    fn solve(&mut self) -> Vec<Option<Vec<Step>>> {
        let rules = self.language.rules;
        let mut leads: Vec<_> = rules
            .iter()
            .zip(self.starts)
            .map(|(rule, start)| (!rule.lexical && *start != Start::UNKNOWN).then(Vec::new))
            .collect();
        // Recompute only the callers of a rule whose lead changed.
        let mut callers = vec![Vec::new(); rules.len()];
        for (id, rule) in rules.iter().enumerate() {
            let mut callees = Vec::new();
            references(&rule.expr, 0, &mut callees);
            callees.sort_unstable();
            callees.dedup();
            for callee in callees {
                if let Some(list) = callers.get_mut(callee as usize) {
                    list.push(id);
                }
            }
        }
        let mut queued = vec![true; rules.len()];
        let mut queue: std::collections::VecDeque<_> = (0..rules.len()).collect();
        while let Some(id) = queue.pop_front() {
            queued[id] = false;
            let Some(current) = &leads[id] else {
                continue;
            };
            let next = self
                .lead(&rules[id].expr, Table::Solving(&leads), 0)
                .and_then(|next| {
                    let mut merged = current.clone();
                    merged.extend(next.steps);
                    normalize(merged)
                });
            if self.budget == 0 {
                return vec![None; rules.len()];
            }
            if next.as_ref() != Some(current) {
                leads[id] = next;
                for caller in &callers[id] {
                    if !queued[*caller] {
                        queued[*caller] = true;
                        queue.push_back(*caller);
                    }
                }
            }
        }
        leads
    }
    fn sequence(
        &mut self,
        parts: &'static [Expr<'static>],
        leads: Table<'_>,
        depth: usize,
    ) -> Option<Found> {
        let mut result = Found::EMPTY;
        for part in parts {
            // Later parts matter only after an empty or an open prefix.
            if !result.nullable && result.steps.iter().all(|step| step.next != Step::OPEN) {
                break;
            }
            // A failing inline check stops before trivia, short of the next token.
            if checks_inline(part, 0) {
                result = result.close();
            }
            let next = self.lead(part, leads, depth + 1);
            result = concat(result, next)?;
        }
        Some(result)
    }
    /// Beginnings of the matches of `expr` in a non-lexical context.
    /// Zero-width parts contribute nothing; nullability is over-approximated.
    fn lead(
        &mut self,
        expr: &'static Expr<'static>,
        leads: Table<'_>,
        depth: usize,
    ) -> Option<Found> {
        if depth > 64 || self.budget == 0 {
            return None;
        }
        self.budget -= 1;
        match expr {
            Expr::Literal("") => Some(Found::EMPTY),
            Expr::Literal(text) => {
                Some(Found::token(self.terminal(Terminal::Literal(text))?, false))
            }
            Expr::Ref(id) => self.reference(*id, leads),
            Expr::Label(_, inner) => self.lead(inner, leads, depth + 1),
            Expr::Sequence(parts) => self.sequence(parts, leads, depth),
            Expr::Choice(parts) => {
                let mut result = Found::NONE;
                for part in *parts {
                    let found = self.lead(part, leads, depth + 1)?;
                    union(&mut result, found)?;
                }
                Some(result)
            }
            Expr::Repeat { max: 0, .. } | Expr::Predicate { .. } => Some(Found::EMPTY),
            Expr::Repeat { inner, min, max } => {
                let inner = self.lead(inner, leads, depth + 1)?;
                let mut found = Found {
                    steps: inner.steps.clone(),
                    nullable: inner.nullable || *min == 0,
                };
                if *max > 1 {
                    // A one-token iteration may be followed by another one.
                    for step in inner.steps.iter().filter(|step| step.next == Step::OPEN) {
                        found.steps.extend(inner.steps.iter().map(|next| Step {
                            first: step.first,
                            next: next.first,
                        }));
                    }
                    found.steps = normalize(found.steps)?;
                }
                Some(found)
            }
            // Operands begin with a prefix match. Operators may follow any of
            // them, so a one-token operand admits anything next.
            Expr::Pratt { prefixes, .. } => {
                let mut result = Found::NONE;
                for id in *prefixes {
                    let found = self.reference(*id, leads)?;
                    union(&mut result, found)?;
                }
                Some(result.close())
            }
            Expr::Class(_) | Expr::NotClass(_) | Expr::Any => None,
        }
    }
    fn reference(&self, id: u16, leads: Table<'_>) -> Option<Found> {
        let rule = self.language.rules.get(id as usize)?;
        let nullable = self.starts.get(id as usize)?.nullable();
        if rule.lexical {
            if matches!(rule.expr, Expr::Pratt { .. }) {
                return None;
            }
            Some(Found::token(
                self.terminal(Terminal::Lexical(id))?,
                nullable,
            ))
        } else {
            let found = Found {
                steps: leads.get(id)?.to_vec(),
                nullable,
            };
            // An excluded text fails after the match, before any following token.
            Some(if rule.except.is_empty() {
                found
            } else {
                found.close()
            })
        }
    }
}
fn references(expr: &Expr, depth: usize, into: &mut Vec<u16>) {
    if depth > 64 {
        return;
    }
    match expr {
        Expr::Ref(id) => into.push(*id),
        Expr::Pratt { prefixes, .. } => into.extend_from_slice(prefixes),
        Expr::Label(_, inner) | Expr::Repeat { inner, .. } | Expr::Predicate { inner, .. } => {
            references(inner, depth + 1, into)
        }
        Expr::Sequence(parts) | Expr::Choice(parts) => {
            for part in *parts {
                references(part, depth + 1, into);
            }
        }
        _ => (),
    }
}
/// Whether `expr` may fail at a zero-width check before reading a token.
/// Such failures stop before trivia, so no token pair may cross them.
fn checks_inline(expr: &Expr, depth: usize) -> bool {
    if depth > 64 {
        return true;
    }
    match expr {
        Expr::Predicate { .. } => true,
        Expr::Label(_, inner) | Expr::Repeat { inner, .. } => checks_inline(inner, depth + 1),
        Expr::Sequence(parts) | Expr::Choice(parts) => {
            parts.iter().any(|part| checks_inline(part, depth + 1))
        }
        _ => false,
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
        let plan = ChoicePlan::new(&starts, Box::default()).unwrap();
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
        assert!(ChoicePlan::new(&starts, Box::default()).is_none());
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
        let mut expression: &'static Expr<'static> = &Expr::Ref(0);
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
    fn leads_for(bodies: Vec<(bool, Expr<'static>)>) -> (Dispatch, &'static Language<'static>) {
        let rules = Box::leak(
            bodies
                .into_iter()
                .map(|(lexical, expr)| Rule {
                    name: "Test",
                    lexical,
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
        let language = Box::leak(Box::new(Language {
            profile: Profile::SyntaxOnly,
            rules,
            start: 0,
            types: &[],
            operations: &[],
            handlers: &[],
            checks: &[],
            helpers: &[],
            source_hash: "leads",
        }));
        (Dispatch::new(language), language)
    }
    #[derive(Clone, Copy, PartialEq, Debug)]
    struct Seq {
        first: Terminal,
        next: Next,
    }
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Next {
        Open,
        Any,
        Token(Terminal),
    }
    fn seqs(plan: &Dispatch, id: u16) -> Option<Vec<Seq>> {
        let steps = plan.lead(id)?;
        Some(
            steps
                .iter()
                .map(|step| Seq {
                    first: plan.terminal(step.first),
                    next: match step.next {
                        Step::OPEN => Next::Open,
                        Step::ANY => Next::Any,
                        token => Next::Token(plan.terminal(token)),
                    },
                })
                .collect(),
        )
    }
    fn seq(first: &'static str, next: Next) -> Seq {
        Seq {
            first: Terminal::Literal(first),
            next,
        }
    }
    fn token(text: &'static str) -> Next {
        Next::Token(Terminal::Literal(text))
    }
    #[test]
    fn leads_close_after_unknown_parts_and_pratt_operands() {
        let (plan, _) = leads_for(vec![
            // An unknown middle part hides what follows "{".
            (
                false,
                Expr::Sequence(&[Expr::Literal("{"), Expr::Ref(1), Expr::Literal("}")]),
            ),
            (false, Expr::Class(&[('a', 'z')])),
            // Operators may follow a one-token operand.
            (
                false,
                Expr::Pratt {
                    prefixes: &[3],
                    infixes: &[4],
                    memoize: false,
                },
            ),
            (false, Expr::Literal("x")),
            (
                false,
                Expr::Sequence(&[Expr::Ref(2), Expr::Literal("+"), Expr::Ref(2)]),
            ),
            (false, Expr::Sequence(&[Expr::Ref(2), Expr::Literal(";")])),
            (true, Expr::Class(&[('a', 'z')])),
            (false, Expr::Sequence(&[Expr::Ref(6), Expr::Literal("::")])),
        ]);
        assert_eq!(seqs(&plan, 0).unwrap(), &[seq("{", Next::Any)]);
        assert!(seqs(&plan, 1).is_none());
        assert_eq!(seqs(&plan, 5).unwrap(), &[seq("x", Next::Any)]);
        assert_eq!(
            seqs(&plan, 7).unwrap(),
            &[Seq {
                first: Terminal::Lexical(6),
                next: token("::"),
            }]
        );
    }
    #[test]
    fn leads_keep_repeated_and_alternative_continuations() {
        let (plan, _) = leads_for(vec![
            (
                false,
                Expr::Sequence(&[
                    Expr::Repeat {
                        inner: &Expr::Literal("a"),
                        min: 0,
                        max: u32::MAX,
                    },
                    Expr::Literal("c"),
                ]),
            ),
            // An open token must not absorb the longer beginning it shares.
            (
                false,
                Expr::Sequence(&[
                    Expr::Choice(&[
                        Expr::Literal("a"),
                        Expr::Sequence(&[Expr::Literal("a"), Expr::Literal("b")]),
                    ]),
                    Expr::Literal("c"),
                ]),
            ),
            (
                false,
                Expr::Sequence(&[
                    Expr::Predicate {
                        positive: false,
                        inner: &Expr::Literal("z"),
                    },
                    Expr::Literal("q"),
                ]),
            ),
        ]);
        let lead = seqs(&plan, 0).unwrap();
        for expected in [
            seq("a", token("a")),
            seq("a", token("c")),
            seq("c", Next::Open),
        ] {
            assert!(lead.contains(&expected), "{lead:?}");
        }
        assert_eq!(lead.len(), 3);
        assert_eq!(
            seqs(&plan, 1).unwrap(),
            &[seq("a", token("b")), seq("a", token("c"))]
        );
        assert_eq!(seqs(&plan, 2).unwrap(), &[seq("q", Next::Open)]);
    }
    #[test]
    fn oversized_leads_keep_only_first_tokens() {
        let words: &'static [Expr<'static>] = Box::leak(
            (0..LEAD_LIMIT + 1)
                .map(|n| Expr::Literal(Box::leak(format!("w{n}").into_boxed_str())))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        let wide: &'static [Expr<'static>] =
            Box::leak(Box::new([Expr::Choice(words), Expr::Literal(";")]));
        // Two by twenty-five pairs exceed the limit; their two first tokens don't.
        let pairs: &'static [Expr<'static>] = Box::leak(Box::new([
            Expr::Choice(&words[..2]),
            Expr::Choice(&words[..LEAD_LIMIT / 2 + 1]),
            Expr::Literal(";"),
        ]));
        let (plan, _) = leads_for(vec![
            (false, Expr::Sequence(wide)),
            (false, Expr::Sequence(pairs)),
        ]);
        assert!(
            seqs(&plan, 0).is_none(),
            "first tokens alone exceed the limit"
        );
        let lead = seqs(&plan, 1).unwrap();
        assert_eq!(lead.len(), 2);
        assert!(lead.iter().all(|seq| seq.next == Next::Any), "{lead:?}");
    }
}
