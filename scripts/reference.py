"""Regenerate observations using actual Nitrogen parsing, binding, types and HIR."""
import json
import pathlib
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
DLL = ROOT / "tools/Nitride.Reference/bin/Debug/net10.0/Nitride.Reference.dll"
CATALOG = ROOT / "languages/portable/catalog.json"


def observe(source):
    with tempfile.TemporaryDirectory() as tmp:
        path = pathlib.Path(tmp) / "input.portable"
        path.write_text(source)
        result = subprocess.run(["dotnet", str(DLL), "oracle", str(path), str(CATALOG)], capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError(result.stderr)
        return json.loads(result.stdout)


def main():
    observations = []
    for case in json.loads((ROOT / "fixtures/cases.json").read_text()):
        result = observe(case["source"])
        if "error" in case:
            assert not result["ok"] and result["error"] == case["error"], (case["name"], result)
            if case["error"] != "execution":
                assert result["trace"] == [], (case["name"], result)
        else:
            assert result["ok"] and result["values"] == case["values"], (case["name"], result)
        observations.append({"name": case["name"], "source": case["source"], "observation": result})
    (ROOT / "fixtures/reference.json").write_text(json.dumps(observations, indent=2, ensure_ascii=False) + "\n")
    print(f"Frozen {len(observations)} Nitrogen observations")


if __name__ == "__main__":
    main()
