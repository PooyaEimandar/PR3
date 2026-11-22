#!/bin/sh
# Every PR3_CHECK site (doc/03, "Panics on bad input"): a trap guards a broken internal
# invariant, never input, so each one is reviewed. tools/traps.txt is the reviewed list;
# this script fails when the code and the list differ.
#
#   tools/check-traps.sh           compare
#   tools/check-traps.sh --write   rewrite tools/traps.txt after review
set -eu
cd "$(dirname "$0")/.."

list() {
  echo "# Reviewed PR3_CHECK sites: file and condition. Regenerate with tools/check-traps.sh --write."
  find src include -type f \( -name '*.c' -o -name '*.h' \) ! -name compiler.h | sort |
    while IFS= read -r f; do
      grep -oE 'PR3_CHECK\(.*\);' "$f" 2>/dev/null | sed "s|^|$f: |" || true
    done
}

if [ "${1:-}" = "--write" ]; then
  list > tools/traps.txt
  echo "check-traps: wrote tools/traps.txt ($(($(wc -l < tools/traps.txt) - 1)) sites)"
  exit 0
fi

# A trap split over lines would drop out of the list unseen; each one fits on one line.
split=$(find src include -type f \( -name '*.c' -o -name '*.h' \) ! -name compiler.h -exec grep -n 'PR3_CHECK(' {} + |
  grep -vE 'PR3_CHECK\(.*\);' || true)
if [ -n "$split" ]; then
  echo "$split" >&2
  echo "check-traps: a PR3_CHECK must fit on one line; name the value it checks first" >&2
  exit 1
fi

now=$(mktemp)
trap 'rm -f "$now"' EXIT
list > "$now"
if ! diff -u tools/traps.txt "$now"; then
  echo "check-traps: PR3_CHECK sites changed; review them, then run tools/check-traps.sh --write" >&2
  exit 1
fi
echo "check-traps: $(($(wc -l < "$now") - 1)) reviewed sites"
