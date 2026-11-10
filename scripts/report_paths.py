"""Generated benchmark reports stay separate from frozen documentation evidence."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def report_path(name, *, root=ROOT):
    name = str(name)
    if Path(name).name != name or any(c in name for c in "/\\:") or not name.endswith(".json"):
        raise ValueError("report name must be a single JSON filename")
    return Path(root) / "target" / "reports" / name


def write_report(name, data, *, root=ROOT):
    path = report_path(name, root=root)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    return path
