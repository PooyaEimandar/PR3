#!/usr/bin/env bash
# Downloads pinned models from config/pr3.toml into models/ and verifies each SHA-256.
#
# Usage: scripts/fetch-model.sh            the models [model] selects for desktop and mobile
#        scripts/fetch-model.sh ID...      specific models, e.g. qwen3.8-27b-q8
#        scripts/fetch-model.sh --list     every model the config knows
# PR3_MODELS_DIR overrides the destination folder.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONFIG="$ROOT/config/pr3.toml"
DEST_DIR="${PR3_MODELS_DIR:-$ROOT/models}"

# Python's tomllib (3.11+) reads the config; prints one model per line.
models() {
  python3 -I - "$CONFIG" "$@" <<'EOF'
import sys, tomllib
config = tomllib.load(open(sys.argv[1], "rb"))
known, args = config["models"], sys.argv[2:]
if args == ["--list"]:
    for id, m in known.items():
        print(f"{id:16} {m['file']:28} {m['size'] / 1e9:6.2f} GB  {m['repo']}", file=sys.stderr)
    sys.exit(0)
if args:
    ids = args
else:
    selected = config["model"]
    ids = list(dict.fromkeys(selected[k] for k in ("desktop", "mobile", "mobile_small")))
for id in ids:
    if id not in known:
        sys.exit(f"unknown model '{id}'; see scripts/fetch-model.sh --list")
    m = known[id]
    print(id, m["repo"], m["revision"], m["file"], m["sha256"], m["size"])
EOF
}

listing="$(models "$@")"
[ -z "$listing" ] && exit 0
mkdir -p "$DEST_DIR"

while read -r id repo revision file sha256 size; do
  dest="$DEST_DIR/$file"
  if [ -f "$dest" ] && echo "$sha256  $dest" | shasum -a 256 -c --status -; then
    echo "$id: $file already present and verified"
    continue
  fi
  partial="$dest.partial"
  echo "$id: downloading $file ($((size / 1000000)) MB) from $repo@${revision:0:7}"
  curl -fL --retry 3 -C - -o "$partial" \
    "https://huggingface.co/$repo/resolve/$revision/$file"
  echo "$sha256  $partial" | shasum -a 256 -c -
  mv "$partial" "$dest"
  echo "$id: saved $dest"
done <<< "$listing"
