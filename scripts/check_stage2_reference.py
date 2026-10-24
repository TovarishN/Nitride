"""Verify frozen stage-two observations against live Nitrogen, without rewriting."""
import json
from reference import ROOT
from stage2_reference import observe

count = 0
for stem, typed in [("syntax", False), ("power", True)]:
    for case in json.loads((ROOT / f"fixtures/{stem}-reference.json").read_text()):
        assert observe(case, typed) == case["observation"], (stem, case["name"])
        count += 1
print(f"{count} frozen stage-two observations match live Nitrogen")
