//! Bounded token and trivia lookahead caches.
/// One trivia-skipped position and the token reads already made from it.
pub(super) struct Peek {
    pub(super) at: usize,
    pub(super) character: Option<char>,
    after: [(u32, u32, Option<char>); 4],
    after_len: usize,
}
impl Peek {
    pub(super) fn new(at: usize, character: Option<char>) -> Self {
        Self {
            at,
            character,
            after: [(0, 0, None); 4],
            after_len: 0,
        }
    }
    pub(super) fn after(&self, end: usize) -> Option<(usize, Option<char>)> {
        self.after[..self.after_len]
            .iter()
            .find(|(known, _, _)| *known as usize == end)
            .map(|(_, next, character)| (*next as usize, *character))
    }
    pub(super) fn remember_after(&mut self, end: usize, (next, character): (usize, Option<char>)) {
        if self.after_len < self.after.len() {
            self.after[self.after_len] = (end as u32, next as u32, character);
            self.after_len += 1;
        }
    }
}

/// Token reads shared by every prediction at a position: identifier word
/// extents and lexical rule ends, each with the observation it made. The
/// source never changes during a parse, so entries never go stale; slots are
/// direct-mapped by position and simply overwritten.
#[derive(Default)]
pub(super) struct TokenCache {
    /// Allocated on first use: small edits often read few positions.
    slots: Vec<TokenSlot>,
    /// Slots to allocate; `TOKEN_SLOTS` when zero.
    size: usize,
}
#[derive(Clone, Copy)]
struct TokenSlot {
    at: u32,
    /// Word end and the observation that found it; `u32::MAX` when unknown.
    word: u32,
    word_observed: u32,
    /// End of trivia from here, its observation and logical steps;
    /// `u32::MAX` when unknown.
    trivia: u32,
    trivia_observed: u32,
    trivia_steps: u32,
    /// Lexical rule, end (`u32::MAX` for no match) and observation.
    lexical: [(u16, u32, u32); 3],
    lexical_len: u8,
}
/// Backtracking revisits recent positions; a short window covers it.
const TOKEN_SLOTS: usize = 512;
impl TokenSlot {
    const EMPTY: Self = Self {
        at: u32::MAX,
        word: u32::MAX,
        word_observed: 0,
        trivia: u32::MAX,
        trivia_observed: 0,
        trivia_steps: 0,
        lexical: [(0, 0, 0); 3],
        lexical_len: 0,
    };
}
impl TokenCache {
    /// A text of `len` bytes has `len + 1` positions. While those fit, a
    /// smaller power-of-two table maps each to the slot the full table
    /// would, so contents and hits are the same with less to allocate.
    pub(super) fn for_text(len: usize) -> Self {
        Self {
            slots: Vec::new(),
            size: len.saturating_add(1).next_power_of_two().min(TOKEN_SLOTS),
        }
    }
    fn get(&self, at: usize) -> Option<&TokenSlot> {
        self.slots
            // Power-of-two length; an empty table's mask finds nothing.
            .get(at & self.slots.len().wrapping_sub(1))
            .filter(|slot| slot.at == at as u32)
    }
    fn slot(&mut self, at: usize) -> &mut TokenSlot {
        if self.slots.is_empty() {
            let size = if self.size == 0 {
                TOKEN_SLOTS
            } else {
                self.size
            };
            self.slots = vec![TokenSlot::EMPTY; size];
        }
        let mask = self.slots.len() - 1;
        let slot = &mut self.slots[at & mask];
        if slot.at != at as u32 {
            *slot = TokenSlot {
                at: at as u32,
                ..TokenSlot::EMPTY
            };
        }
        slot
    }
    pub(super) fn trivia(&self, at: usize) -> Option<(usize, u64, u32)> {
        let slot = self.get(at).filter(|slot| slot.trivia != u32::MAX)?;
        Some((
            slot.trivia as usize,
            u64::from(slot.trivia_observed),
            slot.trivia_steps,
        ))
    }
    pub(super) fn remember_trivia(&mut self, at: usize, end: usize, observed: u64, steps: u32) {
        let slot = self.slot(at);
        slot.trivia = end as u32;
        slot.trivia_observed = observed.min(u64::from(u32::MAX)) as u32;
        slot.trivia_steps = steps;
    }
    pub(super) fn lexical(&mut self, at: usize, rule: u16) -> Option<(Option<usize>, u64)> {
        let slot = self.get(at)?;
        slot.lexical[..slot.lexical_len as usize]
            .iter()
            .find(|(known, _, _)| *known == rule)
            .map(|(_, end, observed)| {
                (
                    (*end != u32::MAX).then_some(*end as usize),
                    u64::from(*observed),
                )
            })
    }
    pub(super) fn remember_lexical(
        &mut self,
        at: usize,
        rule: u16,
        end: Option<usize>,
        observed: u64,
    ) {
        let slot = self.slot(at);
        let entry = (
            rule,
            end.map_or(u32::MAX, |end| end as u32),
            observed.min(u64::from(u32::MAX)) as u32,
        );
        if (slot.lexical_len as usize) < slot.lexical.len() {
            slot.lexical[slot.lexical_len as usize] = entry;
            slot.lexical_len += 1;
        } else {
            slot.lexical.rotate_left(1);
            slot.lexical[2] = entry;
        }
    }
    /// The end of the identifier word at `at` (`at` when none), and the
    /// observation: through the first character after the word.
    pub(super) fn word(&mut self, source: &str, at: usize) -> (usize, u64) {
        let slot = self.slot(at);
        if slot.word != u32::MAX {
            return (slot.word as usize, u64::from(slot.word_observed));
        }
        let mut end = at;
        let mut observed = None;
        for (offset, c) in source.get(at..).unwrap_or("").char_indices() {
            if !(c.is_alphanumeric() || c == '_') {
                observed = Some(at + offset + c.len_utf8());
                break;
            }
            end = at + offset + c.len_utf8();
        }
        let observed = observed.unwrap_or(source.len() + 1);
        slot.word = end as u32;
        slot.word_observed = observed as u32;
        (end, observed as u64)
    }
}
