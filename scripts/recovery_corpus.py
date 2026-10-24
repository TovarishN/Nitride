"""Deterministic source damage and byte-accurate, self-relative measurements."""
from dataclasses import dataclass, asdict
import hashlib
import json
import pathlib
import random
import re

ROOT = pathlib.Path(__file__).resolve().parents[1]
SEED = 20261024


@dataclass(frozen=True)
class Edit:
    start: int
    end: int
    replacement: str

    def apply(self, source):
        raw = source.encode("utf-8")
        return (raw[:self.start] + self.replacement.encode("utf-8") + raw[self.end:]).decode("utf-8")


def map_span(span, edit):
    start, end = span
    delta = len(edit.replacement.encode("utf-8")) - (edit.end - edit.start)
    if end <= edit.start:
        return [start, end]
    if start >= edit.end:
        return [start + delta, end + delta]
    return None


def preservation(before, after, edit):
    actual = {(tuple(item["span"]), item["shape"]) for item in after}
    eligible = kept = 0
    for item in before:
        mapped = map_span(item["span"], edit)
        if mapped is not None:
            eligible += 1
            kept += (tuple(mapped), item["shape"]) in actual
    return {"eligible": eligible, "preserved": kept}


def token_spans(source):
    # The external damage generator needs a lexer, not a language-specific
    # recovery rule. C# uses Roslyn's actual token spans instead of this fallback.
    pattern = r'//[^\n]*|/\*[\s\S]*?\*/|"(?:\\.|[^"\\])*"|\s+|[\w]+|(?:=>|==|!=|<=|>=|\+\+|--|&&|\|\||::)|[^\w\s]'
    offsets = [0]
    for char in source:
        offsets.append(offsets[-1] + len(char.encode("utf-8")))
    return [[offsets[m.start()], offsets[m.end()]] for m in re.finditer(pattern, source) if not m.group().isspace() and not m.group().startswith(("//", "/*"))]


def damages(source, tokens, key):
    rng = random.Random(f"{SEED}:{key}")
    if not tokens:
        for kind in ["delete", "insert", "duplicate", "swap", "truncate", "closer"]:
            yield kind, None
        return
    raw = source.encode("utf-8")
    i = rng.randrange(len(tokens))
    start, end = tokens[i]
    text = raw[start:end].decode("utf-8")
    yield "delete", Edit(start, end, "")
    yield "insert", Edit(start, start, "@ ")
    yield "duplicate", Edit(end, end, " " + text)
    if i + 1 < len(tokens):
        a, b = tokens[i + 1]
        yield "swap", Edit(start, b, raw[a:b].decode("utf-8") + raw[end:a].decode("utf-8") + text)
    else:
        yield "swap", None
    # Use a character boundary inside a nontrivial line where possible.
    points = [index for index, char in enumerate(source) if char != "\n" and 0 < index < len(source)]
    if points:
        point = points[rng.randrange(len(points))]
        at = len(source[:point].encode("utf-8"))
        yield "truncate", Edit(at, len(raw), "")
    else:
        yield "truncate", None
    closers = [(a, b) for a, b in tokens if raw[a:b] in [b")", b"]", b"}"]]
    if closers:
        a, b = closers[rng.randrange(len(closers))]
        yield "closer", Edit(a, b, "")
    else:
        yield "closer", None


def frozen_baselines():
    profiles = [("portable", "reference.json"), ("power", "power-reference.json"), ("namespaces", "namespaces-reference.json"), ("inference", "inference-reference.json"), ("calls", "calls-reference.json"), ("functions", "functions-reference.json"), ("dates", "dates-reference.json"), ("rewrites", "rewrites-reference.json"), ("csharp4", "csharp4-reference.json")]
    for profile, filename in profiles:
        for case in json.loads((ROOT / "fixtures" / filename).read_text()):
            if case["observation"]["ok"]:
                yield {"id":f"{profile}:frozen:{case['name']}", "profile":profile, "source":case["source"], "origin":filename}
    for case in json.loads((ROOT / "fixtures/syntax-reference.json").read_text()):
        if case["observation"]["ok"]:
            yield {"id":f"{case['profile']}:frozen:{case['name']}", "profile":case["profile"], "source":case["source"], "origin":"syntax-reference.json"}
    yield {"id":"grammar:self", "profile":"grammar", "source":(ROOT/"languages/grammar/Grammar.ngr").read_text(), "origin":"languages/grammar/Grammar.ngr"}


