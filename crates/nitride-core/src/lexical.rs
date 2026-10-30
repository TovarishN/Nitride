//! Runtime support for generated lexical scanners.

use crate::Language;
mod runs;
pub use runs::AsciiRun;

/// Exact generated trace for one ASCII first-byte decision. Zero work declines.
#[derive(Clone, Copy)]
pub struct AsciiDecision(u64);
impl AsciiDecision {
    pub const UNKNOWN: Self = Self(0);
    /// `depth` is relative to the expression depth supplied at replay. No rule
    /// entry guard is represented here; generated callbacks retain those calls.
    pub const fn new(work: u16, depth: u8, observed: u32, matched: bool, advance: bool) -> Self {
        Self(u64::from_le_bytes([
            work as u8,
            (work >> 8) as u8,
            depth,
            observed as u8,
            (observed >> 8) as u8,
            (observed >> 16) as u8,
            (observed >> 24) as u8,
            (advance as u8) | ((matched as u8) << 1),
        ]))
    }
}

pub type LexicalScanner = fn(&mut LexicalContext<'_>, u32) -> Option<bool>;
/// A generated table bound to its unmodified static grammar.
pub struct CompiledLexers {
    pub(crate) language: &'static Language<'static>,
    pub(crate) scanners: &'static [Option<LexicalScanner>],
}
impl CompiledLexers {
    pub const fn new(
        language: &'static Language<'static>,
        scanners: &'static [Option<LexicalScanner>],
    ) -> Self {
        Self { language, scanners }
    }
}

/// Local, unpublished scanner state. `None` aborts specialization; ordinary
/// mismatch is `Some(false)`. Restoring a position never discards observations.
pub struct LexicalContext<'a> {
    source: &'a str,
    position: usize,
    pub(crate) observed: u64,
    pub(crate) work: u64,
    pub(crate) max_depth: u32,
    work_budget: u64,
    frames: u32,
    stack_base: usize,
    stack_budget: usize,
}
#[inline(never)]
fn stack_position() -> usize {
    let marker = 0u8;
    &marker as *const u8 as usize
}
impl<'a> LexicalContext<'a> {
    pub(crate) fn new(
        source: &'a str,
        position: usize,
        work_budget: u64,
        frames: u32,
        stack_base: usize,
        stack_budget: usize,
    ) -> Self {
        Self {
            source,
            position,
            observed: position as u64,
            work: 0,
            max_depth: 0,
            work_budget,
            frames,
            stack_base,
            stack_budget,
        }
    }
    #[inline]
    pub fn step(&mut self, depth: u32) -> Option<()> {
        self.work = self.work.checked_add(1)?;
        self.max_depth = self.max_depth.max(depth);
        (depth <= self.frames && self.work <= self.work_budget).then_some(())
    }
    #[inline]
    pub fn enter(&mut self, depth: u32) -> Option<()> {
        self.step(depth)?;
        (stack_position().abs_diff(self.stack_base) <= self.stack_budget).then_some(())
    }
    /// Replay only when the whole trace can complete. Declining leaves every
    /// field unchanged so the ordinary expression reproduces partial failures.
    #[inline]
    pub fn ascii_decision(&mut self, entries: &[AsciiDecision; 128], depth: u32) -> Option<bool> {
        let byte = *self.source.as_bytes().get(self.position)?;
        if !byte.is_ascii() {
            return None;
        }
        let entry = entries[byte as usize].0;
        let work = entry & 0xffff;
        if work == 0 {
            return None;
        }
        let advance = ((entry >> 56) & 1) as usize;
        let matched = entry & (1 << 57) != 0;
        let observed = ((entry >> 24) & 0xffff_ffff) as usize;
        if advance != 0 && (!matched || observed == 0) {
            return None;
        }
        let maximum = depth.checked_add(((entry >> 16) & 0xff) as u32)?;
        let total = self.work.checked_add(work)?;
        if maximum > self.frames || total > self.work_budget {
            return None;
        }
        self.work = total;
        self.max_depth = self.max_depth.max(maximum);
        self.observe(self.position.saturating_add(observed));
        self.position += advance;
        Some(matched)
    }
    #[inline]
    pub fn position(&self) -> usize {
        self.position
    }
    /// Replay complete successful one-byte iterations. Decline short prefixes,
    /// unavailable depth/work headroom, and invalid metadata without mutation.
    /// The caller must run the ordinary body at the first unconsumed byte.
    #[inline]
    pub fn ascii_run(&mut self, plan: &AsciiRun, maximum: u32, depth: u32) -> u32 {
        if !cfg!(feature = "ascii-runs") {
            return 0;
        }
        let bytes = &self.source.as_bytes()[self.position..];
        if maximum < 8 || plan.work == 0 || !plan.prefix(bytes) {
            return 0;
        }
        let Some(max_depth) = depth.checked_add(u32::from(plan.depth)) else {
            return 0;
        };
        if max_depth > self.frames {
            return 0;
        }
        let budget = self.work_budget.saturating_sub(self.work) / u64::from(plan.work);
        let length = bytes
            .len()
            .min(maximum as usize)
            .min(budget.min(usize::MAX as u64) as usize);
        if length == 0 {
            return 0;
        }
        let count = plan.scan(&bytes[..length]);
        self.position += count;
        self.work += count as u64 * u64::from(plan.work);
        self.max_depth = self.max_depth.max(max_depth);
        self.observe(self.position);
        count as u32
    }
    #[inline]
    pub fn restore(&mut self, position: usize) {
        // Generated marks originate here. Keep malformed host callbacks safe.
        if self.source.is_char_boundary(position) {
            self.position = position;
        }
    }
    #[inline]
    fn observe(&mut self, end: usize) {
        self.observed = self.observed.max(end.min(self.source.len() + 1) as u64);
    }
    #[inline]
    pub fn literal(&mut self, text: &str) -> bool {
        self.observe(self.position.saturating_add(text.len()));
        if self.source[self.position..].starts_with(text) {
            self.position += text.len();
            true
        } else {
            false
        }
    }
    #[inline]
    fn character(&mut self) -> Option<char> {
        let c = self.source[self.position..].chars().next();
        self.observe(self.position.saturating_add(c.map_or(1, char::len_utf8)));
        c
    }
    #[inline]
    pub fn any(&mut self) -> bool {
        if let Some(c) = self.character() {
            self.position += c.len_utf8();
            true
        } else {
            false
        }
    }
    #[inline]
    pub fn class(&mut self, ascii: [u64; 2], ranges: &[(u32, u32)], negated: bool) -> bool {
        let Some(c) = self.character() else {
            return false;
        };
        let hit = if c.is_ascii() {
            ascii[c as usize / 64] & (1u64 << (c as usize % 64)) != 0
        } else {
            let at = ranges.partition_point(|(_, b)| *b < c as u32);
            ranges.get(at).is_some_and(|(a, _)| *a <= c as u32)
        };
        if hit != negated {
            self.position += c.len_utf8();
            true
        } else {
            false
        }
    }
    /// `excluded` over spellings sorted for binary search, with bit `n` of
    /// `lengths` set when some spelling is `n` bytes long (all are below 64).
    /// Same answer and observations: only an equal-length spelling observes.
    #[inline]
    pub fn excluded_sorted(&mut self, start: usize, lengths: u64, sorted: &[&str]) -> bool {
        let length = self.position.saturating_sub(start);
        if length >= 64 || lengths & (1 << length) == 0 {
            return false;
        }
        self.observe(start.saturating_add(length));
        self.source
            .get(start..self.position)
            .is_some_and(|text| sorted.binary_search(&text).is_ok())
    }
    #[inline]
    pub fn excluded(&mut self, start: usize, spellings: &[&str]) -> bool {
        spellings.iter().any(|text| {
            if text.len() != self.position.saturating_sub(start) {
                return false;
            }
            self.observe(start.saturating_add(text.len()));
            self.source.get(start..self.position) == Some(*text)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context(source: &str, work: u64, frames: u32) -> LexicalContext<'_> {
        LexicalContext::new(source, 0, work, frames, stack_position(), 1024 * 1024)
    }
    #[test]
    fn observations_include_failed_literals_lookahead_and_eof() {
        let mut c = context("éx", 100, 100);
        assert!(!c.literal("éy"));
        assert_eq!(c.observed, 3);
        assert_eq!(c.position(), 0);
        assert!(c.literal("é"));
        let mark = c.position();
        assert!(c.any());
        assert!(!c.any());
        c.restore(mark);
        assert_eq!(c.position(), 2);
        assert_eq!(c.observed, 4);
        assert!(c.excluded(0, &["é"]));
        assert!(!c.excluded(0, &["éx"]));
    }
    #[test]
    fn classes_distinguish_ascii_unicode_negation_and_eof() {
        let mut c = context("aé😀", 100, 100);
        assert!(c.class([0, 1 << (b'a' - 64)], &[], false));
        assert!(c.class([0, 0], &[(233, 233), (0x1f600, 0x1f600)], false));
        assert!(c.class([0, 0], &[(233, 233)], true));
        assert!(!c.class([0, 0], &[], true));
        assert_eq!(c.observed, 8);
    }
    #[test]
    fn work_depth_and_native_stack_exhaustion_abort() {
        let mut c = context("abc", 2, 3);
        assert_eq!(c.step(3), Some(()));
        assert_eq!(c.step(2), Some(()));
        assert_eq!(c.step(0), None);
        assert_eq!(context("", 100, 3).step(4), None);
        let mut c = context("", 100, 3);
        c.stack_budget = 0;
        assert_eq!(c.enter(0), None);
    }
    fn state(c: &LexicalContext<'_>) -> (usize, u64, u64, u32) {
        (c.position, c.observed, c.work, c.max_depth)
    }
    fn repeated_lower(c: &mut LexicalContext<'_>, max: u32, depth: u32, fast: bool) -> Option<u32> {
        let mask = [0, ((1u64 << 26) - 1) << (b'a' - 64)];
        let plan = AsciiRun::new(mask, 2, 1);
        let mut count = 0;
        while count < max {
            if fast {
                count += c.ascii_run(&plan, max - count, depth);
                if count == max {
                    break;
                }
            }
            c.step(depth)?;
            c.step(depth + 1)?;
            if !c.class(mask, &[], false) {
                break;
            }
            count += 1;
        }
        Some(count)
    }
    #[test]
    fn ascii_runs_preserve_partial_work_depth_observations_and_repeat_limits() {
        for suffix in ["", "!", "é", "😀", "\\u0061", "\0"] {
            for length in [0, 1, 7, 8, 9, 31, 32, 33, 63, 64, 65] {
                let source = format!("zz{}{suffix}", "a".repeat(length));
                for work in 0..=140 {
                    for frames in [0, 3, 4, 5] {
                        for maximum in [0, 1, 7, 8, 9, 31, 32, 33, 64, 100] {
                            let make = || {
                                let mut c = context(&source, work, frames);
                                c.position = 2;
                                c.work = 5;
                                c.observed = 3;
                                c.max_depth = 2;
                                c
                            };
                            let mut fast = make();
                            let mut scalar = make();
                            let result = repeated_lower(&mut fast, maximum, 3, true);
                            let expected = repeated_lower(&mut scalar, maximum, 3, false);
                            assert_eq!(
                                (result, state(&fast)),
                                (expected, state(&scalar)),
                                "len {length} suffix {suffix:?} work {work} frames {frames} max {maximum}"
                            );
                        }
                    }
                }
            }
        }
        let plan = AsciiRun::new([u64::MAX; 2], 1, 1);
        let mut c = context("aaaaaaaa", u64::MAX, u32::MAX);
        let before = state(&c);
        assert_eq!(c.ascii_run(&plan, 8, u32::MAX), 0);
        assert_eq!(state(&c), before);
        c.work = u64::MAX - 3;
        if cfg!(feature = "ascii-runs") {
            assert_eq!(c.ascii_run(&plan, 8, 0), 3);
            assert_eq!(c.work, u64::MAX);
        } else {
            assert_eq!(c.ascii_run(&plan, 8, 0), 0);
            assert_eq!(c.work, u64::MAX - 3);
        }
    }
    fn decision_table() -> [AsciiDecision; 128] {
        std::array::from_fn(|byte| {
            if byte == usize::from(b'\\') {
                AsciiDecision::UNKNOWN
            } else {
                let hit = (byte as u8).is_ascii_lowercase();
                AsciiDecision::new(6, 3, 4, hit, hit)
            }
        })
    }
    fn reference_decision(c: &mut LexicalContext<'_>, d: u32) -> Option<bool> {
        c.step(d)?;
        let mark = c.position();
        c.step(d + 1)?;
        if c.literal("😀") {
            return Some(true);
        }
        c.restore(mark);
        c.step(d + 1)?;
        c.step(d + 2)?;
        let pred = c.position();
        c.step(d + 3)?;
        let hit = c.literal("\\u");
        c.restore(pred);
        if !hit {
            c.step(d + 2)?;
            if c.class([0, ((1u64 << 26) - 1) << (b'a' - 64)], &[], false) {
                return Some(true);
            }
        }
        c.restore(mark);
        Some(false)
    }
    #[test]
    fn ascii_decisions_match_reference_at_every_work_and_frame_boundary() {
        let table = decision_table();
        for byte in 0..128u8 {
            let source = format!("zz{}é", char::from(byte));
            for work in 5..=12 {
                for frames in 7..=12 {
                    let make = || {
                        let mut c = context(&source, work, frames);
                        c.position = 2;
                        c.work = 5;
                        c.max_depth = 2;
                        c.observed = 3;
                        c
                    };
                    let mut fast = make();
                    let mut old = make();
                    let before = state(&fast);
                    let fused = fast.ascii_decision(&table, 8);
                    if fused.is_none() {
                        assert_eq!(state(&fast), before);
                    }
                    let result = fused.or_else(|| reference_decision(&mut fast, 8));
                    let expected = reference_decision(&mut old, 8);
                    assert_eq!(
                        (result, state(&fast)),
                        (expected, state(&old)),
                        "byte{byte} work{work} frames{frames}"
                    );
                }
            }
        }
    }
    #[test]
    fn ascii_decisions_decline_unicode_eof_unknown_and_unsafe_metadata() {
        let table = decision_table();
        for source in ["", "é", "😀", "\\u", "a"] {
            let mut fast = context(source, 100, 100);
            let mut old = context(source, 100, 100);
            let result = fast
                .ascii_decision(&table, 0)
                .or_else(|| reference_decision(&mut fast, 0));
            assert_eq!(
                (result, state(&fast)),
                (reference_decision(&mut old, 0), state(&old))
            );
        }
        for entry in [
            AsciiDecision::new(6, 3, 4, false, true),
            AsciiDecision::new(6, 3, 0, true, true),
            AsciiDecision::new(0, 255, u32::MAX, true, true),
        ] {
            let mut c = context("a", 100, 100);
            let before = state(&c);
            assert_eq!(c.ascii_decision(&[entry; 128], 0), None);
            assert_eq!(state(&c), before);
        }
    }
    #[test]
    fn ascii_decisions_preserve_clipping_exact_limits_and_checked_arithmetic() {
        let table = decision_table();
        let mut c = context("a", 6, 3);
        assert_eq!(c.ascii_decision(&table, 0), Some(true));
        assert_eq!(state(&c), (1, 2, 6, 3));
        let leaf = [AsciiDecision::new(1, 0, 1, true, true); 128];
        let mut c = context("a", 1, 0);
        assert_eq!(c.ascii_decision(&leaf, 0), Some(true));
        assert_eq!(state(&c), (1, 1, 1, 0));
        let mut c = context("a", u64::MAX, u32::MAX);
        c.work = u64::MAX - 2;
        let before = state(&c);
        assert_eq!(c.ascii_decision(&table, 0), None);
        assert_eq!(state(&c), before);
        let mut c = context("a", 100, u32::MAX);
        let before = state(&c);
        assert_eq!(c.ascii_decision(&table, u32::MAX), None);
        assert_eq!(state(&c), before);
    }
}
