"""Freeze actual Nitrogen syntax-tree and typed-composition observations."""
import json
import pathlib
import subprocess
import tempfile
from reference import ROOT, DLL


def observe(case, typed=False):
    profile = "power" if typed else case["profile"]
    with tempfile.TemporaryDirectory() as tmp:
        source = pathlib.Path(tmp) / "input.txt"
        source.write_text(case["source"])
        command = ["dotnet", str(DLL)] + (["oracle-power", str(source), str(ROOT / "languages/power/catalog.json")] if typed else ["oracle-syntax", profile, str(source), str(ROOT / f"languages/{profile}/language.json")])
        result = subprocess.run(command, capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError(result.stderr)
        return json.loads(result.stdout)


def main():
    for stem, typed in [("syntax", False), ("power", True)]:
        output = []
        for case in json.loads((ROOT / f"fixtures/{stem}-cases.json").read_text()):
            observation = observe(case, typed)
            if typed:
                if "error" in case:
                    assert not observation["ok"] and observation["error"] == case["error"], (case, observation)
                else:
                    assert observation["ok"] and observation["values"] == case["values"], (case, observation)
            else:
                assert observation["ok"] == case["ok"], (case, observation)
            output.append({**case, "observation":observation})
        (ROOT / f"fixtures/{stem}-reference.json").write_text(json.dumps(output, indent=2, ensure_ascii=False) + "\n")
        print(f"Frozen {len(output)} {stem} reference cases")


if __name__ == "__main__":
    main()
