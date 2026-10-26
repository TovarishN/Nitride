//! Runtime bridge for generated structural syntax bodies. Storage and rule
//! dispatch stay with Parser; callbacks never publish their own nodes.
use super::*;

pub type SyntaxBody = fn(&mut SyntaxContext<'_, '_>, u32, u32) -> Result<bool, Diagnostic>;
/// Generated callbacks paired with their unchanged language/lexical table.
pub struct CompiledParser {
    pub(crate) lexers: &'static crate::lexical::CompiledLexers,
    pub(crate) bodies: &'static [Option<SyntaxBody>],
    pub(crate) tails: &'static [Option<SyntaxBody>],
}
impl CompiledParser {
    pub const fn new(
        lexers: &'static crate::lexical::CompiledLexers,
        bodies: &'static [Option<SyntaxBody>],
        tails: &'static [Option<SyntaxBody>],
    ) -> Self {
        Self {
            lexers,
            bodies,
            tails,
        }
    }
}
/// A mark belongs to the parser/context that created it.
#[derive(Clone, Copy)]
pub struct SyntaxMark {
    mark: Mark,
    len: usize,
    checkpoint: bool,
}
/// Borrowed access to the current parser and child buffer. This is a concrete
/// context so generated primitive calls can inline without virtual dispatch.
pub struct SyntaxContext<'p, 's> {
    parser: &'p mut Parser<'s>,
    children: &'p mut Children,
}
impl<'p, 's> SyntaxContext<'p, 's> {
    pub(super) fn new(parser: &'p mut Parser<'s>, children: &'p mut Children) -> Self {
        Self { parser, children }
    }
    /// Check the generated function's actual stack frame without charging an
    /// additional logical expression. Recursive rule calls retain their checks.
    #[inline]
    pub fn guard(&self) -> Result<(), Diagnostic> {
        if super::super::stack_position().abs_diff(self.parser.stack_base)
            > self.parser.stack_budget
        {
            return Err(Diagnostic::new(
                "limit",
                [self.parser.position as u32; 2],
                super::super::STACK_EXHAUSTED,
            ));
        }
        Ok(())
    }
    #[inline]
    pub fn step(&mut self, depth: u32) -> Result<(), Diagnostic> {
        self.parser.work += 1;
        self.parser.max_depth = self.parser.max_depth.max(depth);
        if depth > self.parser.limits.parser_frames || self.parser.work > self.parser.work_budget {
            return Err(Diagnostic::new(
                "limit",
                [self.parser.position as u32; 2],
                "parser depth/work limit exceeded",
            ));
        }
        Ok(())
    }
    #[inline]
    pub fn finish(&mut self, matched: bool) -> bool {
        if !matched {
            self.parser.furthest = self.parser.furthest.max(self.parser.position);
        }
        matched
    }
    #[inline]
    pub fn literal(&mut self, text: &str) -> Result<bool, Diagnostic> {
        self.parser.trivia()?;
        let keyword = text
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            && text.chars().all(|c| c.is_alphanumeric() || c == '_');
        let end = self.parser.position + text.len();
        let boundary = !keyword
            || self
                .parser
                .source
                .character(end)
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        if self.parser.source.matches(self.parser.position, text) && boundary {
            self.parser.position = end;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    #[inline]
    pub fn reference(&mut self, id: u16, minimum: u32, depth: u32) -> Result<bool, Diagnostic> {
        self.parser.trivia()?;
        if let Some(node) = self.parser.rule(id, minimum, depth + 1)? {
            if node != NO_NODE {
                self.children.push(
                    Edge { label: 0, node },
                    self.parser
                        .reusable
                        .previous_position(node.start + node.width),
                );
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
    #[inline]
    pub fn children_len(&self) -> usize {
        self.children.len()
    }
    #[inline]
    pub fn label(
        &mut self,
        label: &'static str,
        from: usize,
        depth: u32,
    ) -> Result<(), Diagnostic> {
        if self.children.len() > from {
            let label = self.parser.intern(label)?;
            self.parser.work = self
                .parser
                .work
                .saturating_add((self.children.len() - from) as u64);
            self.parser.tick(depth)?;
            self.children.label(from, label);
        }
        Ok(())
    }
    #[inline]
    pub fn begin_iteration(&mut self, checkpoint: bool) -> SyntaxMark {
        let mark = SyntaxMark {
            mark: self.parser.mark(),
            len: self.children.len(),
            checkpoint: self.parser.checkpoint_next,
        };
        if checkpoint {
            self.parser.checkpoint_next = true;
        }
        mark
    }
    #[inline]
    pub fn end_iteration(
        &mut self,
        mark: SyntaxMark,
        result: Result<bool, Diagnostic>,
        require_progress: bool,
    ) -> Result<bool, Diagnostic> {
        self.parser.checkpoint_next = mark.checkpoint;
        if !result? {
            self.parser.restore(mark.mark);
            self.children.truncate(mark.len);
            return Ok(false);
        }
        if require_progress && self.parser.position == mark.mark.position {
            return Err(Diagnostic::new(
                "grammar",
                [self.parser.position as u32; 2],
                "zero-width repetition",
            ));
        }
        Ok(true)
    }
    #[inline]
    pub fn interpret(
        &mut self,
        expr: &'static Expr<'static>,
        owner: Option<u16>,
        minimum: u32,
        depth: u32,
    ) -> Result<bool, Diagnostic> {
        self.parser
            .expression(expr, false, owner, minimum, depth, self.children)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static LANGUAGE: Language = Language {
        profile: Profile::SyntaxOnly,
        start: 0,
        types: &[],
        operations: &[],
        handlers: &[],
        checks: &[],
        helpers: &[],
        source_hash: "syntax-context-test",
        rules: &[Rule {
            name: "Item",
            lexical: false,
            expr: Expr::Literal("x"),
            except: &[],
            precedence: 0,
            right_associative: false,
            transparent: false,
            alias: false,
            owner: None,
            action: Action::None,
        }],
    };
    fn parser<'a>(text: &'a SourceText, source: &'a str) -> Parser<'a> {
        Parser::new(
            &LANGUAGE,
            Input {
                compiled: None,
                lexers: None,
                text,
                contiguous: Some(source),
            },
            Limits::default(),
            ParseOptions::default(),
            1024 * 1024,
            None,
            Index::default(),
        )
    }
    #[test]
    fn repeat_rollback_restores_children_and_checkpoint_but_retains_reads_and_work() {
        let text = SourceText::new("x;").unwrap();
        let mut p = parser(&text, "x;");
        p.checkpoint_next = false;
        let mut out = Children::new(0, None);
        let mut c = SyntaxContext::new(&mut p, &mut out);
        let mark = c.begin_iteration(true);
        assert!(c.parser.checkpoint_next);
        assert!(c.reference(0, 0, 1).unwrap());
        assert_eq!(c.children_len(), 1);
        assert!(!c.literal("long").unwrap());
        let before = c.parser.work;
        c.label("value", 0, 1).unwrap();
        assert_eq!(c.parser.work, before + 2);
        let label = c.children.iter().next().unwrap().label;
        assert_eq!(c.parser.labels[label as usize], "value");
        c.finish(false);
        let work = c.parser.work;
        assert!(!c.end_iteration(mark, Ok(false), true).unwrap());
        assert_eq!(c.parser.position, 0);
        assert_eq!(c.children_len(), 0);
        assert!(!c.parser.checkpoint_next);
        assert_eq!(c.parser.source.observed(), 3);
        assert_eq!(c.parser.work, work);
        assert_eq!(c.parser.furthest, 1);
        let mark = c.begin_iteration(true);
        let error = Diagnostic::new("grammar", [0, 0], "inner failure");
        assert_eq!(c.end_iteration(mark, Err(error.clone()), true), Err(error));
        assert!(!c.parser.checkpoint_next);
        let mark = c.begin_iteration(true);
        assert_eq!(
            c.end_iteration(mark, Ok(true), true).unwrap_err().message,
            "zero-width repetition"
        );
        assert!(!c.parser.checkpoint_next);
    }
    #[test]
    fn native_callback_guard_checks_stack_without_charging_logical_work() {
        let text = SourceText::new("").unwrap();
        let mut p = parser(&text, "");
        p.stack_base = 0;
        p.stack_budget = 0;
        let mut out = Children::new(0, None);
        let c = SyntaxContext::new(&mut p, &mut out);
        assert_eq!(c.guard().unwrap_err().category, "limit");
        assert_eq!(c.parser.work, 0);
    }
    #[test]
    fn native_trivia_skips_impossible_comments_without_changing_reads_or_work() {
        static LEXERS: crate::lexical::CompiledLexers =
            crate::lexical::CompiledLexers::new(&LANGUAGE, &[None]);
        static COMPILED: CompiledParser = CompiledParser::new(&LEXERS, &[None], &[None]);
        for source in [
            "identifier",
            "é",
            "😀",
            "",
            " \t\r\nx",
            "// line\nx",
            "/* block */x",
            "/",
            "/*",
        ] {
            let text = SourceText::new(source).unwrap();
            let mut old = parser(&text, source);
            let mut new = parser(&text, source);
            old.options.strict_comments = true;
            new.options.strict_comments = true;
            new.compiled = Some(&COMPILED);
            assert_eq!(old.trivia(), new.trivia(), "{source:?}");
            assert_eq!(
                (old.position, old.work, old.max_depth, old.source.observed()),
                (new.position, new.work, new.max_depth, new.source.observed()),
                "{source:?}"
            );
            if !matches!(source, "/" | "/*") {
                assert!(new.stats.trivia_fast_skips > 0, "{source:?}");
            }
        }
    }
}
