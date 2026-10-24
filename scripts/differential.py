"""Deterministic generated-source checks against live Nitrogen and native executables."""
import json
import pathlib
import random
import subprocess
import tempfile
from reference import ROOT, observe

rng = random.Random(20261024)


def expression(depth):
    if depth == 0 or rng.randrange(3) == 0:
        return str(rng.randrange(1, 20))
    if rng.randrange(4) == 0:
        return "-(" + expression(depth - 1) + ")"
    return "(" + expression(depth - 1) + rng.choice([" + ", " - ", " * "]) + expression(depth - 1) + ")"


with tempfile.TemporaryDirectory() as tmp:
    path = pathlib.Path(tmp) / "generated.portable"
    for i in range(64):
        source = "// λ 😀\n" + expression(3) + ";"
        if i % 3 == 0:
            source = "let x : Scalar = " + expression(2) + "; x + " + expression(2) + ";"
        if i % 4 == 0:
            source += " @"
        expected = observe(source)
        path.write_text(source)
        native = subprocess.run([ROOT / "target/release/nitride", path], capture_output=True, text=True)
        assert native.returncode in [0, 1], (source, native.stderr)
        actual = json.loads(native.stdout)
        if expected["ok"]:
            for key in ["ok", "syntax", "bindings", "hir", "values", "trace"]:
                assert actual[key] == expected[key], (i, key, source, actual, expected)
        else:
            for key in ["ok", "error", "span", "trace"]:
                assert actual[key] == expected[key], (i, key, source, actual, expected)
print("64 deterministic generated programs match live Nitrogen")
