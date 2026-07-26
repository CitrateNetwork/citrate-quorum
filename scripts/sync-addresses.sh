#!/usr/bin/env bash
# created: 2026-07-26 | branch: feat/qrm-s6-anchor-wiring
# author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
# status: active
#
# citrate-quorum — vendor the canonical chain-40204 address books (WP-Z).
#
# The federation's single source of truth is
# citrate-chain/contracts/addresses/. Every consumer vendors a copy rather than
# hardcoding addresses, so a redeploy is a one-file change here and not an edit
# scattered across the app (CLAUDE.md rule 8).
#
# TWO books, deliberately kept apart:
#
#   40204.json      the main book — MeetingRegistry, AnchorRegistry, the
#                   marketplace/inference contracts.
#   bfr-40204.json  the governance/RBAC book — TenantHierarchy,
#                   ClassificationRegistry, AgentDecisionRegistryV2, …
#
# They are NOT merged. Three names (ComputeVerifier, ComputeMarketplace,
# TEEAttestationRegistry) appear in both at DIFFERENT addresses, so a merge
# would silently pick a winner and the app would read a contract the operator
# never pointed it at. Each book is loaded by name and says which file answered.
#
# The BFR file is a flat {name: address} map with no chain id. It is rewritten
# here into the same envelope as the main book, taking chainId/rpcUrl from it,
# so one parser reads both and the chain id is explicit in the vendored artifact
# rather than assumed by the code.
#
# Usage: scripts/sync-addresses.sh [path-to-citrate-chain]
set -euo pipefail
cd "$(dirname "$0")/.."
CHAIN="${1:-../citrate-chain}"
SRC="$CHAIN/contracts/addresses/40204.json"
SRC_BFR="$CHAIN/contracts/addresses/bfr-40204.json"
DST="src-tauri/src/generated/addresses.json"
DST_BFR="src-tauri/src/generated/addresses-bfr.json"

[ -f "$SRC" ] || { echo "canonical book not found at $SRC" >&2; exit 1; }
[ -f "$SRC_BFR" ] || { echo "BFR book not found at $SRC_BFR" >&2; exit 1; }
mkdir -p "$(dirname "$DST")"
cp "$SRC" "$DST"
echo "vendored $SRC -> $DST"
uv run python3 - "$DST" "$SRC_BFR" "$DST_BFR" <<'PY'
import json, sys

main_path, bfr_src, bfr_dst = sys.argv[1], sys.argv[2], sys.argv[3]
d = json.load(open(main_path))
c = d["contracts"]
print(f"  chainId {d['chainId']}  contracts {len(c)}")
for n in ("MeetingRegistry", "AnchorRegistry"):
    print(f"  {n}: {c.get(n, '— ABSENT (anchoring will report unavailable)')}")

flat = json.load(open(bfr_src))
book = {
    "chainId": d["chainId"],
    "rpcUrl": d.get("rpcUrl"),
    "comment": (
        "Governance/RBAC book (BFR), vendored from contracts/addresses/bfr-40204.json "
        "by scripts/sync-addresses.sh. chainId/rpcUrl come from the main 40204 book. "
        "Kept separate from the main book: three names collide across the two at "
        "different addresses."
    ),
    "contracts": flat,
}
with open(bfr_dst, "w") as f:
    json.dump(book, f, indent=2)
    f.write("\n")
print(f"vendored {bfr_src} -> {bfr_dst}")
print(f"  bfr contracts {len(flat)}")
print(f"  TenantHierarchy: {flat.get('TenantHierarchy', '— ABSENT (tenancy will report unavailable)')}")
PY
