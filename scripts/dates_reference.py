"""Freeze/check actual Nitrogen Dates binding, checks, types, HIR and execution."""
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
        result = run("oracle-dates", path, ROOT / "languages/dates/catalog.json", TODAY)
        assert result.returncode == 0, result.stderr
        return json.loads(result.stdout)


def native(source):
    with tempfile.TemporaryDirectory() as folder:
        path = pathlib.Path(folder) / "input.dates"
        path.write_text(source)
        result = subprocess.run([ROOT / "target/release/nitride", "--dates", path, "--today", TODAY], capture_output=True, text=True)
        return json.loads(result.stdout)


def same(actual, expected, label):
    actual, expected = dict(actual), dict(expected)
    actual.pop("message", None); expected.pop("message", None)
    assert actual == expected, (label, expected, actual)


def main():
    frozen = []
    for case in json.loads((ROOT / "fixtures/dates-cases.json").read_text()):
        result = observe(case["source"])
        if "error" in case:
            assert not result["ok"] and result["error"] == case["error"], (case, result)
            assert result["trace"] == []
        else:
            assert result["ok"] and result["values"] == case["values"], (case, result)
        frozen.append({"name": case["name"], "source": case["source"], "observation": result})
    path = ROOT / "fixtures/dates-reference.json"
    if "--freeze" in sys.argv:
        path.write_text(json.dumps(frozen, indent=2, ensure_ascii=False) + "\n")
        print(f"Frozen {len(frozen)} dates cases from real Nitrogen")
        return
    assert json.loads(path.read_text()) == frozen, "frozen dates reference changed"
    for case in frozen:
        same(native(case["source"]), case["observation"], case["name"])
    print(f"{len(frozen)} dates cases match live Nitrogen and native release")
    for index in range(64):
        year = 2020 + index % 9
        month = index % 12 + 1
        day = [1, 15, 28, 29, 30, 31][index % 6]
        text = f"let d = {year:04}-{month:02}-{day:02}; d + {index % 5} days;"
        if index % 3 == 0: text += " (today - d) in days;"
        if index % 4 == 0: text += f" let today = d - {index % 7} days; today;"
        if index % 5 == 0: text += " pi * 2 days in days;"
        if index % 7 == 0: text += " d + d;"
        if index % 11 == 0: text += " missing;"
        same(native(text), observe(text), text)
    print("64 generated dates programs match live Nitrogen")


if __name__ == "__main__":
    main()
