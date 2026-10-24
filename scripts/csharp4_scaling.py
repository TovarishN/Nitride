"""Measure CSharp4 parse scaling: native Nitride vs Nitrogen's generated parser on the same files."""
import json
import pathlib
import platform
import subprocess
import tempfile
from test_reference import ROOT

RELEASE_DLL = ROOT / "tools/Nitride.Reference/bin/Release/net10.0/Nitride.Reference.dll"


def generated(size):
    out = ["using System;\nusing System.Linq;\n"]
    length, index = len(out[0]), 0
    while length < size:
        block = (f"namespace N{index} {{ public class C{index}<T> where T : class {{\n"
                 f"int f{index} = {index};\n"
                 f"public string Name {{ get {{ return \"n{index}\"; }} set {{ }} }}\n"
                 f"int F(int x) {{ if (x > {index}) {{ return x * 2 + f{index}; }} else {{ for (int i = 0; i < x; i++) {{ x += i; }} return x; }} }}\n"
                 f"void Q() {{ var xs = from a in new[] {{ {index}, 2, 3 }} where a > 1 select a * 2; Func<int, int> g = y => y + {index}; g(1); }}\n"
                 "} }\n")
        out.append(block)
        length += len(block)
        index += 1
    return "".join(out)


def main():
    rows = []
    with tempfile.TemporaryDirectory() as folder:
        for size in [64 * 1024, 1 << 20, 4 << 20, 16 << 20]:
            path = pathlib.Path(folder) / f"g{size}.cs"
            path.write_text(generated(size))
            runs = []
            for _ in range(3):
                report = json.loads(subprocess.check_output([ROOT / "target/release/nitride-profile", "csharp4", path], text=True))
                runs.append(report)
            best = min(runs, key=lambda r: r["parse"]["elapsed_ns"])
            reference = json.loads(subprocess.check_output(["dotnet", RELEASE_DLL, "bench-syntax", "csharp4", path, "3"], text=True))
            rows.append({"bytes": path.stat().st_size, "native_parse_seconds": best["parse"]["elapsed_ns"] / 1e9,
                         "native_parse_allocations": best["parse"]["allocation_count"], "native_parse_peak_bytes": best["parse"]["peak_bytes"],
                         "native_tree_bytes": best["syntax_capacity_bytes"], "native_nodes": best["syntax_nodes"],
                         "nitrogen_parse_seconds": reference["seconds_min"],
                         "nitrogen_parse_allocated_bytes": reference["allocated_bytes"], "nitrogen_nodes": reference["nodes"],
                         "nitrogen_tree_array_bytes": reference["tree_array_bytes"], "nitrogen_text_bytes": reference["text_bytes"],
                         "nitrogen_heap_growth_bytes": reference["heap_growth_bytes"]})
            print(json.dumps(rows[-1]))
    report = {"host": platform.platform(), "machine": platform.machine(), "measurements": rows,
              "metric": "Best of 3 parse-phase wall times; native allocations/peak are requested System bytes; native tree bytes are the retained tree including its UTF-8 source copy; Nitrogen allocated bytes are GC.GetAllocatedBytesForCurrentThread over a warm parse (Release build), which omits pooled rentals; Nitrogen retained size is the tree's arrays at their (pooled or exact) length plus its UTF-16 text, cross-checked by managed heap growth while the tree is alive"}
    (ROOT / "docs/stage13-scaling.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
