"""Deterministic live XML/composition comparisons, separate from frozen fixtures."""
import json
import pathlib
import random
import subprocess
import tempfile
from reference import ROOT
from stage2_reference import observe

rng = random.Random(20261025)


def xml(depth):
    name = f"node{rng.randrange(8)}"
    body = "value λ 😀"
    if depth and rng.randrange(2):
        body = "".join(xml(depth - 1) for _ in range(rng.randrange(1,4)))
    return f'<{name} key="v{rng.randrange(100)}"><!--😀-->{body}</{name}>'


with tempfile.TemporaryDirectory() as tmp:
    path = pathlib.Path(tmp) / "input.txt"
    for index in range(40):
        source = xml(3)
        if index % 4 == 0:
            source = source[:-1]
        case = {"profile":"xml", "source":source}
        expected = observe(case)
        path.write_text(source)
        result = subprocess.run([ROOT / "target/release/nitride", "--parse-xml", path],capture_output=True,text=True)
        assert result.returncode in [0,1], result.stderr
        actual = json.loads(result.stdout)
        assert actual == expected, (index, source, actual, expected)
    for index in range(24):
        source = f"let x : Scalar = {rng.randrange(1,8)}; (x + {rng.randrange(1,4)}) ^ {rng.randrange(0,4)};"
        case = {"source":source}
        expected = observe(case, True)
        path.write_text(source)
        result = subprocess.run([ROOT / "target/release/nitride", "--power", path],capture_output=True,text=True)
        assert result.returncode in [0,1], result.stderr
        actual = json.loads(result.stdout)
        assert actual == expected, (index, source, actual, expected)
print("64 generated XML/typed-composition programs match live Nitrogen")
