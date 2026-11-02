"""Build target/pgo/release/nitride-json-bench with profile-guided optimization."""
import os
import pathlib
import shutil
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from release.build import profdata_tool  # noqa: E402

INPUTS = HERE / "target/json-bench"
RAW = HERE / "target/pgo-raw"


def build(target_dir, flags):
    env = os.environ.copy()
    env.pop("RUSTFLAGS", None)
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
    env["LLVM_PROFILE_FILE"] = str(RAW / "bench-%m-%p.profraw")
    subprocess.run(["cargo", "build", "--release", "--target-dir", target_dir], cwd=HERE, env=env, check=True)
    return HERE / target_dir / "release/nitride-json-bench"


def main():
    profdata = profdata_tool(os.environ.get("LLVM_PROFDATA"))
    shutil.rmtree(RAW, ignore_errors=True)
    RAW.mkdir(parents=True)
    instrumented = build("target/pgo-gen", [f"-Cprofile-generate={RAW}"])
    env = {**os.environ, "LLVM_PROFILE_FILE": str(RAW / "bench-%m-%p.profraw")}
    for shape in ("records", "numbers", "strings"):
        subprocess.run([instrumented, "loop", INPUTS / f"{shape}-1m.json", "20"], env=env, check=True)
    subprocess.run([instrumented, "edit", INPUTS / "records-1m.json", "20"], env=env, check=True,
                   stdout=subprocess.DEVNULL)
    merged = RAW / "merged.profdata"
    subprocess.run([profdata, "merge", "-o", merged, *sorted(RAW.glob("*.profraw"))], check=True)
    binary = build("target/pgo", [f"-Cprofile-use={merged}", "-Cllvm-args=-pgo-warn-missing-function"])
    print(binary)


main()
