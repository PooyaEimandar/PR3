#!/bin/sh
# Documentation links (doc/14, "Format"): every relative link in README.md and doc/
# names a file that exists, and every #anchor names a heading in that file.
set -eu
cd "$(dirname "$0")/.."
exec python3 -I - "$@" <<'PY'
import os, re, sys

def slug(heading):
    s = heading.strip().lower()
    s = re.sub(r"[`*_]", "", s)
    s = re.sub(r"[^\w\- ]", "", s)
    return s.replace(" ", "-")

def anchors(path, cache={}):
    if path not in cache:
        found, fence = set(), False
        for line in open(path, encoding="utf-8"):
            if line.startswith("```"):
                fence = not fence
            elif not fence and line.startswith("#"):
                found.add(slug(line.lstrip("#")))
        cache[path] = found
    return cache[path]

files = ["README.md"] + sorted(os.path.join("doc", f) for f in os.listdir("doc") if f.endswith(".md"))
problems, count = [], 0
for f in files:
    text, fence = [], False
    for line in open(f, encoding="utf-8"):
        if line.startswith("```"):
            fence = not fence
        elif not fence:
            text.append(re.sub(r"`[^`]*`", "", line))
    for target in re.findall(r"\]\(([^)\s]+)\)", "".join(text)):
        if re.match(r"[a-z]+:", target):
            continue
        count += 1
        path, _, anchor = target.partition("#")
        dest = os.path.normpath(os.path.join(os.path.dirname(f), path)) if path else f
        if not os.path.exists(dest):
            problems.append("%s: missing file %s" % (f, target))
        elif anchor and dest.endswith(".md") and anchor not in anchors(dest):
            problems.append("%s: missing anchor %s" % (f, target))
for p in problems:
    print(p)
print("check-links: %d links, %d problems" % (count, len(problems)))
sys.exit(1 if problems else 0)
PY
