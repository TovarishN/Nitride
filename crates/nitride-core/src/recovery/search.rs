use super::grammar::{Code, Grammar, Instruction};
use super::*;
use crate::parser::StrictProbe;
use std::collections::{BinaryHeap, HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Task {
    Rule {
        id: u16,
        power: u32,
        label: &'static str,
        expand: bool,
    },
    Expr {
        code: Code,
        owner: Option<u16>,
        power: u32,
        label: &'static str,
    },
    Close {
        id: u16,
        start: usize,
        label: &'static str,
    },
    Repeat {
        code: Code,
        count: u32,
        min: u32,
        max: u32,
        label: &'static str,
    },
    AfterRepeat {
        code: Code,
        count: u32,
        min: u32,
        max: u32,
        label: &'static str,
        start: usize,
        history: u32,
    },
    Tail {
        point: u16,
        power: u32,
        label: &'static str,
        start: usize,
    },
    End,
}

#[derive(Clone, Copy, Debug)]
enum Event {
    Open {
        id: u16,
        start: usize,
        label: &'static str,
    },
    Close {
        end: usize,
    },
    Strict {
        root: u32,
        label: &'static str,
    },
    Missing {
        expected: Expected,
        at: usize,
    },
    Skip {
        start: usize,
        end: usize,
    },
    Exhausted {
        at: usize,
    },
    Wrap {
        id: u16,
        start: usize,
        left_label: &'static str,
        label: &'static str,
    },
}

#[derive(Clone, Debug)]
struct State {
    stack: u32,
    history: u32,
    position: usize,
    cost: u32,
    skipped: usize,
    mandatory_skipped: usize,
    missing: u32,
    repairs: u64,
    clean: u32,
    origin: usize,
    prefix: Option<usize>,
    reopen_through: Option<usize>,
}

struct Prefix {
    state: State,
    id: u16,
    power: u32,
    label: &'static str,
    furthest: usize,
}

struct Tasks {
    nodes: Vec<(Task, u32)>,
    known: HashMap<(Task, u32), u32>,
}
impl Tasks {
    fn new() -> Self {
        Self {
            nodes: vec![(Task::End, 0)],
            known: HashMap::new(),
        }
    }
    fn push(&mut self, task: Task, next: u32) -> u32 {
        *self.known.entry((task, next)).or_insert_with(|| {
            let id = self.nodes.len() as u32;
            self.nodes.push((task, next));
            id
        })
    }
}

type Rank = (
    std::cmp::Reverse<u32>,
    std::cmp::Reverse<usize>,
    std::cmp::Reverse<u32>,
    usize,
    std::cmp::Reverse<u64>,
);
type StateKey = (u32, usize, usize, Option<usize>);
type RepairRank = (u32, usize, u32);
struct Engine<'a> {
    language: &'static Language,
    source: &'a str,
    limits: Limits,
    options: RecoveryOptions,
    graph: Grammar,
    probe: StrictProbe<'a>,
    tasks: Tasks,
    history: Vec<(u32, Event)>,
    committed_history: u32,
    states: Vec<State>,
    queue: BinaryHeap<(Rank, usize)>,
    seen: HashMap<StateKey, RepairRank>,
    stats: RecoveryStats,
    epoch_states: usize,
    serial: u64,
    best: Option<State>,
    literal_tokens: Vec<&'static str>,
    character_tokens: Vec<&'static Expr>,
    lexical: Vec<u16>,
    prefixes: Vec<Prefix>,
}

pub(super) fn recover(
    language: &'static Language,
    source: &str,
    limits: Limits,
    options: RecoveryOptions,
) -> RecoveredParse {
    let graph = Grammar::new(language);
    let mut literal_tokens = Vec::new();
    let mut character_tokens = Vec::new();
    for rule in language.rules.iter().filter(|r| !r.lexical) {
        let mut pending = vec![&rule.expr];
        while let Some(expr) = pending.pop() {
            match expr {
                Expr::Literal(s) if !s.is_empty() => literal_tokens.push(*s),
                Expr::Class(_) | Expr::NotClass(_) | Expr::Any => character_tokens.push(expr),
                Expr::Sequence(parts) | Expr::Choice(parts) => pending.extend(parts.iter()),
                Expr::Label(_, inner) | Expr::Repeat { inner, .. } => pending.push(inner),
                _ => {}
            }
        }
    }
    let lexical = language
        .rules
        .iter()
        .enumerate()
        .filter(|(_, r)| r.lexical)
        .map(|(i, _)| i as u16)
        .collect();
    let mut engine = Engine {
        language,
        source,
        limits,
        options,
        graph,
        probe: StrictProbe::new(language, source, limits),
        tasks: Tasks::new(),
        history: vec![(0, Event::Close { end: 0 })],
        committed_history: 0,
        states: vec![],
        queue: BinaryHeap::new(),
        seen: HashMap::new(),
        stats: RecoveryStats::default(),
        epoch_states: 0,
        serial: 0,
        best: None,
        literal_tokens,
        character_tokens,
        lexical,
        prefixes: Vec::new(),
    };
    let end = engine.tasks.push(Task::End, 0);
    let stack = engine.tasks.push(
        Task::Rule {
            id: language.start,
            power: 0,
            label: "",
            expand: true,
        },
        end,
    );
    engine.enqueue(State {
        stack,
        history: 0,
        position: 0,
        cost: 0,
        skipped: 0,
        mandatory_skipped: 0,
        missing: 0,
        repairs: 0,
        clean: 0,
        origin: 0,
        prefix: None,
        reopen_through: None,
    });
    match engine.run() {
        Ok(state) => engine.finish(state, None),
        Err(error) => {
            if let Some(state) = engine.best.clone() {
                engine.finish(state, Some(error))
            } else {
                tree::fallback(language, source, error)
            }
        }
    }
}

