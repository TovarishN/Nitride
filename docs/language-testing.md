# Testing a language

Generated projects include `tests.ntest`. Run `nitride test` in the project folder,
`nitride test path/to/suite.ntest` for an explicit suite, or use VS Code's Testing
view. `nitride test --json` reports results for CI; `--case "name"` selects cases
from one suite. No paths means `tests.ntest` in the current directory.

```ntest
tests version 1 {
  project "nitride.nproj";
  test "binding evaluates" {
    source <[let answer = 42; show answer;]>;
    expect run values [42];
  }
  test "unresolved reference" {
    source "show missing;";
    expect compile reject {
      diagnostic "binding" code none span [5, 12];
    }
  }
  test "fixture parses" {
    file "sample.calc";
    expect parse accept;
  }
}
```

The diagnostic example targets the generated typed starter. Categories, codes
and spans should be taken from actual results for your language.

Each suite names one language project relative to its location. Each uniquely
named test has one source/file and one expectation. Raw `<[...]>` blocks preserve
all bytes, including newlines and indentation. The first `]>` closes the block;
use a JSON-escaped quoted string or an external file to test that delimiter.
Comments are supported outside strings and raw blocks. Arrays contain finite
numbers, strings, booleans or nested arrays. Values compare the complete show
result list; integer precision is preserved and 42 equals 42.0.

Use `expect parse accept;`, `expect compile accept;`, `expect run values [...];`
or `expect PHASE reject;`. A reject block asserts the complete ordered strict
diagnostic list. Each `diagnostic "category"` may add `code "CODE"`, `code none`
and `span [start,end]`. Omitted clauses are not compared. Spans are UTF-8 bytes
of the test program, not UTF-16 editor positions. A reject without a block accepts
any ordinary error at that phase. An earlier-phase error fails the assertion.

Preparation, missing input, limits, cancellation and unavailable execution setup
are infrastructure errors. Run tests use built-in handlers and fixed Today
2000-01-01, with no effect grants or external process/component bindings. Existing
interface conformance tests remain available through `nitride conform`.

Exit 0 means all selected tests passed; exit 1 means assertion failures; exit 2 means
configuration/infrastructure errors. `--list --json` discovers cases and physical
dependencies without preparing or executing the language. `--events` streams a
versioned runStarted/caseFinished/runFinished JSON-lines protocol. `--json` emits
one aggregate report. An incomplete or malformed stream never establishes a pass.

Editor runs use frozen unsaved suite/project/grammar/catalog/fixture buffers and
a separate native CLI worker. They do not save source files. Changing inputs
invalidates the running snapshot; rerun against the new revision. Cancel ends the
worker and skips unfinished cases. Test execution requires workspace trust and
workspace-contained paths. Suite syntax/validation diagnostics and test results
are separate from ordinary language-server diagnostics.

`--snapshot-stdin` is the machine interface for overlays:
`{"version":1,"files":[{"path":"/absolute/path","text":"buffer text"}]}`.
Other inputs come from disk; bundled library sources retain their canonical text.
Input limits are 8 MiB per suite, 16 MiB per source/overlay, 64 MiB per snapshot
envelope and 32 MiB per suite result report; a suite contains 1–1000 tests.
