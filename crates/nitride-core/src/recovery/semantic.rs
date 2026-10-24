use crate::*;

#[derive(Debug)]
pub struct RecoveredAnalysis {
    pub program: Option<Program>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn compile_recovering(
    language: &'static Language,
    parsed: &RecoveredParse,
    limits: Limits,
) -> RecoveredAnalysis {
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
