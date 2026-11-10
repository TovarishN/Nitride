use crate::{
    Diagnostic, ExecutionReport, HostBinding, HostValue, Language, Limits, ParseOptions, Program,
    SyntaxTree,
};

/// The stage that rejected a source document or failed while running it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Phase {
    Parse,
    Check,
    Preflight,
    Execute,
}

/// All diagnostics from a failed stage, plus any execution trace.
#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    phase: Phase,
    diagnostics: Vec<Diagnostic>,
    trace: Vec<String>,
}

impl Error {
    fn new(phase: Phase, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            phase,
            diagnostics,
            trace: Vec::new(),
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    /// Operations invoked before an execution failure, including the failing call.
    /// Earlier stages have no trace.
    pub fn trace(&self) -> &[String] {
        &self.trace
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} failed", self.phase)?;
        for diagnostic in &self.diagnostics {
            write!(
                f,
                ": {} at {}..{}",
                diagnostic.message, diagnostic.span[0], diagnostic.span[1]
            )?;
        }
        Ok(())
    }
}
impl std::error::Error for Error {}

/// A reusable entry point for one admitted language. Tables are borrowed, not
/// leaked or copied; each call owns its source analysis and execution state.
/// Parsing is strict by default, including unterminated trailing comments.
pub struct Engine<'g> {
    language: &'g Language<'g>,
    limits: Limits,
    options: ParseOptions,
}

impl<'g> Engine<'g> {
    pub fn new(language: &'g Language<'g>) -> Self {
        Self {
            language,
            limits: Limits::default(),
            options: ParseOptions {
                strict_comments: true,
                ..Default::default()
            },
        }
    }

    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Override strict parsing options explicitly. Recovery and incremental
    /// snapshots remain available through the re-exported core APIs.
    pub fn with_parse_options(mut self, options: ParseOptions) -> Self {
        self.options = options;
        self
    }

    pub fn parse(&self, source: &str) -> Result<SyntaxTree<'g>, Error> {
        crate::core::parse_with_options(self.language, source, self.limits, self.options)
            .map_err(|error| Error::new(Phase::Parse, vec![error]))
    }

    pub fn compile(&self, source: &str) -> Result<Program<'g>, Error> {
        let tree = self.parse(source)?;
        crate::core::compile(self.language, &tree, self.limits)
            .map_err(|errors| Error::new(Phase::Check, errors))
    }

    /// Parse, check, preflight all required bindings, then execute. No host
    /// operation runs if parsing, checking or preflight rejects the document.
    pub fn run(
        &self,
        source: &str,
        bindings: &[HostBinding<'_>],
    ) -> Result<ExecutionReport<'g>, Error> {
        self.run_with_values(source, bindings, &[])
    }

    /// As `run`, with explicit providers for catalog built-in values. Providers
    /// are not read during preflight and are cached within each execution.
    pub fn run_with_values(
        &self,
        source: &str,
        bindings: &[HostBinding<'_>],
        values: &[HostValue],
    ) -> Result<ExecutionReport<'g>, Error> {
        let program = self.compile(source)?;
        // Keep every preflight diagnostic; the lower-level executor also
        // validates its inputs, but its failure reports only the first one.
        crate::core::preflight_with_values(&program, bindings, values)
            .map_err(|errors| Error::new(Phase::Preflight, errors))?;
        crate::core::execute_with_values(&program, bindings, values, self.limits).map_err(
            |failure| Error {
                phase: Phase::Execute,
                diagnostics: vec![failure.diagnostic],
                trace: failure.trace.into_iter().map(str::to_owned).collect(),
            },
        )
    }
}
