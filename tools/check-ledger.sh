#!/bin/sh
# The test ledger (doc/20-test-ledger.md, SPEC-02): every test function in test/unit/
# is listed in the ledger, and every ledger row marked Written or Passing names a test
# that exists. Rows still Open may name tests not written yet.
set -eu
cd "$(dirname "$0")/.."
exec python3 -I - <<'PY'
import os, re, sys

tests = {}
for f in sorted(os.listdir("test/unit")):
    if f.endswith(".c"):
        for m in re.finditer(r"^static void (test_[a-z0-9_]+)\(pr3_t \*t\)", open(os.path.join("test/unit", f)).read(), re.M):
            tests[m.group(1)] = f

rows = {}
for line in open("doc/20-test-ledger.md", encoding="utf-8"):
    m = re.match(r"\|\s*`(test_[a-z0-9_]+)`\s*\|(.*)\|\s*$", line)
    if m:
        rows[m.group(1)] = m.group(2).split("|")[-1].strip()
mentioned = set(re.findall(r"`(test_[a-z0-9_]+)`", open("doc/20-test-ledger.md", encoding="utf-8").read()))

problems = []
for name, f in sorted(tests.items()):
    if name not in mentioned:
        problems.append("test/unit/%s: %s is not in the ledger" % (f, name))
for name, status in sorted(rows.items()):
    if status.lower() in ("written", "passing") and name not in tests:
        problems.append("doc/20-test-ledger.md: %s is %s but no test function has that name" % (name, status))
for p in problems:
    print(p)
print("check-ledger: %d test functions, %d ledger rows, %d problems" % (len(tests), len(rows), len(problems)))
sys.exit(1 if problems else 0)
PY
