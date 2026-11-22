#!/bin/sh
# Runs one fuzz target for a while from its seed corpus, keeping what it finds.
#   tools/fuzz.sh TARGET [SECONDS]    e.g. tools/fuzz.sh json 3600
# Needs the fuzz preset built: cmake --preset fuzz && cmake --build --preset fuzz
set -eu
target=${1:?target}
seconds=${2:-60}
root=$(cd "$(dirname "$0")/.." && pwd)
bin="$root/build/fuzz/test/fuzz/fuzz_$target"
[ -x "$bin" ] || { echo "fuzz.sh: $bin not built" >&2; exit 2; }
seeds="$root/build/fuzz/test/fuzz/seeds/$target"
corpus="$root/build/fuzz/corpus/$target"
python3 -I "$root/tools/fuzz-seeds.py" "$root/test/vectors" "$root/build/fuzz/test/fuzz/seeds"
mkdir -p "$corpus" "$root/build/fuzz/crashes/$target"
dict=""
[ -f "$root/test/fuzz/$target.dict" ] && dict="-dict=$root/test/fuzz/$target.dict"
cd "$root/build/fuzz/crashes/$target"
# shellcheck disable=SC2086
exec "$bin" -max_total_time="$seconds" -print_final_stats=1 -timeout=10 -rss_limit_mb=4096 \
    $dict "$corpus" "$seeds"
