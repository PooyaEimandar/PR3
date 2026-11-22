#!/bin/sh
# Fetch third-party sources at the commits pinned in third_party/manifest.txt.
#
#   tools/fetch-third-party.sh               fetch every entry
#   tools/fetch-third-party.sh NAME...       fetch the named entries
#
# Each entry is a shallow, sparse checkout of exactly one commit: only the listed
# paths are downloaded, and the checkout is refused unless HEAD equals the pinned hash.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
manifest="$root/third_party/manifest.txt"

want() {
  [ $# -eq 0 ] && return 0
  for w in "$@"; do [ "$w" = "$name" ] && return 0; done
  return 1
}

fetch() {
  dir="$root/third_party/$name"
  if [ -d "$dir/.git" ] && [ "$(git -C "$dir" rev-parse HEAD 2>/dev/null)" = "$commit" ]; then
    echo "$name: at $commit"
    return 0
  fi
  echo "$name: fetching $commit from $repo ($paths)"
  rm -rf "$dir"
  git init -q "$dir"
  git -C "$dir" remote add origin "$repo"
  git -C "$dir" sparse-checkout set --no-cone $paths
  git -C "$dir" fetch -q --depth 1 --filter=blob:none origin "$commit"
  git -C "$dir" -c advice.detachedHead=false checkout -q FETCH_HEAD
  got=$(git -C "$dir" rev-parse HEAD)
  if [ "$got" != "$commit" ]; then
    echo "$name: expected $commit, got $got" >&2
    rm -rf "$dir"
    exit 1
  fi
  echo "$name: at $commit"
}

found=0
while read -r name repo commit paths; do
  case "$name" in ''|'#'*) continue ;; esac
  want "$@" || continue
  found=1
  fetch
done < "$manifest"

if [ "$found" -eq 0 ]; then
  echo "no matching entry in $manifest" >&2
  exit 1
fi
