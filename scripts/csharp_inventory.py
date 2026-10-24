"""Find every construct in the vendored CSharp4 grammar that Nitride's frontend does not admit.

The four known gaps are stubbed (grammar functions, UTF-16 surrogate pairs,
`transparent`, non-literal infix operators); each is reported once. Preparation
then repeats until it succeeds or reaches a construct no stub covers, which is
reported as a new gap.
"""
import json
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
GRAMMAR = ROOT / "languages/csharp4"
NATIVE = ROOT / "target/release/nitride"
CALLS = r"\b(StartPlanes|PartPlanes|HexIn|Escaped|Contextual|Plane|Pairs|Supplementary|SupplementaryIn|HexDigit)\("


def stub_functions(text, found):
    out, skipping = [], False
    for line in text.split("\n"):
        if re.match(r"function ", line):
            found.add("grammar functions (`function ... : Grammar.Rule`)")
            skipping = True
            continue
        if skipping and line.startswith((" ", "\t")):
            continue
        skipping = False
        token = re.match(r"(\s*token (\w+) = )(.*)", line)
        if token and re.search(CALLS, token.group(3)):
            word = re.search(r'Contextual\("(\w+)"\)', line)
            line = f'{token.group(1)}"{word.group(1)}" !(IdPart);' if word else f'{token.group(1)}"\\u0001";'
        out.append(line)
    return "\n".join(out)


def stub_known(path, found):
    text = path.read_text()
    text = stub_functions(text, found)
    paired = re.sub(r"\['\\ud[89ab][0-9a-f]{2}'\] \['\\ud[c-f][0-9a-f]{2}'\]", '"\\\\u0001"', text)
    if paired != text:
        found.add("UTF-16 surrogate-pair classes")
    unmarked = text = paired
    unmarked = re.sub(r"\|\s*transparent\s+", "| ", text)
    if unmarked != text:
        found.add("`transparent` alternatives")
    path.write_text(unmarked)


def stub_infix(path, span, found):
    """Insert a placeholder literal before a reference operator so probing can continue."""
    text = path.read_text()
    line_start = text.rfind("\n", 0, span[0]) + 1
    line_end = text.find("\n", span[0])
    line = text[line_start:line_end]
    stub_infix.count = getattr(stub_infix, "count", 0) + 1
    marker = f'"\\u0002{stub_infix.count}" '
    fixed = re.sub(r"(=\s*\w+:\w+(?:\.\w+)*\s+)(?=[A-Z\w]+:|[A-Z])", lambda m: m.group(1) + marker, line, count=1)
    if fixed == line:
        return False
    found.add("infix/postfix operators that are rule references")
    path.write_text(text[:line_start] + fixed + text[line_end:])
    return True


def stub_shared_operator(path, span, found):
    """Make a repeated literal operator unique in the offending alternative."""
    text = path.read_text()
    line_start = text.rfind("\n", 0, span[0]) + 1
    line_end = text.find("\n", span[0])
    line = text[line_start:line_end]
    stub_shared_operator.count = getattr(stub_shared_operator, "count", 0) + 1
    fixed = re.sub(r'(=\s*\w+:\w+(?:\.\w+)*\s+)"[^"]+"', lambda m: m.group(1) + f'"\\u0003{stub_shared_operator.count}"', line, count=1)
    if fixed == line:
        return False
    found.add("several infix/postfix alternatives sharing one literal operator")
    path.write_text(text[:line_start] + fixed + text[line_end:])
    return True


def main():
    known, unknown = set(), []
    with tempfile.TemporaryDirectory() as folder:
        work = pathlib.Path(folder) / "csharp4"
        shutil.copytree(GRAMMAR, work)
        for path in (work / "Grammars").glob("*.ngr"):
            stub_known(path, known)
        history = []
        for _ in range(500):
            result = subprocess.run([NATIVE, "prepare", work / "project.json"], capture_output=True, text=True)
            history.append(result.stderr.strip()[-160:])
            if result.returncode == 0:
                break
            error = result.stderr.strip()
            match = re.match(r"(.+?):(\d+):(\d+) \[(\d+)\.\.(\d+)\]: (.*)", error)
            if not match:
                unknown.append({"error": error})
                break
            path, message = pathlib.Path(match[1]), match[6]
            span = (int(match[4]), int(match[5]))
            if "infix must start with a literal operator" in message and stub_infix(path, span, known):
                continue
            if "conflicting extension operator" in message and stub_shared_operator(path, span, known):
                continue
            if "prefix associativity is not admitted" in message:
                text = path.read_text()
                line_end = text.find("\n", span[0])
                line_start = text.rfind("\n", 0, span[0]) + 1
                line = text[line_start:line_end]
                fixed = re.sub(r"(precedence \d+) (left|right)", r"\1", line, count=1)
                if fixed != line:
                    known.add("prefix alternatives with associativity")
                    path.write_text(text[:line_start] + fixed + text[line_end:])
                    continue
            text = path.read_text()
            line = text[text.rfind("\n", 0, span[0]) + 1:text.find("\n", span[0])]
            unknown.append({"file": path.name, "line": int(match[2]), "message": message, "excerpt": line.strip()[:200]})
            break
        else:
            unknown.append({"error": "probe did not converge", "last": history[-3:]})
    report = {"known": sorted(known), "unknown": unknown, "prepared": not unknown}
    print(json.dumps(report, indent=2))
    return 0 if "--require-complete" not in sys.argv or report["prepared"] else 1


if __name__ == "__main__":
    sys.exit(main())
