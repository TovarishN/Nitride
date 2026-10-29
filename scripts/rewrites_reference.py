"""Freeze/check actual Nitrogen quote rewrites over Dates: syntax, origins, values, effects."""
import json
import pathlib
import subprocess
import sys
import tempfile
from test_reference import ROOT, run

TODAY = "2026-10-24"


def observe(source):
    with tempfile.TemporaryDirectory() as folder:
        path = pathlib.Path(folder) / "input.dates"
        path.write_text(source)
        result = run("oracle-rewrites", path, ROOT / "languages/dates/Dates.ncatalog", TODAY)
        assert result.returncode == 0, result.stderr
        return json.loads(result.stdout)


def native(source):
    with tempfile.TemporaryDirectory() as folder:
        path = pathlib.Path(folder) / "input.dates"
        path.write_text(source)
        result = subprocess.run([ROOT / "target/release/nitride", "--rewrites", path, "--today", TODAY], capture_output=True, text=True)
        return json.loads(result.stdout)


def same(actual, expected, label):
    actual, expected = dict(actual), dict(expected)
    actual.pop("message", None); expected.pop("message", None)
    assert actual == expected, (label, expected, actual)


def main():
    frozen = []
    for case in json.loads((ROOT / "fixtures/rewrites-cases.json").read_text()):
        result = observe(case["source"])
        if "error" in case:
            assert not result["ok"] and result["error"] == case["error"], (case, result)
            assert result["trace"] == []
        else:
            assert result["ok"] and result["values"] == case["values"], (case, result)
        frozen.append({"name": case["name"], "source": case["source"], "observation": result})
    path = ROOT / "fixtures/rewrites-reference.json"
    if "--freeze" in sys.argv:
        path.write_text(json.dumps(frozen, indent=2, ensure_ascii=False) + "\n")
        print(f"Frozen {len(frozen)} rewrites cases from real Nitrogen")
        return
    assert json.loads(path.read_text()) == frozen, "frozen dates reference changed"
    for case in frozen:
        same(native(case["source"]), case["observation"], case["name"])
    print(f"{len(frozen)} rewrites cases match live Nitrogen and native release")
    for index in range(64):
        value = index % 9 + 1
        text = f"let n = {value}; n + {index % 4} swapped; {value} fortnights;"
        if index % 2 == 0: text += f" twice({value} days) in days;"
        if index % 3 == 0: text += f" (~{value} - today) in days;"
        if index % 5 == 0: text += f" let today = 2026-01-0{value % 9 + 1}; ~n;"
        if index % 7 == 0: text += " (n + 1) swapped swapped;"
        if index % 11 == 0: text += " 2026-10-05 + 1 days swapped;"
        if index % 13 == 0: text += " twice(missing);"
        same(native(text), observe(text), text)
    print("64 generated rewrites programs match live Nitrogen")


if __name__ == "__main__":
    main()
