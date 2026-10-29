# Native lexical execution plan

Execute inline with executing-plans and TDD. Approved direction: user Go after the generated-lexical proposal. Spec: docs/superpowers/specs/2026-10-26-native-lexers-design.md. Preserve all previous work/checkpoints; no commits or integration.

## Task 1: Generated recognizer and runtime
- [x] RED scanner-generation eligibility regression; emit bounded acyclic lexical scanners alongside static Expr data.
- [x] RED runtime semantic/read/budget regressions; implement local scanner context and opt-in snapshot integration.
- [x] RED generated C# equivalence/activation and mode regressions; expose profiler selection and counters.
- [x] Core/codegen/CLI tests, exploratory Release comparison; retain only measurable improvement.

## Task 2: Verification and evidence
- [x] Full suite, fmt, strict all-feature clippy, bootstrap, driver tests.
- [x] One whole-delta read-only review and one TDD fix pass if required.
- [x] Freeze corrected ordinary-allocator profiler; three cold samples per size, complete corpora and validated recovered edit/retention checks.
- [x] Record matching source/executable hashes, raw evidence and remaining Roslyn gap.

## Execution result

Locally complete and verified, Roslyn parity false. See docs/stage18-evidence.md. One P2 review finding fixed RED→GREEN; all post-fix gates and captures passed. No commits or integration.
