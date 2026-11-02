"""Generate JSON conformance cases and benchmark inputs under target/json-bench."""
import json
import pathlib
import random
import sys

OUT = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "target/json-bench")

# (name, text, valid per RFC 8259). Invalid cases cover each grammar edge.
CASES = [
    ("empty_object", "{}", True), ("empty_array", "[]", True),
    ("scalar_string", '"x"', True), ("scalar_number", "0", True),
    ("scalar_true", "true", True), ("scalar_null", " null ", True),
    ("nested", '{"a":[1,{"b":[[],{}]}],"c":{"d":null}}', True),
    ("escapes", r'["\"\\\/\b\f\n\r\tA😀"]', True),
    ("unicode_raw", '["é ☃ 😀"]', True), ("lone_surrogate_escape", r'["\uD800"]', True),
    ("numbers", "[0,-0,1.5,-1.5e10,1E+2,2e-3,123456789012345678901234567890]", True),
    ("whitespace", ' \t\n\r[ 1 ,\n2 ] \n', True), ("duplicate_keys", '{"a":1,"a":2}', True),
    ("deep_200", "[" * 200 + "]" * 200, True), ("deep_50000", "[" * 50000 + "]" * 50000, True),
    ("empty_input", "", False), ("only_space", "   ", False),
    ("trailing_comma_array", "[1,]", False), ("trailing_comma_object", '{"a":1,}', False),
    ("leading_comma", "[,1]", False), ("double_comma", "[1,,2]", False),
    ("missing_colon", '{"a" 1}', False), ("missing_value", '{"a":}', False),
    ("unquoted_key", "{a:1}", False), ("single_quotes", "['x']", False),
    ("unclosed_array", "[1", False), ("unclosed_object", '{"a":1', False),
    ("unclosed_string", '["abc]', False), ("extra_close", "[1]]", False),
    ("two_roots", "[] []", False), ("leading_zero", "[01]", False),
    ("plus_number", "[+1]", False), ("dot_start", "[.5]", False),
    ("dot_end", "[5.]", False), ("exp_no_digits", "[1e]", False),
    ("hex_number", "[0x10]", False), ("nan", "[NaN]", False),
    ("infinity", "[Infinity]", False), ("minus_only", "[-]", False),
    ("bad_escape", r'["\x41"]', False), ("short_unicode", r'["\u12"]', False),
    ("raw_tab_in_string", '["a\tb"]', False), ("raw_newline_in_string", '["a\nb"]', False),
    ("capital_true", "[True]", False), ("truex", "[truex]", False),
    ("nul", "[nul]", False), ("key_not_string", "{1:2}", False),
    ("line_comment", "[1 // c\n]", False), ("block_comment", "[1 /* c */]", False),
    ("nbsp_whitespace", "[1, 2]", False), ("bom", "﻿[]", False),
]


def write_cases():
    cases = OUT / "cases"
    cases.mkdir(parents=True, exist_ok=True)
    expected = {}
    for name, text, valid in CASES:
        path = cases / f"{name}.json"
        path.write_bytes(text.encode("utf-8", "surrogatepass"))
        expected[str(path)] = valid
    (OUT / "expected.json").write_text(json.dumps(expected, indent=1))


def record(rng, i):
    return {
        "id": i, "guid": f"{rng.getrandbits(128):032x}", "active": rng.random() < 0.5,
        "balance": round(rng.uniform(-1e4, 1e6), 2), "age": rng.randint(18, 90),
        "name": rng.choice(["Ada", "Grace", "Linus", "Barbara", "Ken"]) + f" #{i}",
        "email": f"user{i}@example.com", "tags": [rng.choice("abcdefgh") * rng.randint(1, 8) for _ in range(rng.randint(0, 6))],
        "location": {"lat": rng.uniform(-90, 90), "lon": rng.uniform(-180, 180)},
        "about": "Lorem ipsum \"dolor\" sit amet\n\tconsectetur é中 ".strip() * rng.randint(1, 4),
        "friends": [{"id": rng.randint(0, 10**6), "name": "F"} for _ in range(rng.randint(0, 3))],
        "manager": None,
    }


SHAPES = {
    # Typical API payload: an array of records, pretty-printed.
    "records": lambda rng, i: record(rng, i),
    # Many numbers, few strings.
    "numbers": lambda rng, i: [rng.uniform(-1e9, 1e9) if rng.random() < .5 else rng.randint(-10**9, 10**9) for _ in range(32)],
    # Long strings with escapes.
    "strings": lambda rng, i: "x" * rng.randint(200, 2000) + "\\\"\n" + "y" * rng.randint(0, 500),
}


def write_inputs(sizes):
    for shape, make in SHAPES.items():
        for label, size in sizes:
            rng = random.Random(f"{shape}-{size}")
            parts, total, i = [], 2, 0
            while total < size:
                part = json.dumps(make(rng, i), indent=2 if shape == "records" else None)
                parts.append(part)
                total += len(part.encode()) + 2
                i += 1
            (OUT / f"{shape}-{label}.json").write_text("[\n" + ",\n".join(parts) + "\n]\n")


if __name__ == "__main__":
    write_cases()
    write_inputs([("64k", 64 << 10), ("1m", 1 << 20), ("16m", 16 << 20)])
    print(OUT)
