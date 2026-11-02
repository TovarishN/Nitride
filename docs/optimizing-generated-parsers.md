# Optimizing a generated parser

A parser that `nitride_codegen::generate` writes is Rust source. It is
compiled into your crate together with the `nitride-core` runtime, so your
crate's build settings decide how fast it runs. The profile-guided
optimization in Nitride's own release builds applies only to the `nitride`
CLI and language server binary, and a profile cannot be carried from one binary
to another.

## Build settings

Cargo's default release profile uses 16 codegen units and no LTO. Nitride's
own measurements use:

```toml
[profile.release]
lto = true          # or "fat"
codegen-units = 1
panic = "abort"     # if your program does not catch panics
```

Building for the native CPU (`-Ctarget-cpu=native`) made no consistent
difference to parsing, so portable builds lose nothing
([C# assessment](compiler-assessment-2026-10-30.md)).

## Profile-guided optimization

PGO is the largest single gain measured so far:

- 37–40% less time on structured JSON and 26% on long strings, warm, with the
  JSON grammar ([JSON comparison](json-comparison.md)).
- 32–35% less parse time on C# 4 inputs from 256 KiB to 16 MiB
  ([C# assessment](compiler-assessment-2026-10-30.md)).

[`scripts/pgo_generated.py`](../scripts/pgo_generated.py) builds any binary
target with PGO. It:

1. builds an instrumented copy;
2. runs it once per `--train`, from the crate folder;
3. merges the profiles with the `llvm-profdata` that matches your `rustc`;
4. rebuilds with the merged profile.

```sh
rustup component add llvm-tools-preview
python3 -I scripts/pgo_generated.py --crate path/to/your-crate --bin your-bin \
    --train 'parse samples/typical.json' \
    --train 'parse samples/large.json'
```

Each `--train` holds the binary's own arguments. The optimized binary is built
under `<crate>/target/pgo/<host>/release/`, and the script prints its path.
Both builds pass the host as `--target`, so the profile flags reach your
binary's code and not build scripts, such as a `build.rs` that runs the
parser generator. [`scripts/json-bench/pgo.py`](../scripts/json-bench/pgo.py)
is a worked example.

### Training input

The profile is only as good as the training run:

- **Parse the inputs you expect in use:** the same grammar, similar sizes and
  similar shapes. In the C# assessment the training set had no long strings,
  and a long-trivia input then ran 4% slower with PGO than without it.
- **Include edits** if your program edits snapshots (`ParseSnapshot::edit`),
  and include damaged input if it parses in recovery mode. The JSON bench
  trains on three 1 MiB shapes plus an edit run.
- **Retrain when the grammar changes.** Generated code that the profile does
  not cover gets no benefit; `-pgo-warn-missing-function` lists those
  functions during the second build.

## What else to check

- **Time per node, not per byte.** Dense inputs (many small values) cost more
  than long tokens, so measure with inputs shaped like yours.
- **Cold start.** A native parser has no JIT warm-up, so its first parse runs
  at full speed. That matters most for a short-lived process that parses one
  file (see "Warming up on a different file" in the
  [JSON comparison](json-comparison.md)).
