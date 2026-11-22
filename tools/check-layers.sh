#!/bin/sh
# The module dependency order (doc/02-architecture.md#modules): a module includes only
# headers of modules before it in PR3_LAYERS (cmake/pr3_module.cmake), trusted/ only
# base, json and crypto, and no module reaches into another's directory.
set -eu
cd "$(dirname "$0")/.."

layers=$(sed -n 's/^set(PR3_LAYERS \(.*\))$/\1/p' cmake/pr3_module.cmake)
trusted=$(sed -n 's/^set(PR3_TRUSTED_ALLOWED \(.*\))$/\1/p' cmake/pr3_module.cmake)
[ -n "$layers" ] || { echo "check-layers: PR3_LAYERS not found in cmake/pr3_module.cmake" >&2; exit 2; }

index_of() {
  i=0
  for l in $layers; do
    [ "$l" = "$1" ] && { echo $i; return; }
    i=$((i + 1))
  done
  echo -1
}

status=0
checked=0
check_file() { # file owner-module
  f=$1
  owner=$2
  own=$(index_of "$owner")
  includes=$(grep -nE '^[[:space:]]*#[[:space:]]*include[[:space:]]*[<"]' "$f" || true)
  [ -n "$includes" ] || return 0
  echo "$includes" | while IFS= read -r line; do
    target=$(echo "$line" | sed -E 's/.*include[[:space:]]*[<"]([^>"]*)[>"].*/\1/')
    case "$target" in
      ../*|*/../*) echo "$f: ${line%%:*}: includes another module's private file: $target"; echo x >&3 ;;
      pr3/*.h)
        mod=$(basename "$target" .h)
        idx=$(index_of "$mod")
        [ "$idx" -eq -1 ] && continue # compiler.h, limits.h, poison.h
        if [ "$idx" -gt "$own" ]; then
          echo "$f: ${line%%:*}: $owner may not include $target (layers point down)"
          echo x >&3
        fi
        if [ "$owner" = trusted ] && [ "$mod" != trusted ]; then
          case " $trusted " in *" $mod "*) ;; *) echo "$f: ${line%%:*}: trusted may not include $target"; echo x >&3 ;; esac
        fi
        ;;
    esac
  done
}

errors=$(mktemp)
trap 'rm -f "$errors"' EXIT
for l in $layers; do
  for f in $(find "src/$l" "include/pr3/$l.h" -type f \( -name '*.c' -o -name '*.h' \) 2>/dev/null | sort); do
    check_file "$f" "$l" 3>>"$errors"
    checked=$((checked + 1))
  done
done
if [ -s "$errors" ]; then
  exit 1
fi
echo "check-layers: $checked files follow the layer order"
