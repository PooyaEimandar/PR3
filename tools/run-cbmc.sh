#!/bin/sh
# Runs the CBMC harnesses in test/cbmc/ (doc/14, "Model checking").
#
#   tools/run-cbmc.sh           every harness
#   tools/run-cbmc.sh NAME...   the named harnesses
set -eu
cd "$(dirname "$0")/.."
command -v cbmc >/dev/null || { echo "run-cbmc: cbmc is not installed (CBMC 6)" >&2; exit 2; }

checks="--bounds-check --pointer-check --pointer-overflow-check --signed-overflow-check --conversion-check --div-by-zero-check --undefined-shift-check --unwinding-assertions"

# Extra options per harness.
#   ring   CBMC has no model of Clang's C11 atomic builtins, which <stdatomic.h> expands to,
#          so the ring is checked in its fenced-word mode (the one boards without atomics
#          use): the index logic is the same in both modes, and cross-thread ordering is
#          ThreadSanitizer's job (test_ring_transfers_across_threads).
#   buddy  Clearing the free lists walks every order the type allows (PR3_BUDDY_ORDERS,
#          31); every other loop walks the orders of a 512-byte heap, at most 4. Its
#          address arithmetic names more objects than CBMC's default 8 object bits allow.
#   json_number  Inputs of up to six bytes need at most seven passes of any loop the harness
#          keeps; the exact decimal is left to the vectors and fuzzing (see the harness).
options() {
  case "$1" in
    ring) echo "-DPR3_RING_FENCED_WORDS=1" ;;
    buddy) echo "--unwindset pr3_buddy_init.0:32 --object-bits 12" ;;
    json_number)
      # the exact decimal's loops: no input the harness keeps may enter them
      echo "--unwindset right_shift.0:0,right_shift.1:0,right_shift.2:0,right_shift.3:0,left_shift.0:0,left_shift.1:0,left_shift.2:0,shift.0:0,shift.1:0,rounded_integer.0:0,rounded_integer.1:0,decimal_to_double.0:0,decimal_to_double.1:0"
      ;;
    *) echo "" ;;
  esac
}

# name: base sources and the loop bound (unwinding assertions check that it is enough)
harness() {
  case "$1" in
    arena) echo "src/base/arena.c 8" ;;
    pool) echo "src/base/arena.c src/base/pool.c 9" ;;
    buddy) echo "src/base/arena.c src/base/buddy.c 10" ;;
    ring) echo "src/base/arena.c src/base/ring.c 12" ;;
    buf) echo "src/base/str.c src/base/utf8.c 34" ;;
    utf8) echo "src/base/utf8.c 6" ;;
    json_number) echo "src/json/number.c 8" ;;
    *) return 1 ;;
  esac
}

names=${*:-"arena pool buddy ring buf utf8 json_number"}
failed=0
for name in $names; do
  spec=$(harness "$name") || { echo "run-cbmc: no harness '$name'" >&2; exit 2; }
  unwind=${spec##* }
  sources=${spec% *}
  echo "== $name (unwind $unwind)"
  # shellcheck disable=SC2086
  if ! cbmc "test/cbmc/$name.c" $sources -I include -I test/cbmc -DNDEBUG $(options "$name") $checks --unwind "$unwind"; then
    failed=1
  fi
done
exit $failed
