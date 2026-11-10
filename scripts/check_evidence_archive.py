"""Verify frozen stage artifacts without running their historical workloads."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ARCHIVE = Path("docs/archive/stages")


def check_archive(root=ROOT):
    root = Path(root)
    directory = (root / ARCHIVE).resolve()
    try:
        manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        return [f"cannot read evidence manifest: {error}"]
    if not isinstance(manifest, dict) or manifest.get("schema") != 1 or not isinstance(manifest.get("files"), list) or not manifest["files"]:
        return ["invalid evidence manifest"]
    errors = []
    seen = set()
    for entry in manifest["files"]:
        try:
            path = (root / entry["archived"]).resolve()
            if not path.is_relative_to(directory) or path in seen:
                raise ValueError("invalid or duplicate archive path")
            seen.add(path)
            data = path.read_bytes()
            digest = hashlib.sha256(data).hexdigest()
            if len(data) != entry["bytes"] or digest != entry["sha256"]:
                errors.append(f"changed archive artifact: {entry['archived']}")
            if path.suffix == ".json" and digest != entry["original_sha256"]:
                errors.append(f"raw JSON differs from original evidence: {entry['archived']}")
        except (OSError, ValueError, KeyError, TypeError) as error:
            errors.append(f"invalid archive entry: {error}")
    return errors


def main():
    errors = check_archive()
    if errors:
        for error in errors:
            print(error)
        return 1
    manifest = json.loads((ROOT / ARCHIVE / "manifest.json").read_text(encoding="utf-8"))
    raw = sum(Path(entry["archived"]).suffix == ".json" for entry in manifest["files"])
    print(f"Verified {len(manifest['files'])} archived files; {raw} raw JSON reports are byte-identical.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
