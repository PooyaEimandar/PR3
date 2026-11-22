#!/bin/sh
# Compares the symbols a freestanding image leaves undefined with the reviewed list for
# its target (doc/11, "The board contract"): the board's callbacks, the mem* functions
# and the compiler's runtime helpers, and nothing else.
#
#   tools/check-freestanding.sh NM ELF EXPECTED [SIZE]
#   PR3_WRITE=1 tools/check-freestanding.sh ...   rewrite EXPECTED after review
set -eu
nm=$1
elf=$2
expected=$3
size=${4:-}

[ -f "$elf" ] || { echo "check-freestanding: $elf was not built" >&2; exit 1; }
got=$("$nm" -u "$elf" | awk '{print $NF}' | sort -u)
if [ "${PR3_WRITE:-}" = 1 ]; then
  { echo "# Undefined symbols of $(basename "$elf"); reviewed. Regenerate with PR3_WRITE=1 tools/check-freestanding.sh."; echo "$got"; } > "$expected"
  echo "check-freestanding: wrote $expected"
  exit 0
fi
want=$(grep -v '^#' "$expected" | sort -u)
if [ "$got" != "$want" ]; then
  echo "check-freestanding: undefined symbols differ from $expected" >&2
  diff -u /dev/fd/3 /dev/fd/4 3<<EOF_WANT 4<<EOF_GOT >&2 || true
$want
EOF_WANT
$got
EOF_GOT
  exit 1
fi
echo "check-freestanding: $(echo "$got" | wc -l | tr -d ' ') external symbols, as reviewed:" $got
[ -n "$size" ] && [ -x "$size" ] && "$size" "$elf"
exit 0
