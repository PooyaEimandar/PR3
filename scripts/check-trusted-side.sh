#!/usr/bin/env bash
# Keeps the trusted side separable (Roadmap §4.2): pr3-core's trusted modules may use
# each other and `types::Timestamp`, and nothing else in the crate — not the agent loop,
# the tool registry, traces, events or inference types. The agent side may depend on
# the trusted side; the reverse fails this check.
set -euo pipefail

dir="$(cd "$(dirname "$0")/.." && pwd)/crates/pr3-core/src/trusted"
bad="$(grep -nE '(crate|super::super)::' "$dir"/*.rs |
  grep -vE '(crate::trusted::|crate::types::Timestamp[;,} ])' || true)"
if [ -n "$bad" ]; then
  echo "the trusted side may use only itself and types::Timestamp from pr3-core:" >&2
  echo "$bad" >&2
  exit 1
fi
echo "trusted side: no dependency on the agent side"
