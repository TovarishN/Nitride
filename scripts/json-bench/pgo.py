"""Build the bench with profile-guided optimization through scripts/pgo_generated.py.

Trains on the 1 MiB inputs and an edit run; the binary lands under
target/pgo/<host>/release/, where bench.py looks for it.
"""
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
TRAIN = [f"loop target/json-bench/{shape}-1m.json 20" for shape in ("records", "numbers", "strings")]
TRAIN.append("edit target/json-bench/records-1m.json 20")
command = [sys.executable, "-I", str(HERE.parent / "pgo_generated.py"), "--crate", str(HERE),
           "--bin", "nitride-json-bench"]
for run in TRAIN:
    command += ["--train", run]
subprocess.run(command, check=True)
