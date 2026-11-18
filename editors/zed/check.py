#!/usr/bin/env python3
"""Run Tree-sitter tests, query validation, and existing-file compatibility checks."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent
GRAMMAR = ROOT / "tree-sitter-nitride"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tree-sitter", default="tree-sitter")
    args = parser.parse_args()
    cli = shutil.which(args.tree_sitter)
    if not cli:
        parser.error("tree-sitter-cli 0.25.10 must be installed and on PATH")

    def run(*arguments, **kwargs):
        subprocess.run([cli, *arguments], cwd=GRAMMAR, check=True, **kwargs)

    manifest = tomllib.loads((ROOT / "extension.toml").read_text())
    languages = []
    for language in sorted((ROOT / "languages").iterdir()):
        config = tomllib.loads((language / "config.toml").read_text())
        assert config["grammar"] in manifest["grammars"], config["name"]
        languages.append(config["name"])
        for query in (GRAMMAR / "queries").glob("*.scm"):
            assert (language / query.name).read_bytes() == query.read_bytes(), f"query drift: {language / query.name}"
    assert set(manifest["language_servers"]["nitride"]["languages"]) <= set(languages)
    run("test")

    repo = ROOT.parents[1]
    paths = subprocess.check_output(
        ["git", "ls-files", "*.ngr", "*.ncatalog", "*.nbind", "*.nproj", "*.ntest"],
        cwd=repo, text=True,
    ).splitlines()
    # Configuring this one parser keeps validation independent of user settings.
    with tempfile.TemporaryDirectory(prefix="nitride-tree-sitter-") as temp:
        import json
        config = Path(temp) / "config.json"
        config.write_text(json.dumps({"parser-directories": [str(ROOT)]}))
        path_list = Path(temp) / "sources.txt"
        path_list.write_text("\n".join(str(repo / path) for path in paths) + "\n")
        run("parse", "--config-path", str(config), "--quiet", "--stat", "--paths", str(path_list))
        sample = str(repo / "languages/portable/Portable.ngr")
        for query in sorted((GRAMMAR / "queries").glob("*.scm")):
            run("query", "--config-path", str(config), str(query), sample, stdout=subprocess.DEVNULL)
    print(f"Validated {len(languages)} language configurations and all shared queries.", flush=True)


if __name__ == "__main__":
    main()
