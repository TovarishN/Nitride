"""Repeat existing strict allocation workloads without overwriting older evidence."""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    for name in ["phase", "namespace", "inference", "calls", "functions", "dates", "rewrites"]:
        path = ROOT / f"scripts/{name}_memory.py"
        source = re.sub(r'docs/(?:stage\d+-)?([^\"\']*memory\.json)',r'docs/stage14-\1',path.read_text())
        namespace = {"__file__":str(path),"__name__":"stage14_memory"}
        exec(compile(source,str(path),"exec"),namespace)
        namespace["main"]()


if __name__ == "__main__":
    main()
