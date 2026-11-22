#!/usr/bin/env python3
"""Writes the benchmark workloads under bench/c/workloads/ (doc/15, "Module budgets").
Deterministic, so every device runs the same bytes. Run from anywhere:

    python3 -I tools/gen-workloads.py           write every file
    python3 -I tools/gen-workloads.py --check   fail if a file would change"""

import json
import os
import random
import sys

sys.dont_write_bytecode = True  # no __pycache__ in the tree

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, os.path.join(HERE, "gen-vectors"))

import canonical_json  # noqa: E402


def dumps(v):
    return json.dumps(v, ensure_ascii=False, separators=(",", ":"))


def small():
    """One tool call's arguments, keys out of order: the common case."""
    return dumps({"unit": "celsius", "city": "Tehran", "days": 3, "include_hourly": False, "lang": "fa"})


def nested():
    """Objects and arrays ten levels deep, with keys to sort at every level."""
    rng = random.Random(0x4E455354)
    words = ["id", "name", "type", "items", "meta", "value", "tags", "next", "a", "z", "k", "score"]

    def node(depth):
        if depth == 0:
            return rng.choice([None, True, False, rng.randrange(1000), "leaf", 0.5])
        if rng.random() < 0.3:
            return [node(depth - 1) for _ in range(rng.randrange(1, 3))]
        keys = rng.sample(words, rng.randrange(2, 6))
        # one member goes deeper; the rest are leaves or shallow
        return {k: node(depth - 1 if i == 0 else min(depth - 1, rng.randrange(3))) for i, k in enumerate(keys)}

    return dumps({"roots": [node(10) for _ in range(8)]})


def numbers():
    """A long array of every kind of number: integers, negative integers, doubles in
    shortest and long forms, exponents."""
    rng = random.Random(0x4E554D42)
    out = []
    for i in range(2000):
        pick = i % 5
        if pick == 0:
            out.append(str(rng.getrandbits(rng.randrange(1, 64))))
        elif pick == 1:
            out.append(str(-rng.getrandbits(rng.randrange(1, 63)) - 1))
        elif pick == 2:
            out.append(repr(rng.uniform(-1e6, 1e6)))
        elif pick == 3:
            out.append("%.17g" % (rng.random() * 10.0 ** rng.randrange(-30, 30)))
        else:
            out.append("%de%d" % (rng.randrange(1, 99999), rng.randrange(-40, 40)))
    return "[" + ",".join(out) + "]"


def escapes():
    """Strings full of escapes and non-ASCII text: quotes, control characters,
    surrogate pairs, and Persian and English prose."""
    rng = random.Random(0x45534350)
    pieces = ['\\"', "\\\\", "\\n", "\\t", "\\u00e9", "\\ud83d\\ude00", "\\u0001", "\\/", "\\u0645\\u0631",
              "plain text ", "سلام دنیا ", "café ", "€ ", "😀 "]
    strings = []
    for _ in range(300):
        strings.append('"' + "".join(rng.choice(pieces) for _ in range(rng.randrange(4, 16))) + '"')
    return "[" + ",".join(strings) + "]"


FILES = [
    ("json/small.json", small),
    ("json/nested.json", nested),
    ("json/numbers.json", numbers),
    ("json/escapes.json", escapes),
]


def main(argv):
    check = "--check" in argv
    stale = []
    for rel, gen in FILES:
        text = gen()
        canonical_json.parse(text.encode("utf-8"))  # every workload is valid canonical-JSON input
        data = (text + "\n").encode("utf-8")
        path = os.path.join(ROOT, "bench", "c", "workloads", rel)
        try:
            with open(path, "rb") as f:
                old = f.read()
        except FileNotFoundError:
            old = None
        if old == data:
            continue
        if check:
            stale.append(rel)
            continue
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(data)
        print("wrote bench/c/workloads/" + rel)
    if stale:
        print("stale workloads (regenerate with python3 -I tools/gen-workloads.py):", file=sys.stderr)
        for rel in stale:
            print("  bench/c/workloads/" + rel, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