def generated_baselines():
    """Extract the existing generators' source-only statements, without calling
    their live oracle/CLI loops. Freeze hashes so a changed generator is visible.
    Unrecognized generator layouts fail rather than inventing replacement cases.
    """
    import ast

    def block(filename, count, source_variable, namespace):
        path = ROOT / "scripts" / filename
        tree = ast.parse(path.read_text())
        loops = [node for node in ast.walk(tree) if isinstance(node, ast.For) and isinstance(node.iter, ast.Call) and isinstance(node.iter.func, ast.Name) and node.iter.func.id == "range" and len(node.iter.args) == 1 and isinstance(node.iter.args[0], ast.Constant) and node.iter.args[0].value == count]
        if len(loops) != 1:
            raise RuntimeError(f"generator shape changed: {filename}")
        loop = loops[0]
        statements = []
        for statement in loop.body:
            calls = [node for node in ast.walk(statement) if isinstance(node, ast.Call)]
            if any((isinstance(c.func, ast.Name) and c.func.id in {"observe", "native", "same"}) or (isinstance(c.func, ast.Attribute) and c.func.attr == "write_text") for c in calls):
                break
            statements.append(statement)
        if not statements:
            raise RuntimeError(f"no source generator in {filename}")
        code = compile(ast.fix_missing_locations(ast.Module(body=statements,type_ignores=[])),str(path),"exec")
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        for index in range(count):
            namespace[loop.target.id] = index
            exec(code,namespace)
            yield index, namespace[source_variable], digest

    for profile in ["namespaces","inference","calls","functions","dates","rewrites"]:
        filename = f"{profile}_reference.py"
        for index, source, digest in block(filename,64,"text",{}):
            yield {"id":f"{profile}:generated:{index}","profile":profile,"source":source,"origin":filename,"generator_sha256":digest}

    for filename, seed, function, loops in [
        ("differential.py",20261024,"expression",[(64,"portable")]),
        ("stage2_differential.py",20261025,"xml",[(40,"xml"),(24,"power")]),
    ]:
        tree = ast.parse((ROOT/"scripts"/filename).read_text())
        definition = next(node for node in tree.body if isinstance(node,ast.FunctionDef) and node.name == function)
        namespace = {"rng":random.Random(seed)}
        exec(compile(ast.fix_missing_locations(ast.Module(body=[definition],type_ignores=[])),filename,"exec"),namespace)
        for count, profile in loops:
            for index, source, digest in block(filename,count,"source",namespace):
                yield {"id":f"{profile}:generated:{index}","profile":profile,"source":source,"origin":filename,"generator_sha256":digest}

    path = ROOT/"scripts/csharp4_reference.py"
    definition = next(node for node in ast.parse(path.read_text()).body if isinstance(node,ast.FunctionDef) and node.name == "generated")
    namespace = {}
    exec(compile(ast.fix_missing_locations(ast.Module(body=[definition],type_ignores=[])),str(path),"exec"),namespace)
    for index in range(64):
        yield {"id":f"csharp4:generated:{index}","profile":"csharp4","source":namespace["generated"](index),"origin":path.name,"generator_sha256":hashlib.sha256(path.read_bytes()).hexdigest()}


def inverse_span(span, edit):
    if edit is None:
        return span
    start, end = span
    after_end = edit.start + len(edit.replacement.encode("utf-8"))
    delta = after_end - edit.end
    if end <= edit.start:
        return [start,end]
    if start >= after_end:
        return [start-delta,end-delta]
    return None


def semantic_items(observation, edit=None):
    """HIR, types, bindings and source origins, canonicalized to original bytes.
    Dependencies on damaged declarations remain in the denominator; their
    intentional error propagation is visible as semantic preservation loss.
    """
    shapes = {}
    for node in observation["hir"]:
        value = {k:v for k,v in node.items() if k not in {"id","args","span","origins","origin_rewrites"}}
        value["span"] = inverse_span(node["span"],edit)
        if "symbol" in value and value["symbol"] is not None:
            mapped = inverse_span([value["symbol"],value["symbol"]],edit)
            value["symbol"] = mapped[0] if mapped else None
        flags = node.get("origin_rewrites",[])
        value["origins"] = [span if i < len(flags) and flags[i] else inverse_span(span,edit) for i,span in enumerate(node.get("origins",[]))]
        value["args"] = [shapes[arg] for arg in node.get("args",[])]
        shapes[node["id"]] = hashlib.sha256(json.dumps(value,sort_keys=True).encode()).hexdigest()
    items = [node["span"] for node in observation["nodes"] if node.get("item")]
    result = []
    for statement in observation["statements"]:
        root = observation["hir"][statement["value"]]
        candidates = [span for span in items if span[0] <= root["span"][0] and span[1] >= root["span"][1]]
        if not candidates:
            continue
        span = min(candidates,key=lambda s:s[1]-s[0])
        bindings = []
        for binding in observation["bindings"]:
            if span[0] <= binding["span"][0] and binding["span"][1] <= span[1]:
                mapped = inverse_span([binding["symbol"],binding["symbol"]],edit)
                bindings.append([binding["namespace"],inverse_span(binding["span"],edit),mapped[0] if mapped else None])
        result.append({"span":span,"shape":hashlib.sha256(json.dumps([shapes[root["id"]],bindings],sort_keys=True).encode()).hexdigest()})
    return result
