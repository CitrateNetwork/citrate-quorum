#!/usr/bin/env bash
# created: 2026-09-25 | branch: fix/r2-sod-roster-compat
# author: Claude Opus 5.5 (1M context), directed by @SaulBuilds
# status: active
#
# verify-template-hashes.sh — prove the vendored template artifacts are exactly
# what a fresh build of the pinned citrate-chain revision produces.
#
# `src-tauri/src/template_hashes.rs` records the citrate-chain revision and the
# toolchain the artifacts were built with (CHAIN_REV, FORGE_VERSION,
# SOLC_VERSION). This script:
#
#   1. refuses a chain checkout that is not at CHAIN_REV, or is dirty under
#      contracts/src (the bytes would be of different source);
#   2. refuses a solc (and, when building, a forge) that is not the pinned one
#      (via-IR bytecode moves between compiler versions);
#   3. with --build, runs the same compile chain CI runs
#      (`forge build --use <solc>`, whole repo, default profile);
#   4. for every template, checks keccak256(artifacts/templates/<T>.hex) ==
#      its pinned row; and for every template in BUILT_FROM_CHAIN_REV, that the
#      row also == keccak256(out/<T>.sol/<T>.json bytecode) of the fresh build.
#      Rows outside that list are registry reads (verify them with the vendor
#      script's live mode); a whole-repo build is not expected to reproduce
#      them, so they are reported, not compared.
#
# Any mismatch exits 1. It never rewrites anything; vendoring is
# scripts/vendor-template-artifacts.sh.
#
# Usage:
#   scripts/verify-template-hashes.sh [--build] [path-to-citrate-chain]
# Env:
#   FORGE   forge binary (default: forge on PATH)
#   CAST    cast binary  (default: cast next to FORGE, else on PATH)
set -euo pipefail

BUILD=0
if [ "${1:-}" = "--build" ]; then BUILD=1; shift; fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CHAIN="${1:-${CITRATE_CHAIN_DIR:-$ROOT/../citrate-chain}}"
CONTRACTS="$CHAIN/contracts"
HASHES="$ROOT/src-tauri/src/template_hashes.rs"
ART="$ROOT/src-tauri/artifacts/templates"

FORGE="${FORGE:-forge}"
if [ -z "${CAST:-}" ]; then
  if [ -x "$(dirname "$(command -v "$FORGE")")/cast" ]; then CAST="$(dirname "$(command -v "$FORGE")")/cast"; else CAST=cast; fi
fi
if command -v uv >/dev/null 2>&1; then PY=(uv run python3); else PY=(python3); fi

pinned() { sed -n "s/^pub const $1: &str = \"\(.*\)\";$/\1/p" "$HASHES"; }
REV="$(pinned CHAIN_REV)"
WANT_FORGE="$(pinned FORGE_VERSION)"
WANT_SOLC="$(pinned SOLC_VERSION)"
[ -n "$REV" ] && [ -n "$WANT_FORGE" ] && [ -n "$WANT_SOLC" ] || {
  echo "template_hashes.rs does not pin CHAIN_REV / FORGE_VERSION / SOLC_VERSION" >&2; exit 2; }

have_rev="$(git -C "$CHAIN" rev-parse HEAD 2>/dev/null || true)"
case "$have_rev" in
  "$REV"*) ;;
  *) echo "citrate-chain at '$CHAIN' is at '${have_rev:-<not a git checkout>}', not the pinned $REV" >&2; exit 2 ;;
esac
if [ -n "$(git -C "$CHAIN" status --porcelain -- contracts/src 2>/dev/null)" ]; then
  echo "citrate-chain contracts/src is dirty; the build would not be of the pinned source" >&2; exit 2
fi

if [ "$BUILD" -eq 1 ]; then
  # Only a build needs the pinned forge; an existing out/ is checked by its
  # recorded solc version and, above all, by the bytes themselves.
  have_forge="$("$FORGE" --version | sed -n 's/^forge Version: \([^ -]*\).*/\1/p')"
  [ "$have_forge" = "$WANT_FORGE" ] || { echo "forge $have_forge, pinned $WANT_FORGE (set FORGE=)" >&2; exit 2; }
  echo ">>> forge build --use $WANT_SOLC in $CONTRACTS"
  (cd "$CONTRACTS" && "$FORGE" build --use "$WANT_SOLC" --threads "${FORGE_THREADS:-1}")
fi
[ -d "$CONTRACTS/out" ] || { echo "no forge output at $CONTRACTS/out (run with --build)" >&2; exit 2; }

BUILT_LIST="$(sed -n '/^pub const BUILT_FROM_CHAIN_REV/,/^\];/p' "$HASHES" | sed -n 's/^ *"\([A-Za-z]*\)",$/\1/p')"
[ -n "$BUILT_LIST" ] || { echo "BUILT_FROM_CHAIN_REV is empty: nothing here is build-sourced (live-registry vendoring)"; }

fail=0
checked=0
while IFS='|' read -r name want; do
  vend_hash="$("$CAST" keccak "$(tr -d '[:space:]' < "$ART/$name.hex")")"
  if [ "$vend_hash" != "$want" ]; then
    printf '  MISMATCH %s: vendored %s, pinned %s\n' "$name" "$vend_hash" "$want"; fail=1; continue
  fi
  if ! printf '%s\n' "$BUILT_LIST" | grep -qx "$name"; then
    printf '  reg %-22s %s  (registry-sourced row; vendored bytes match it)\n' "$name" "$want"
    continue
  fi
  art="$CONTRACTS/out/$name.sol/$name.json"
  [ -f "$art" ] || { echo "  MISSING build artifact $art"; fail=1; continue; }
  built="$("${PY[@]}" -c "import json,sys; print(json.load(open(sys.argv[1]))['bytecode']['object'])" "$art")"
  solc="$("${PY[@]}" -c "import json,sys; print(json.loads(json.load(open(sys.argv[1]))['rawMetadata'])['compiler']['version'])" "$art" 2>/dev/null || echo unknown)"
  case "$solc" in "$WANT_SOLC"*) ;; *) echo "  $name built with solc $solc, pinned $WANT_SOLC"; fail=1; continue ;; esac
  built_hash="$("$CAST" keccak "$built")"
  if [ "$built_hash" = "$want" ]; then
    printf '  ok  %-22s %s  (fresh build == vendored == pinned)\n' "$name" "$want"
    checked=$((checked+1))
  else
    printf '  MISMATCH %s\n     pinned/vendored %s\n     fresh build     %s\n' "$name" "$want" "$built_hash"
    fail=1
  fi
done < <(sed -n 's/^ *("\([A-Za-z]*\)", "\(0x[0-9a-f]*\)"),$/\1|\2/p' "$HASHES")

n="$(grep -c '^ *("[A-Za-z]*", "0x' "$HASHES")"
[ "$n" -gt 0 ] || { echo "no pinned hashes parsed from $HASHES" >&2; exit 2; }
[ "$fail" -eq 0 ] || { echo ">>> vendored templates do NOT match a fresh build of $REV" >&2; exit 1; }
want_built="$(printf '%s\n' "$BUILT_LIST" | grep -c . || true)"
[ "$checked" -eq "$want_built" ] || { echo ">>> only $checked of $want_built build-sourced rows were checked" >&2; exit 1; }
echo ">>> $n rows: all vendored bytes match their pins; $checked build-sourced rows match a fresh build of citrate-chain $REV (forge $WANT_FORGE, solc $WANT_SOLC)"
