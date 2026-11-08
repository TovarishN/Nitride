# Calendar planning example

Calendar composes nominal dates/durations, host interfaces, typed rewrites,
records and lists. Dates cross host bindings as zero-based Gregorian day numbers.
The deterministic [planning workflow](planning.calendar) spans leap day, adds a
two-week sprint, finds Monday meetings and checks duration arithmetic.

Run with the Rust implementation:

```sh
cargo run --locked -p nitride-cli --example calendar
```

Run through Python's standard-library implementation using a native-only build:

```sh
cargo run --locked -p nitride-cli --no-default-features -- run languages/calendar/process/CalendarProcess.nproj languages/calendar/planning.calendar
cargo run --locked -p nitride-cli --no-default-features -- conform languages/calendar/process/CalendarProcess.nproj
```

Python 3.9+ is required. This process-only manifest works with distributed native
binaries too. With a default component-enabled development build, use
`languages/calendar/Calendar.nproj` to run the same source through the existing
WebAssembly component. Gregorian version 3 and Clock version 1 are unchanged.

`days`, `weeks` and `fortnights` form durations. Weeks rewrite to seven days,
and fortnights to two weeks. Scalar multiplication/division and duration scaling
use the engine's existing handlers; scalar and duration negation remain nominally
typed. For example, `-2 weeks in days;` produces `-14`, while
`(2 weeks * 3 / 2) in days;` produces `21`.

A date plus another date, or a date multiplied by a scalar, is a type error.
Invalid dates, fractional-day date offsets and out-of-range dates retain declared
host failure codes. Division by zero and invalid list indexes are execution
errors. Type/preflight rejection runs no operation handlers.

Clock access is explicit: programs using `today` need the `Calendar.Clock`
grant. The deterministic workflow needs no clock or other effect. Process-bound
operations never run for editor hints; component hints require a component-enabled
server. Pure engine duration arithmetic can produce hints in the native server.

The repository workspace includes the ordinary Calendar profile. To use the
process-only profile in another workspace, include `languages/calendar/process`
instead. Include only one profile for the `.calendar` extension.

This is a complete DateCalc-shaped planning workflow, not full compatibility
with Nitrogen's DateCalc Math library or arbitrary C# helpers.
