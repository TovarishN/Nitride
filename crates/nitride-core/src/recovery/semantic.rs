use crate::*;

#[derive(Debug)]
pub struct RecoveredAnalysis<'g> {
    pub program: Option<Program<'g>>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn compile_recovering<'g>(
    language: &'g Language<'g>,
    parsed: &RecoveredParse<'g>,
    limits: Limits,
) -> RecoveredAnalysis<'g> {
    let mut analysis = compile_program(language, parsed, limits);
    analysis
        .diagnostics
        .extend(crate::rule_diagnostics(language, &parsed.tree));
    analysis
}

fn compile_program<'g>(
    language: &'g Language<'g>,
    parsed: &RecoveredParse<'g>,
    limits: Limits,
) -> RecoveredAnalysis<'g> {
    if language.profile == Profile::SyntaxOnly {
        return RecoveredAnalysis {
            program: None,
            diagnostics: parsed
                .tree
                .recovery()
                .map_or_else(Vec::new, |r| r.diagnostics.clone()),
        };
    }
    match crate::semantic::compile_mode(language, &parsed.tree, limits, true) {
        Ok(program) => RecoveredAnalysis {
            diagnostics: program.diagnostics().to_vec(),
            program: Some(program),
        },
        Err(error) => {
            let mut diagnostics = parsed
                .tree
                .recovery()
                .map_or_else(Vec::new, |r| r.diagnostics.clone());
            diagnostics.push(error);
            RecoveredAnalysis {
                program: None,
                diagnostics,
            }
        }
    }
}
