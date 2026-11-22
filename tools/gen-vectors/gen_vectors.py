#!/usr/bin/env python3
"""Writes PR3's golden vectors under test/vectors/ from the specification, independently
of the C code (doc/19 D-15). Run from anywhere:

    python3 -I tools/gen-vectors/gen_vectors.py           write every file
    python3 -I tools/gen-vectors/gen_vectors.py --check   fail if a file would change

Each file is JSON Lines. Its first line names the format, its version and this command;
every other line is one vector. A changed vector is a format change (doc/19 D-21)."""

import json
import os
import sys

sys.dont_write_bytecode = True  # no __pycache__ in the tree

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)

import canonical_json  # noqa: E402
import core_steps  # noqa: E402
import siphash  # noqa: E402

COMMAND = "python3 -I tools/gen-vectors/gen_vectors.py"

# (path under test/vectors, format name, version, generator)
FILES = [
    ("base/siphash.jsonl", "siphash", 1, siphash.vectors),
] + canonical_json.FILES + core_steps.FILES


def render(name, version, rows):
    header = {"format": name, "version": version, "generated_by": COMMAND}
    lines = [json.dumps(header, ensure_ascii=False, separators=(",", ":"))]
    for row in rows:
        lines.append(json.dumps(row, ensure_ascii=False, separators=(",", ":")))
    return ("\n".join(lines) + "\n").encode("utf-8", "surrogatepass")


def main(argv):
    check = "--check" in argv
    stale = []
    for rel, name, version, gen in FILES:
        path = os.path.join(ROOT, "test", "vectors", rel)
        data = render(name, version, gen())
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
        print("wrote test/vectors/" + rel)
    if stale:
        print("stale vector files (regenerate with %s):" % COMMAND, file=sys.stderr)
        for rel in stale:
            print("  test/vectors/" + rel, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
