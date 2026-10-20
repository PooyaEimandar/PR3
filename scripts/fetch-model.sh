#!/usr/bin/env bash
# Downloads the pinned development model into models/ and verifies its SHA-256.
# Usage: scripts/fetch-model.sh [destination-dir]
set -euo pipefail

MODEL="Qwen3-0.6B-Q8_0.gguf"
REPO="Qwen/Qwen3-0.6B-GGUF"
REVISION="23749fefcc72300e3a2ad315e1317431b06b590a"
SHA256="9465e63a22add5354d9bb4b99e90117043c7124007664907259bd16d043bb031"

DEST_DIR="${1:-$(cd "$(dirname "$0")/.." && pwd)/models}"
DEST="$DEST_DIR/$MODEL"
mkdir -p "$DEST_DIR"

if [ -f "$DEST" ] && echo "$SHA256  $DEST" | shasum -a 256 -c --status -; then
  echo "$MODEL already present and verified: $DEST"
  exit 0
fi

PARTIAL="$DEST.partial"
echo "Downloading $MODEL (~640 MB) from $REPO@${REVISION:0:7}"
curl -fL --retry 3 -C - -o "$PARTIAL" \
  "https://huggingface.co/$REPO/resolve/$REVISION/$MODEL"

echo "$SHA256  $PARTIAL" | shasum -a 256 -c -
mv "$PARTIAL" "$DEST"
echo "Saved $DEST"
