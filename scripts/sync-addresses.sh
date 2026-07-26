#!/usr/bin/env bash
# created: 2026-07-26 | branch: feat/qrm-s6-anchor-wiring
# author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
# status: active
#
# citrate-quorum — vendor the canonical chain-40204 address book (WP-Z).
#
# The federation's single source of truth is
# citrate-chain/contracts/addresses/40204.json. Every consumer vendors a copy
# rather than hardcoding addresses, so a redeploy is a one-file change here and
# not an edit scattered across the app (CLAUDE.md rule 8).
#
# Usage: scripts/sync-addresses.sh [path-to-citrate-chain]
set -euo pipefail
cd "$(dirname "$0")/.."
CHAIN="${1:-../citrate-chain}"
SRC="$CHAIN/contracts/addresses/40204.json"
DST="src-tauri/src/generated/addresses.json"

[ -f "$SRC" ] || { echo "canonical book not found at $SRC" >&2; exit 1; }
mkdir -p "$(dirname "$DST")"
cp "$SRC" "$DST"
echo "vendored $SRC -> $DST"
uv run python3 - "$DST" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
c = d["contracts"]
print(f"  chainId {d['chainId']}  contracts {len(c)}")
for n in ("MeetingRegistry", "AnchorRegistry"):
    print(f"  {n}: {c.get(n, '— ABSENT (anchoring will report unavailable)')}")
PY
