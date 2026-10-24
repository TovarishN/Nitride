"""Compare identical observation workloads. Run release builds before this script."""
import json
import os
import pathlib
import platform
import re
import shutil
import statistics
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
NATIVE = ROOT / "target/release/nitride"
REFERENCE = ROOT / "tools/Nitride.Reference/bin/Release/net10.0/Nitride.Reference.dll"
CATALOG = ROOT / "languages/portable/catalog.json"


def measure(command, count=5, env=None):
    mac = platform.system() == "Darwin"
    if not mac and platform.system() != "Linux":
        raise RuntimeError("process-memory measurement currently supports macOS/Linux")
    timer = ["/usr/bin/time", "-l" if mac else "-v"]
    elapsed, peaks = [], []
    for _ in range(count):
        started = time.perf_counter()
        result = subprocess.run(timer + list(map(str, command)), stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True, env=env)
        elapsed.append((time.perf_counter() - started) * 1000)
        if result.returncode:
            raise RuntimeError(result.stderr)
        pattern = r"(\d+)\s+maximum resident set size" if mac else r"Maximum resident set size \(kbytes\):\s*(\d+)"
        match = re.search(pattern, result.stderr)
        if not match:
            raise RuntimeError("missing process-memory metric: " + result.stderr)
        peaks.append(int(match[1]) * (1 if mac else 1024))
    return {"runs": count, "wall_ms_median": statistics.median(elapsed), "wall_ms_min": min(elapsed), "wall_ms_max": max(elapsed), "peak_rss_bytes_median": statistics.median(peaks), "peak_rss_bytes_min": min(peaks), "peak_rss_bytes_max": max(peaks)}


def main():
    dotnet = shutil.which("dotnet")
    if not dotnet:
        raise RuntimeError(".NET required for reference measurement only")
    env = {k: v for k, v in os.environ.items() if not k.startswith("DOTNET")}
    env["PATH"] = "/usr/bin:/bin"
    cases = {"empty": "", "small": "let x : Scalar = 1 + 2; x * 7;", "1000_statements": "\n".join(f"{i} + 2 * 3;" for i in range(1000))}
    report = {"host": platform.platform(), "machine": platform.machine(), "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "dotnet_sdk": subprocess.check_output([dotnet, "--version"], text=True).strip(), "reference_target_framework": "net10.0", "native_executable_bytes": NATIVE.stat().st_size,
        "reference_output_directory_bytes_excluding_symbols": sum(p.stat().st_size for p in REFERENCE.parent.iterdir() if p.is_file() and p.suffix != ".pdb"),
        "measurements": {}, "limitations": ["Peak RSS, not retained engine bytes or an idle-process baseline.", "Wall time includes process spawn, initialization, pipeline, JSON observation formatting and output; it is not a warm parser benchmark.", "Reference is a development adapter over real Nitrogen APIs, not an optimized production deployment.", "Reference output size excludes the separately installed .NET runtime; native executable still uses normal OS libraries.", ".NET HIR constants are f32; native constants are f64. The measured corpus uses exactly representable integers.", "Allocator high-water instrumentation and stack measurement are not available in this report.", "Warm OS caches; process-cold runs; no cold disk-cache claim."]}
    with tempfile.TemporaryDirectory() as tmp:
        for name, source in cases.items():
            path = pathlib.Path(tmp) / (name + ".portable")
            path.write_text(source)
            report["measurements"][name] = {"source_bytes": len(source.encode()), "native": measure([NATIVE, path], env=env), "dotnet_reference": measure([dotnet, REFERENCE, "oracle", path, CATALOG])}
        path = pathlib.Path(tmp) / "small.portable"
        report["native_warm_pipeline"] = json.loads(subprocess.check_output([NATIVE, "--bench", path, "1000"], text=True))
    (ROOT / "docs/footprint.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
