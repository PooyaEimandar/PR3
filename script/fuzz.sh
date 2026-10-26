#!/usr/bin/env bash
# Fuzzes PR3's parsers of untrusted input (docs Sec. 12) on the nightly pinned in
# fuzz/rust-toolchain.toml. Each target keeps its corpus in fuzz/corpus/<target> and
# its format's tokens in fuzz/dict/<target>.dict, seeded from fuzz/seed/<target> where
# real examples help; a failing input lands in
# fuzz/artifact/<target>/ and stops the run.
#
# Usage: script/fuzz.sh [seconds per target] [target...]   (default: 60 s, every target)
#        cargo +<nightly> fuzz run <target> fuzz/artifact/<target>/<file>   reproduces one
set -euo pipefail

cd "$(dirname "$0")/.."
root="$(pwd)"
nightly="$(sed -n 's/^channel = "\(.*\)"/\1/p' fuzz/rust-toolchain.toml)"
# This machine's own target, given explicitly: cargo-fuzz otherwise defaults to the
# one it was built for, which for a prebuilt binary can be musl, where the sanitizer
# does not run.
host="$(rustc "+$nightly" -vV | sed -n 's/^host: //p')"
seconds="${1:-60}"
[ $# -gt 0 ] && shift
targets="$*"
[ -n "$targets" ] || targets="$(cargo "+$nightly" fuzz list)"

for target in $targets; do
  echo "=== $target: $seconds s"
  mkdir -p "fuzz/corpus/$target" "fuzz/artifact/$target"
  # A target's format tokens, if it has any, so the fuzzer reaches the deep paths.
  dict=()
  [ -f "fuzz/dict/$target.dict" ] && dict=(-dict="$root/fuzz/dict/$target.dict")
  # Hand-written examples of real input, if the target has any, seed the corpus.
  seeds=()
  [ -d "fuzz/seed/$target" ] && seeds=("fuzz/seed/$target")
  cargo "+$nightly" fuzz run --target "$host" "$target" "fuzz/corpus/$target" ${seeds[@]+"${seeds[@]}"} -- \
    ${dict[@]+"${dict[@]}"} \
    -max_total_time="$seconds" \
    -artifact_prefix="$root/fuzz/artifact/$target/" \
    -print_final_stats=1
done
