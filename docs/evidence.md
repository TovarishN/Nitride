# Historical stage-one native deployment evidence

These measurements describe the first milestone. See [stage-two evidence](stage2-evidence.md)
for the current binary and isolated engine allocations.

Measured on macOS-27.0.1-arm64-arm-64bit with rustc 1.99.1 (58cf8f9c3 2026-10-13) (Homebrew).

The release executable is 418,992 bytes (409.2 KiB). `otool -L` lists only `/usr/lib/libSystem.B.dylib`.

Five process-cold runs per case, with warm OS caches. Peak RSS is measured by macOS `/usr/bin/time -l`; the empty-program case approximates a startup workload, not an idle-process snapshot.

| Workload | Rust peak RSS median | .NET reference peak RSS median | Rust wall time median | .NET reference wall time median |
|---|---:|---:|---:|---:|
| empty | 1.92 MiB | 52.50 MiB | 4.51 ms | 105.25 ms |
| small | 1.97 MiB | 53.30 MiB | 9.78 ms | 124.63 ms |
| 1000_statements | 25.83 MiB | 68.30 MiB | 27.99 ms | 188.55 ms |

These workloads include initialization, parse/bind/check/lower/execute, JSON observation construction and output. The 1,000-statement case therefore includes substantial observation-formatting storage; it is not a measurement of the parser alone.

The .NET reference is a development adapter, not an optimized production baseline. Its deployment directory excludes the separately installed .NET runtime. Do not interpret these results as a general speed ratio or a claim about full Nitrogen workloads.

The native warm pipeline measurement runs without observation JSON. On the small case, syntax storage retains 1054 capacity bytes and HIR/statements/binding vectors retain 432 capacity bytes. These counts exclude language static data, temporary buffers, allocator overhead and execution storage; they are not peak engine allocation measurements.

The native executable was also run with PATH restricted to system tools and DOTNET environment variables absent. A machine with .NET physically uninstalled was not available. Linux and Windows were not executed.

## Reproduce

```sh
cargo build --release
dotnet build tools/Nitride.Reference -c Release
python3 scripts/footprint.py
```

Full raw measurements and limitations: [footprint.json](footprint.json). Allocator high-water measurements, an idle-process baseline and stack profiling are pending. The measurements support continuing the native implementation while leaving broader language and platform admission open.