impl Engine<'_> {
    fn enqueue(&mut self, state: State) {
        if state.cost > self.options.max_local_cost
            || (state.cost > 0
                && state.position.saturating_sub(state.origin) > self.options.max_local_bytes)
        {
            return;
        }
        let key = (
            state.stack,
            state.position,
            state.origin,
            state.reopen_through,
        );
        let rank = (
            state.cost,
            state.skipped - state.mandatory_skipped,
            state.missing,
        );
        if self.seen.get(&key).is_some_and(|old| *old <= rank) {
            return;
        }
        self.seen.insert(key, rank);
        let priority = (
            std::cmp::Reverse(state.cost),
            std::cmp::Reverse(state.skipped - state.mandatory_skipped),
            std::cmp::Reverse(state.missing),
            state.position,
            std::cmp::Reverse(self.serial),
        );
        self.serial += 1;
        let id = self.states.len();
        self.states.push(state);
        self.queue.push((priority, id));
    }
    fn event(&mut self, state: &mut State, event: Event) {
        let id = self.history.len() as u32;
        self.history.push((state.history, event));
        state.history = id;
    }
    fn push(&mut self, state: &mut State, task: Task) {
        state.stack = self.tasks.push(task, state.stack);
    }
    fn expr(
        &mut self,
        state: &mut State,
        code: Code,
        owner: Option<u16>,
        power: u32,
        label: &'static str,
    ) {
        self.push(
            state,
            Task::Expr {
                code,
                owner,
                power,
                label,
            },
        );
    }
    fn budget(&self) -> u64 {
        self.limits
            .work
            .max((self.source.len() as u64).saturating_mul(self.limits.parser_work_per_byte))
    }
    fn tick(&mut self, state: &State) -> Result<(), Diagnostic> {
        self.stats.work += 1;
        if self.stats.work.saturating_add(self.probe.work()) > self.budget() {
            return Err(Diagnostic::new(
                "limit",
                [state.position as u32; 2],
                "recovery work limit exceeded",
            ));
        }
        if state.cost > 0 {
            self.epoch_states += 1;
            self.stats.search_states += 1;
            if self.epoch_states > self.options.max_states {
                return Err(Diagnostic::new(
                    "limit",
                    [state.position as u32; 2],
                    "recovery search state limit exceeded",
                ));
            }
        }
        Ok(())
    }
    fn trivia(&mut self, state: &mut State) {
        loop {
            let rest = &self.source[state.position..];
            if let Some(c) = rest.chars().next().filter(|c| c.is_whitespace()) {
                state.position += c.len_utf8();
            } else if rest.starts_with("//") {
                state.position += rest.find('\n').unwrap_or(rest.len());
            } else if let Some(after_open) = rest.strip_prefix("/*") {
                if let Some(end) = after_open.find("*/") {
                    state.position += end + 4;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
    fn literal_end(&self, text: &str, at: usize) -> Option<usize> {
        let end = at.checked_add(text.len())?;
        if !self.source.get(at..)?.starts_with(text) {
            return None;
        }
        let keyword = text.starts_with(|c: char| c.is_alphabetic() || c == '_')
            && text.chars().all(|c| c.is_alphanumeric() || c == '_');
        if keyword && self.source[end..].starts_with(|c: char| c.is_alphanumeric() || c == '_') {
            return None;
        }
        Some(end)
    }
    /// Conservative, grammar-derived FIRST test; predicates are still evaluated
    /// strictly by the interpreter. A visited set breaks nullable/ref cycles.
    fn starts(&mut self, code: Code, at: usize) -> Result<bool, Diagnostic> {
        let mut pending = vec![code];
        let mut seen = HashSet::new();
        while let Some(code) = pending.pop() {
            if !seen.insert(code) {
                continue;
            }
            match &self.graph.code[code as usize] {
                Instruction::Terminal(Expected::Literal(text), _) => {
                    if self.literal_end(text, at).is_some() {
                        return Ok(true);
                    }
                }
                Instruction::Terminal(_, expr) => {
                    if self.probe.predicate(expr, at)? {
                        return Ok(true);
                    }
                }
                Instruction::Predicate(_) => {}
                Instruction::Ref(id) => {
                    if self.language.rules[*id as usize].lexical {
                        if self.probe.rule(*id, at, 0)?.is_some() {
                            return Ok(true);
                        }
                    } else {
                        pending.push(self.graph.rules[*id as usize]);
                    }
                }
                Instruction::Sequence(parts) => {
                    for part in parts {
                        pending.push(*part);
                        if !self.nullable(*part) {
                            break;
                        }
                    }
                }
                Instruction::Choice(parts) => pending.extend(parts.iter().copied()),
                Instruction::Label(_, inner) | Instruction::Repeat { inner, .. } => {
                    pending.push(*inner)
                }
                Instruction::Pratt { prefixes, .. } => {
                    pending.extend(prefixes.iter().map(|id| self.graph.rules[*id as usize]))
                }
            }
        }
        Ok(false)
    }
    fn nullable(&self, code: Code) -> bool {
        self.graph.nullable[code as usize]
    }
    fn recognized_end(&mut self, at: usize) -> Result<usize, Diagnostic> {
        let mut end = at;
        for text in &self.literal_tokens {
            end = end.max(self.literal_end(text, at).unwrap_or(at));
        }
        for id in &self.lexical {
            if let Some(hit) = self.probe.rule(*id, at, 0)? {
                end = end.max(hit.end);
            }
        }
        for expr in &self.character_tokens {
            if self.probe.predicate(expr, at)? {
                end = end.max(at + self.source[at..].chars().next().map_or(0, char::len_utf8));
            }
        }
        Ok(end)
    }
    fn skip_end(&mut self, at: usize) -> Result<usize, Diagnostic> {
        let end = self.recognized_end(at)?;
        Ok(if end > at {
            end
        } else {
            at + self.source[at..].chars().next().map_or(0, char::len_utf8)
        })
    }
    fn repair_base(&self, state: &mut State) {
        if state.cost == 0 {
            state.origin = state.position;
        }
        state.cost += 1;
        state.repairs += 1;
        state.clean = 0;
    }
    fn reopen_prefix(&mut self, state: &State) {
        let mut prefix = state.prefix;
        while let Some(index) = prefix {
            let checkpoint = &self.prefixes[index];
            prefix = checkpoint.state.prefix;
            if state.position <= checkpoint.furthest {
                let mut alternative = checkpoint.state.clone();
                alternative.reopen_through = Some(checkpoint.furthest);
                let task = Task::Rule {
                    id: checkpoint.id,
                    power: checkpoint.power,
                    label: checkpoint.label,
                    expand: true,
                };
                self.push(&mut alternative, task);
                self.enqueue(alternative);
            }
        }
    }
    fn terminal_failure(
        &mut self,
        state: State,
        retry: Task,
        expected: Expected,
    ) -> Result<(), Diagnostic> {
        let recognized = state.position == self.source.len()
            || self.recognized_end(state.position)? > state.position;
        self.reopen_prefix(&state);
        // An unrecognized scalar must be deleted in every viable continuation.
        // Inserting before it only adds cost without advancing recognition.
        if recognized {
            let mut insert = state.clone();
            self.repair_base(&mut insert);
            insert.missing += 1;
            let at = insert.position;
            self.event(&mut insert, Event::Missing { expected, at });
            if let Expected::Rule(id) = expected {
                self.event(
                    &mut insert,
                    Event::Open {
                        id,
                        start: at,
                        label: match retry {
                            Task::Rule { label, .. } => label,
                            _ => "",
                        },
                    },
                );
                self.event(&mut insert, Event::Close { end: at });
            }
            self.enqueue(insert);
        }
        self.delete(state, retry)
    }
    fn delete(&mut self, mut state: State, retry: Task) -> Result<(), Diagnostic> {
        let start = state.position;
        let recognized = self.recognized_end(start)?;
        let end = if recognized > start {
            recognized
        } else {
            start
                + self.source[start..]
                    .chars()
                    .next()
                    .map_or(0, char::len_utf8)
        };
        if end == start {
            return Ok(());
        }
        self.repair_base(&mut state);
        state.skipped += end - start;
        if recognized == start {
            state.mandatory_skipped += end - start;
        }
        self.event(&mut state, Event::Skip { start, end });
        state.position = end;
        self.push(&mut state, retry);
        self.enqueue(state);
        Ok(())
    }
    fn run(&mut self) -> Result<State, Diagnostic> {
        loop {
            let Some((_, index)) = self.queue.pop() else {
                if self.resynchronize()? {
                    continue;
                }
                break;
            };
            let mut state = self.states[index].clone();
            if let Err(error) = self.tick(&state) {
                if error.message == "recovery search state limit exceeded"
                    && self.resynchronize()?
                {
                    continue;
                }
                return Err(error);
            }
            if self.best.as_ref().is_none_or(|best| {
                state.position > best.position
                    || (state.position == best.position
                        && (state.cost, std::cmp::Reverse(state.origin))
                            < (best.cost, std::cmp::Reverse(best.origin)))
            }) {
                self.best = Some(state.clone());
            }
            let (task, next) = self.tasks.nodes[state.stack as usize];
            state.stack = next;
            match task {
                Task::End => {
                    self.trivia(&mut state);
                    if state.position == self.source.len() {
                        return Ok(state);
                    }
                    self.delete(state, task)?;
                    continue;
                }
                Task::Rule {
                    id,
                    power,
                    label,
                    mut expand,
                } => {
                    self.trivia(&mut state);
                    let rule = &self.language.rules[id as usize];
                    if !expand {
                        if let Some(hit) = self.probe.rule(id, state.position, power)? {
                            if !rule.lexical
                                && state.reopen_through.is_some_and(|target| {
                                    hit.end < target && hit.furthest >= target
                                })
                            {
                                expand = true;
                            }
                            if !expand {
                                if !rule.lexical && hit.furthest > hit.end {
                                    let checkpoint = Prefix {
                                        state: state.clone(),
                                        id,
                                        power,
                                        label,
                                        furthest: hit.furthest,
                                    };
                                    state.prefix = Some(self.prefixes.len());
                                    self.prefixes.push(checkpoint);
                                }
                                state.position = hit.end;
                                state.clean += 1;
                                self.event(
                                    &mut state,
                                    Event::Strict {
                                        root: hit.root,
                                        label,
                                    },
                                );
                                self.enqueue(state);
                                continue;
                            }
                        }
                    }
                    if rule.lexical {
                        self.terminal_failure(state, task, Expected::Rule(id))?;
                        continue;
                    }
                    let start = state.position;
                    let code = self.graph.rules[id as usize];
                    if let Instruction::Pratt { prefixes, .. } = &self.graph.code[code as usize] {
                        let prefixes = *prefixes;
                        for prefix in prefixes {
                            let mut branch = state.clone();
                            self.push(
                                &mut branch,
                                Task::Tail {
                                    point: id,
                                    power,
                                    label,
                                    start,
                                },
                            );
                            self.push(
                                &mut branch,
                                Task::Rule {
                                    id: *prefix,
                                    power: self.language.rules[*prefix as usize]
                                        .precedence
                                        .saturating_add(1),
                                    label,
                                    expand: false,
                                },
                            );
                            self.enqueue(branch);
                        }
                        continue;
                    }
                    self.event(&mut state, Event::Open { id, start, label });
                    self.push(&mut state, Task::Close { id, start, label });
                    // Only the direct final part inherits the Pratt operand power.
                    if let Instruction::Sequence(parts) = &self.graph.code[code as usize] {
                        let parts = parts.clone();
                        for (i, part) in parts.iter().enumerate().rev() {
                            self.expr(
                                &mut state,
                                *part,
                                (i + 1 == parts.len()).then_some(rule.owner).flatten(),
                                power,
                                "",
                            );
                        }
                    } else {
                        self.expr(&mut state, code, rule.owner, power, "");
                    }
                }
                Task::Close { id, start, .. } => {
                    let end = state.position;
                    self.event(&mut state, Event::Close { end });
                    if self.language.rules[id as usize]
                        .except
                        .contains(&&self.source[start..state.position])
                    {
                        continue;
                    }
                }
                Task::Expr {
                    code,
                    owner,
                    power,
                    label,
                } => {
                    match &self.graph.code[code as usize] {
                        Instruction::Ref(id) => self.push(
                            &mut state,
                            Task::Rule {
                                id: *id,
                                power: if owner == Some(*id) { power } else { 0 },
                                label,
                                expand: false,
                            },
                        ),
                        Instruction::Label(name, inner) => {
                            let (name, inner) = (*name, *inner);
                            self.expr(
                                &mut state,
                                inner,
                                owner,
                                power,
                                if label.is_empty() { name } else { label },
                            );
                        }
                        Instruction::Sequence(parts) => {
                            let parts = parts.clone();
                            for part in parts.iter().rev() {
                                self.expr(&mut state, *part, None, 0, label);
                            }
                        }
                        Instruction::Choice(parts) => {
                            let parts = parts.clone();
                            // Strict PEG choice commits to the first matching arm.
                            let mut selected = None;
                            for part in &parts {
                                if self.strict_expr(*part, state.position)?.is_some() {
                                    selected = Some(*part);
                                    break;
                                }
                            }
                            if let Some(part) = selected {
                                self.expr(&mut state, part, None, 0, label);
                            } else {
                                for part in parts {
                                    let mut branch = state.clone();
                                    self.expr(&mut branch, part, None, 0, label);
                                    self.enqueue(branch);
                                }
                                continue;
                            }
                        }
                        Instruction::Repeat { inner, min, max } => self.push(
                            &mut state,
                            Task::Repeat {
                                code: *inner,
                                count: 0,
                                min: *min,
                                max: *max,
                                label,
                            },
                        ),
                        Instruction::Predicate(expr) => {
                            let expr = *expr;
                            if !self.probe.predicate(expr, state.position)? {
                                continue;
                            }
                        }
                        Instruction::Pratt { .. } => {
                            return Err(Diagnostic::new(
                                "grammar",
                                [state.position as u32; 2],
                                "Pratt instruction outside a rule",
                            ));
                        }
                        Instruction::Terminal(expected, expr) => {
                            let (expected, expr) = (*expected, *expr);
                            self.trivia(&mut state);
                            let at = state.position;
                            let end = match expected {
                                Expected::Literal(text) => self.literal_end(text, at),
                                _ => self.probe.predicate(expr, at)?.then(|| {
                                    at + self.source[at..].chars().next().map_or(0, char::len_utf8)
                                }),
                            };
                            if let Some(end) = end {
                                state.position = end;
                                state.clean += u32::from(end > at);
                            } else {
                                self.terminal_failure(state, task, expected)?;
                                continue;
                            }
                        }
                    }
                }
                Task::Repeat {
                    code,
                    count,
                    min,
                    max,
                    label,
                } => {
                    let before_trivia = state.position;
                    self.trivia(&mut state);
                    let starts = count < max && self.starts(code, state.position)?;
                    if count >= min && !starts {
                        let mut stop = state.clone();
                        stop.position = before_trivia;
                        self.enqueue(stop);
                        if state.position < self.source.len() {
                            self.delete(state, task)?;
                        }
                        continue;
                    }
                    if count >= max {
                        state.position = before_trivia;
                        self.enqueue(state);
                        continue;
                    }
                    if count >= min && self.strict_expr(code, state.position)?.is_none() {
                        let mut stop = state.clone();
                        stop.position = before_trivia;
                        self.enqueue(stop);
                    }
                    let start = state.position;
                    let history = state.history;
                    self.push(
                        &mut state,
                        Task::AfterRepeat {
                            code,
                            count,
                            min,
                            max,
                            label,
                            start,
                            history,
                        },
                    );
                    self.expr(&mut state, code, None, 0, label);
                }
                Task::AfterRepeat {
                    code,
                    count,
                    min,
                    max,
                    label,
                    start,
                    history: _,
                } => {
                    if state.position == start && max > 1 {
                        continue;
                    }
                    // A nested list cannot validate repairs made before its element.
                    // Otherwise a conjectured outer branch can discard the true
                    // continuation after recognizing an unrelated inner item.
                    if max > 1
                        && state.clean > 0
                        && ((state.cost > 0 && state.origin >= start)
                            || (state.cost == 0 && self.queue.is_empty() && state.prefix.is_none()))
                    {
                        self.commit(&mut state);
                        state.cost = 0;
                        state.skipped = 0;
                        state.mandatory_skipped = 0;
                        state.missing = 0;
                        state.origin = state.position;
                        self.best = Some(state.clone());
                    }
                    self.push(
                        &mut state,
                        Task::Repeat {
                            code,
                            count: count + 1,
                            min,
                            max,
                            label,
                        },
                    );
                }
                Task::Tail {
                    point,
                    power,
                    label,
                    start,
                } => {
                    let before_trivia = state.position;
                    self.trivia(&mut state);
                    let Instruction::Pratt { infixes, .. } =
                        self.graph.code[self.graph.rules[point as usize] as usize]
                    else {
                        unreachable!()
                    };
                    let mut extended = false;
                    for id in infixes {
                        let rule = &self.language.rules[*id as usize];
                        if rule.precedence < power {
                            continue;
                        }
                        let Instruction::Sequence(parts) =
                            &self.graph.code[self.graph.rules[*id as usize] as usize]
                        else {
                            continue;
                        };
                        let parts = parts.clone();
                        if parts.len() < 2 || !self.starts(parts[1], state.position)? {
                            continue;
                        }
                        extended = true;
                        let mut branch = state.clone();
                        let left_label = match self.graph.code[parts[0] as usize] {
                            Instruction::Label(name, _) => name,
                            _ => "",
                        };
                        self.event(
                            &mut branch,
                            Event::Wrap {
                                id: *id,
                                start,
                                left_label,
                                label,
                            },
                        );
                        self.push(&mut branch, task);
                        self.push(
                            &mut branch,
                            Task::Close {
                                id: *id,
                                start,
                                label,
                            },
                        );
                        for (i, part) in parts.iter().enumerate().skip(1).rev() {
                            self.expr(
                                &mut branch,
                                *part,
                                if i + 1 == parts.len() {
                                    rule.owner
                                } else {
                                    None
                                },
                                rule.precedence
                                    .saturating_add(u32::from(!rule.right_associative)),
                                "",
                            );
                        }
                        self.enqueue(branch);
                    }
                    if extended {
                        continue;
                    }
                    state.position = before_trivia;
                }
            }
            self.enqueue(state);
        }
        Err(Diagnostic::new(
            "limit",
            [self.best.as_ref().map_or(0, |s| s.position) as u32; 2],
            "no repair within local recovery limits",
        ))
    }
    fn commit(&mut self, state: &mut State) {
        self.queue.clear();
        self.seen.clear();
        self.states.clear();
        self.epoch_states = 0;
        self.prefixes.clear();
        state.prefix = None;
        state.reopen_through = None;
        // Compact only the new selected suffix: rescanning the entire accepted
        // history at every error would make dense damage quadratic.
        let mut suffix = Vec::new();
        let mut h = state.history;
        while h > self.committed_history {
            let (previous, event) = self.history[h as usize];
            suffix.push((h, event));
            h = previous;
        }
        suffix.reverse();
        let mut roots: Vec<_> = suffix
            .iter()
            .filter_map(|(_, e)| match e {
                Event::Strict { root, .. } => Some(*root),
                _ => None,
            })
            .collect();
        self.probe.commit(&mut roots, state.position);
        let mut roots = roots.into_iter();
        self.history.truncate(self.committed_history as usize + 1);
        let mut remapped = HashMap::new();
        for (old, mut event) in suffix {
            if let Event::Strict { root, .. } = &mut event {
                *root = roots.next().unwrap();
            }
            let id = self.history.len() as u32;
            self.history.push((h, event));
            remapped.insert(old, id);
            h = id;
        }
        state.history = h;
        self.committed_history = h;
        // Only the selected continuation survives a stable list boundary.
        // Re-intern its spine so scratch states/tasks don't accumulate per byte.
        let mut spine = Vec::new();
        let mut stack = state.stack;
        while stack != 0 {
            let (task, next) = self.tasks.nodes[stack as usize];
            let task = match task {
                Task::AfterRepeat {
                    history,
                    code,
                    count,
                    min,
                    max,
                    label,
                    start,
                } => Task::AfterRepeat {
                    history: remapped.get(&history).copied().unwrap_or(history),
                    code,
                    count,
                    min,
                    max,
                    label,
                    start,
                },
                _ => task,
            };
            spine.push(task);
            stack = next;
        }
        self.tasks.nodes.truncate(1);
        self.tasks.known.clear();
        state.stack = 0;
        for task in spine.into_iter().rev() {
            state.stack = self.tasks.push(task, state.stack);
        }
        self.best = Some(state.clone());
    }
    fn continuation_starts(&mut self, mut stack: u32, at: usize) -> Result<bool, Diagnostic> {
        while stack != 0 {
            let (task, next) = self.tasks.nodes[stack as usize];
            match task {
                Task::End => return Ok(at == self.source.len()),
                Task::Expr { code, .. } => {
                    if self.starts(code, at)? && self.strict_expr(code, at)?.is_some() {
                        return Ok(true);
                    }
                    if !self.nullable(code) {
                        return Ok(false);
                    }
                }
                Task::Rule { id, power, .. } => {
                    return Ok(self.probe.rule(id, at, power)?.is_some());
                }
                Task::Repeat {
                    code, min, count, ..
                } => {
                    if self.starts(code, at)? && self.strict_expr(code, at)?.is_some() {
                        return Ok(true);
                    }
                    if count < min {
                        return Ok(false);
                    }
                }
                Task::Close { .. } | Task::Tail { .. } | Task::AfterRepeat { .. } => {}
            }
            stack = next;
        }
        Ok(at == self.source.len())
    }
    /// Search exhaustion discards only the current incomplete list element,
    /// then resumes at a strict element or a grammar-derived outer continuation.
    /// This is an explicit, measured fallback, not a minimum-cost repair claim.
    fn resynchronize(&mut self) -> Result<bool, Diagnostic> {
        let Some(best) = self.best.clone() else {
            return Ok(false);
        };
        let mut stack = best.stack;
        while stack != 0 {
            let (task, next) = self.tasks.nodes[stack as usize];
            if let Task::AfterRepeat {
                code,
                count,
                min,
                max,
                label,
                start,
                history,
            } = task
            {
                if max > 1 {
                    let mut at = best.position.max(start);
                    if at == start {
                        at += self.source[at..].chars().next().map_or(0, char::len_utf8);
                    }
                    let bound = self
                        .source
                        .len()
                        .min(at.saturating_add(self.options.max_local_bytes));
                    while at <= bound {
                        let mut cursor = best.clone();
                        cursor.position = at;
                        self.trivia(&mut cursor);
                        at = cursor.position;
                        if at > bound {
                            break;
                        }
                        self.stats.work += 1;
                        if self.stats.work.saturating_add(self.probe.work()) > self.budget() {
                            return Err(Diagnostic::new(
                                "limit",
                                [at as u32; 2],
                                "recovery work limit exceeded during resynchronization",
                            ));
                        }
                        let element =
                            self.starts(code, at)? && self.strict_expr(code, at)?.is_some();
                        let continuation = count >= min && self.continuation_starts(next, at)?;
                        if at > start && at < self.source.len() && (element || continuation) {
                            let mut resumed = best.clone();
                            resumed.history = history;
                            resumed.position = at;
                            resumed.stack = next;
                            resumed.cost = 0;
                            resumed.skipped = 0;
                            resumed.mandatory_skipped = 0;
                            resumed.missing = 0;
                            resumed.clean = 0;
                            resumed.origin = at;
                            self.event(&mut resumed, Event::Skip { start, end: at });
                            self.event(&mut resumed, Event::Exhausted { at: start });
                            self.push(
                                &mut resumed,
                                Task::Repeat {
                                    code,
                                    count,
                                    min,
                                    max,
                                    label,
                                },
                            );
                            self.queue.clear();
                            self.seen.clear();
                            self.epoch_states = 0;
                            self.stats.fallback_count += 1;
                            self.best = Some(resumed.clone());
                            self.enqueue(resumed);
                            return Ok(true);
                        }
                        if at == self.source.len() {
                            break;
                        }
                        let end = self.skip_end(at)?;
                        if end <= at {
                            break;
                        }
                        at = end;
                    }
                }
            }
            stack = next;
        }
        Ok(false)
    }
    fn strict_expr(&mut self, code: Code, at: usize) -> Result<Option<()>, Diagnostic> {
        // Used to preserve ordered choice at a failure expansion. Reconstructing
        // a full expression here is unnecessary: its original static Expr is
        // retained in Grammar for strict recognition.
        self.probe
            .predicate(self.graph.expressions[code as usize], at)
            .map(|yes| yes.then_some(()))
    }
    fn finish(mut self, state: State, failure: Option<Diagnostic>) -> RecoveredParse {
        let mut events = Vec::new();
        let mut history = state.history;
        while history > 0 {
            let (previous, event) = self.history[history as usize];
            events.push(event);
            history = previous;
        }
        events.reverse();
        let mut labels = vec![""];
        let mut label_ids = HashMap::from([("", 0u16)]);
        for label in self
            .probe
            .labels()
            .iter()
            .copied()
            .chain(events.iter().flat_map(|event| match *event {
                Event::Open { label, .. } | Event::Strict { label, .. } => [label, ""],
                Event::Wrap {
                    label, left_label, ..
                } => [label, left_label],
                _ => ["", ""],
            }))
        {
            if !label_ids.contains_key(label) {
                let Ok(id) = u16::try_from(labels.len()) else {
                    return super::tree::fallback(
                        self.language,
                        self.source,
                        Diagnostic::new("limit", [0, 0], "too many syntax edge labels"),
                    );
                };
                label_ids.insert(label, id);
                labels.push(label);
            }
        }
        let mut builder = Builder::new(self.language, labels, label_ids);
        let mut repairs = Vec::new();
        let mut exhausted = Vec::new();
        for event in events {
            match event {
                Event::Exhausted { at } => exhausted.push(Diagnostic::new(
                    "limit",
                    [at as u32; 2],
                    "local recovery search exhausted; resumed at a grammar continuation",
                )),
                Event::Open { id, start, label } => builder.frames.push(Frame {
                    id,
                    start,
                    label,
                    children: vec![],
                }),
                Event::Close { end } => builder.close(end),
                Event::Strict { root, label } => builder.strict(root, label, &self.probe),
                Event::Missing { expected, at } => repairs.push(Repair::Missing {
                    expected,
                    at: at as u32,
                }),
                Event::Skip { start, end } => repairs.push(Repair::Skipped {
                    span: [start as u32, end as u32],
                }),
                Event::Wrap {
                    id,
                    start,
                    left_label,
                    label,
                } => {
                    let child = builder
                        .frames
                        .last_mut()
                        .and_then(|f| f.children.pop())
                        .map(|mut e| {
                            e.label = builder.label_ids[left_label];
                            e
                        });
                    builder.frames.push(Frame {
                        id,
                        start,
                        label,
                        children: child.into_iter().collect(),
                    });
                }
            }
        }
        if failure.is_some() {
            self.stats.fallback_count += 1;
            if state.position < self.source.len() {
                repairs.push(Repair::Skipped {
                    span: [state.position as u32, self.source.len() as u32],
                });
            }
        }
        while builder.frames.len() > 1 {
            builder.close(state.position);
        }
        let mut roots = builder.frames.pop().unwrap().children;
        let root = if roots.len() == 1 {
            roots.remove(0).node
        } else {
            let id = builder.nodes.len() as u32;
            builder.nodes.push(Node {
                rule: self.language.start,
                span: [0, self.source.len() as u32],
                first_child: builder.edges.len() as u32,
                child_count: roots.len() as u32,
            });
            builder.edges.extend(roots);
            id
        };
        builder.nodes[root as usize].span = [0, self.source.len() as u32];
        let mut diagnostics = repair_diagnostics(&repairs, self.source);
        diagnostics.extend(exhausted);
        if let Some(error) = failure {
            diagnostics.push(error);
        }
        if diagnostics.is_empty() {
            diagnostics.push(Diagnostic::new(
                "syntax",
                [state.position as u32; 2],
                "incomplete syntax",
            ));
        }
        let missing: HashSet<_> = repairs
            .iter()
            .filter_map(|r| match r {
                Repair::Missing {
                    expected: Expected::Rule(rule),
                    at,
                } => Some((*rule, *at)),
                _ => None,
            })
            .collect();
        let damaged_nodes = builder
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.span[0] == n.span[1] && missing.contains(&(n.rule, n.span[0])))
            .map(|(id, _)| id as u32)
            .collect();
        let coverage = coverage(self.source, &repairs, &builder.nodes, self.language);
        self.stats.repair_count = repairs.len() as u64;
        self.stats.work += self.probe.work();
        builder.nodes.shrink_to_fit();
        builder.edges.shrink_to_fit();
        builder.labels.shrink_to_fit();
        RecoveredParse {
            tree: SyntaxTree {
                language: self.language,
                source: self.source.into(),
                nodes: builder.nodes,
                edges: builder.edges,
                labels: builder.labels,
                root,
                recovery: Some(Box::new(RecoveryData {
                    repairs,
                    coverage,
                    diagnostics: diagnostics.clone(),
                    damaged_nodes,
                })),
            },
            diagnostics,
            stats: self.stats,
        }
    }
}

struct Frame {
    id: u16,
    start: usize,
    label: &'static str,
    children: Vec<Edge>,
}
struct Builder {
    language: &'static Language,
    frames: Vec<Frame>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    labels: Vec<&'static str>,
    label_ids: HashMap<&'static str, u16>,
}
impl Builder {
    fn new(
        language: &'static Language,
        labels: Vec<&'static str>,
        label_ids: HashMap<&'static str, u16>,
    ) -> Self {
        Self {
            language,
            frames: vec![Frame {
                id: language.start,
                start: 0,
                label: "",
                children: vec![],
            }],
            nodes: vec![],
            edges: vec![],
            labels,
            label_ids,
        }
    }
    fn close(&mut self, end: usize) {
        if self.frames.len() < 2 {
            return;
        }
        let frame = self.frames.pop().unwrap();
        let rule = &self.language.rules[frame.id as usize];
        if rule.alias && frame.children.is_empty() && rule.owner.is_none() {
            return;
        }
        let node = if (rule.transparent || rule.alias) && frame.children.len() == 1 {
            frame.children[0].node
        } else {
            let node = self.nodes.len() as u32;
            self.nodes.push(Node {
                rule: frame.id,
                span: [frame.start as u32, end as u32],
                first_child: self.edges.len() as u32,
                child_count: frame.children.len() as u32,
            });
            self.edges.extend(frame.children);
            node
        };
        self.frames.last_mut().unwrap().children.push(Edge {
            label: self.label_ids[frame.label],
            node,
        });
    }
    fn strict(&mut self, root: u32, label: &'static str, probe: &StrictProbe<'_>) {
        if root == u32::MAX {
            return;
        }
        let mut pending = vec![(root, false)];
        let mut mapped = HashMap::new();
        while let Some((old, visited)) = pending.pop() {
            let n = probe.nodes()[old as usize];
            let children =
                &probe.edges()[n.first_child as usize..(n.first_child + n.child_count) as usize];
            if !visited {
                pending.push((old, true));
                pending.extend(children.iter().rev().map(|e| (e.node, false)));
            } else {
                let node = self.nodes.len() as u32;
                self.nodes.push(Node {
                    first_child: self.edges.len() as u32,
                    ..n
                });
                self.edges.extend(children.iter().map(|e| Edge {
                    label: self.label_ids[probe.label(e)],
                    node: mapped[&e.node],
                }));
                mapped.insert(old, node);
            }
        }
        self.frames.last_mut().unwrap().children.push(Edge {
            label: self.label_ids[label],
            node: mapped[&root],
        });
    }
}

fn repair_diagnostics(repairs: &[Repair], source: &str) -> Vec<Diagnostic> {
    let mut result: Vec<Diagnostic> = vec![];
    for repair in repairs {
        let (span, message) = match repair {
            Repair::Missing { expected, at } => ([*at, *at], format!("missing {expected:?}")),
            Repair::Skipped { span } => (*span, "unexpected source".into()),
        };
        if let Some(previous) = result.last_mut() {
            if previous.span[1] <= span[0]
                && source[previous.span[1] as usize..span[0] as usize]
                    .chars()
                    .all(char::is_whitespace)
            {
                previous.span[1] = span[1];
                continue;
            }
        }
        result.push(Diagnostic::new("syntax", span, message));
    }
    result
}

fn coverage(
    source: &str,
    repairs: &[Repair],
    nodes: &[Node],
    language: &Language,
) -> Vec<Coverage> {
    let mut skipped: Vec<_> = repairs
        .iter()
        .filter_map(|r| match r {
            Repair::Skipped { span } => Some(*span),
            _ => None,
        })
        .collect();
    skipped.sort_unstable();
    let mut tokens: Vec<_> = nodes
        .iter()
        .filter(|n| language.rules[n.rule as usize].lexical && n.span[0] < n.span[1])
        .map(|n| n.span)
        .collect();
    tokens.sort_unstable();
    let (mut sk, mut tk, mut at) = (0, 0, 0usize);
    let mut result: Vec<Coverage> = vec![];
    while at < source.len() {
        while sk < skipped.len() && skipped[sk][1] as usize <= at {
            sk += 1;
        }
        while tk < tokens.len() && tokens[tk][1] as usize <= at {
            tk += 1;
        }
        let (end, kind) = if sk < skipped.len() && skipped[sk][0] as usize <= at {
            (skipped[sk][1] as usize, CoverageKind::Skipped)
        } else if tk < tokens.len() && tokens[tk][0] as usize <= at {
            (tokens[tk][1] as usize, CoverageKind::Retained)
        } else {
            let rest = &source[at..];
            if rest.starts_with("//") {
                (
                    at + rest.find('\n').unwrap_or(rest.len()),
                    CoverageKind::Trivia,
                )
            } else if rest.starts_with("/*") && rest[2..].contains("*/") {
                (at + rest[2..].find("*/").unwrap() + 4, CoverageKind::Trivia)
            } else {
                let c = rest.chars().next().unwrap();
                (
                    at + c.len_utf8(),
                    if c.is_whitespace() {
                        CoverageKind::Trivia
                    } else {
                        CoverageKind::Retained
                    },
                )
            }
        };
        // A skipped range always wins, even when it falls inside a lexical span.
        let end = if sk < skipped.len() && skipped[sk][0] as usize > at {
            end.min(skipped[sk][0] as usize)
        } else {
            end
        };
        if let Some(last) = result.last_mut().filter(|last| last.kind == kind) {
            last.span[1] = end as u32;
        } else {
            result.push(Coverage {
                span: [at as u32, end as u32],
                kind,
            });
        }
        at = end;
    }
    result
}
