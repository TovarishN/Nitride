# JSONTestSuite parsing corpus

Vendored from https://github.com/nst/JSONTestSuite at commit
`1ef36fa01286573e846ac449e8683f8833c5b26a` (MIT; see LICENSE).
Only `test_parsing/` is included. No third-party executable is run.

The Rust `trivia` integration suite requires acceptance of `y_` cases and
rejection of `n_` cases by the strict generated JSON parser. Invalid UTF-8
is rejected before parsing. `i_` cases have implementation-defined outcomes
and are retained as upstream reference data rather than pass/fail gates.
Resource-limit failures are rejection, not a claim of grammar invalidity.
