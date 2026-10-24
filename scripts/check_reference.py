"""Check frozen expectations against fresh .NET output without rewriting fixtures."""
import json
from reference import ROOT, observe

cases = json.loads((ROOT / "fixtures/reference.json").read_text())
for case in cases:
    assert observe(case["source"]) == case["observation"], case["name"]
print(f"{len(cases)} frozen observations match the live Nitrogen reference")
