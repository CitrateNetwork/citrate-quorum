#!/usr/bin/env bash
# created: 2026-07-28 | branch: feat/qrm-s7-vendor-artifacts
# author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
# status: active
#
# vendor-template-artifacts.sh — copy the AUDITED creation code of the eight
# governance templates out of citrate-chain and into this app, and prove each
# one hashes to the value registered on chain.
#
# WHY THIS EXISTS
#
# GovernanceProtocolFactory.deployProtocol takes the creation code as an
# argument and refuses it unless `keccak256(creationCode)` equals the
# `initCodeHash` the registry pinned (GF-2). So this app cannot deploy a
# governance protocol without carrying the exact audited bytes — it cannot
# derive them, and it must not invent them.
#
# WHY IT VERIFIES AGAINST THE CHAIN RATHER THAN THE BUILD
#
# Copying a file out of `forge` output proves only that the copy matches
# whatever was compiled locally, which may not be what was registered — a
# different compiler version, a dirty tree, or an edited source all produce
# bytes that look fine and fail GF-2 at the ceremony, after a human has already
# approved. So every artifact is checked against `get(templateId).initCodeHash`
# read from the LIVE registry. A mismatch fails this script loudly, here, where
# it is cheap.
#
# BUILD MODE (--from-build)
#
# When citrate-chain changes a template's source, the new bytes cannot match the
# live registry until the templates are registered again, so the live check
# above would refuse them. `--from-build` vendors from a clean build of a known
# chain revision instead, and records the revision and toolchain in
# template_hashes.rs. scripts/verify-template-hashes.sh then proves the vendored
# bytes equal a fresh build of that revision. It proves the bytes are the
# audited SOURCE; it does not prove they are registered. Re-run without the
# flag once they are.
#
# Name the templates whose source changed after `--from-build`. The others keep
# their vendored, registry-verified bytes (their hash row must still match the
# file, or this refuses): a whole-repo via-IR build moves the bytes of an
# UNCHANGED template too, and re-vendoring it would make a template that
# deploys today stop matching its registered hash for no source reason.
#
# Usage:
#   scripts/vendor-template-artifacts.sh [path-to-citrate-chain]
#   scripts/vendor-template-artifacts.sh --from-build <path-to-citrate-chain> <Template>...
#
# Env:
#   RPC_URL   default https://rpc.citrate.ai
#   FORGE     forge binary used for the build (recorded; default forge on PATH)
#   SOLC      solc version the build used (default 0.8.36, chain CI's)
set -euo pipefail

if command -v uv >/dev/null 2>&1; then PY=(uv run python3); else PY=(python3); fi

FROM_BUILD=0
if [ "${1:-}" = "--from-build" ]; then FROM_BUILD=1; shift; fi

CHAIN="${1:-../citrate-chain}"
[ "$#" -gt 0 ] && shift
REBUILD=("$@")
if [ "$FROM_BUILD" -eq 1 ] && [ "${#REBUILD[@]}" -eq 0 ]; then
  echo "--from-build needs the templates whose source changed, by name" >&2
  exit 2
fi
RPC_URL="${RPC_URL:-https://rpc.citrate.ai}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT="$ROOT/src-tauri/artifacts/templates"
CONTRACTS="$CHAIN/contracts"

TEMPLATES=(
  ThresholdApproval
  ClassificationGate
  BudgetedAutonomy
  SegregationOfDuties
  TimeBoundedElevation
  ChangeControlBoard
  SupplierAdmission
  IncidentEscalation
)

[ -d "$CONTRACTS/out" ] || {
  echo "no forge output at $CONTRACTS/out — run \`forge build\` in citrate-chain first" >&2
  exit 2
}

CHAIN_REV="$(git -C "$CHAIN" rev-parse HEAD)"
if [ -n "$(git -C "$CHAIN" status --porcelain -- contracts/src)" ]; then
  echo "citrate-chain contracts/src is dirty — vendor only from a clean checkout" >&2
  exit 2
fi
FORGE_VERSION="$("${FORGE:-forge}" --version | sed -n 's/^forge Version: \([^ -]*\).*/\1/p')"
SOLC_VERSION="${SOLC:-0.8.36}"

if [ "$FROM_BUILD" -eq 1 ]; then
  echo ">>> vendoring ${REBUILD[*]} from the build of citrate-chain $CHAIN_REV (forge $FORGE_VERSION, solc $SOLC_VERSION)"
  mkdir -p "$OUT"
  PREV="$ROOT/src-tauri/src/template_hashes.rs"
  fail=0
  HASH_ROWS=()
  BUILT=()
  for t in "${TEMPLATES[@]}"; do
    rebuild=0
    for r in "${REBUILD[@]}"; do [ "$r" = "$t" ] && rebuild=1; done
    if [ "$rebuild" -eq 0 ]; then
      prev="$(sed -n "s/^ *(\"$t\", \"\(0x[0-9a-f]*\)\"),\$/\1/p" "$PREV")"
      have="$(cast keccak "$(tr -d '[:space:]' < "$OUT/$t.hex")")"
      if [ -z "$prev" ] || [ "$prev" != "$have" ]; then
        echo "  KEEP REFUSED $t: vendored bytes hash $have, previous row '${prev:-<none>}'"
        fail=1; continue
      fi
      HASH_ROWS+=("$t|$prev")
      printf '  %-22s %s  (kept)\n' "$t" "$prev"
      continue
    fi
    art="$CONTRACTS/out/$t.sol/$t.json"
    [ -f "$art" ] || { echo "  MISSING artifact: $art"; fail=1; continue; }
    code="$("${PY[@]}" -c "
import json
print(json.load(open('$art'))['bytecode']['object'])
")"
    h="$(cast keccak "$code")"
    printf '%s\n' "$code" > "$OUT/$t.hex"
    HASH_ROWS+=("$t|$h")
    BUILT+=("$t")
    printf '  %-22s %s  (%s bytes, built)\n' "$t" "$h" "$(( (${#code} - 2) / 2 ))"
  done
  [ "$fail" -eq 0 ] || { echo ">>> REFUSING to vendor" >&2; exit 1; }
  [ "${#BUILT[@]}" -eq "${#REBUILD[@]}" ] || { echo ">>> REFUSING: a named template is not one of the eight" >&2; exit 1; }
  SOURCE_NOTE="rows named in BUILT_FROM_CHAIN_REV were BUILT from citrate-chain $CHAIN_REV with forge $FORGE_VERSION and solc $SOLC_VERSION (--from-build) and are NOT yet registered: register them, then re-vendor without --from-build. Every other row was read from GovernanceTemplateRegistry when it was last vendored."
else
  REGISTRY="$("${PY[@]}" -c "
import json
print(json.load(open('$CONTRACTS/addresses/40204.json'))['contracts']['GovernanceTemplateRegistry'])
")"
  [ -n "$REGISTRY" ] || { echo "GovernanceTemplateRegistry is not in the address book" >&2; exit 2; }

  echo ">>> registry $REGISTRY on $RPC_URL"
  mkdir -p "$OUT"

  fail=0
  for t in "${TEMPLATES[@]}"; do
    art="$CONTRACTS/out/$t.sol/$t.json"
    [ -f "$art" ] || { echo "  MISSING artifact: $art"; fail=1; continue; }

    code="$("${PY[@]}" -c "
import json
print(json.load(open('$art'))['bytecode']['object'])
")"
    local_hash="$(cast keccak "$code")"

    id="$(cast call "$REGISTRY" 'templateId(string,uint32)(bytes32)' "$t" 1 --rpc-url "$RPC_URL")"
    chain_hash="$(cast call "$REGISTRY" \
      'get(bytes32)((bytes32,string,uint32,bytes32,bytes32,string,bool,uint64))' \
      "$id" --rpc-url "$RPC_URL" 2>/dev/null | tr -d '()' | cut -d, -f4 | tr -d ' ')"

    if [ "$local_hash" != "$chain_hash" ]; then
      echo "  MISMATCH $t"
      echo "     local $local_hash"
      echo "     chain $chain_hash"
      echo "     the bytes here are NOT the bytes that were registered — do not vendor them"
      fail=1
      continue
    fi

    printf '%s\n' "$code" > "$OUT/$t.hex"
    printf '  %-22s %s  (%s bytes)\n' "$t" "$local_hash" "$(( (${#code} - 2) / 2 ))"
  done

  [ "$fail" -eq 0 ] || { echo ">>> REFUSING to vendor: at least one artifact does not match the chain" >&2; exit 1; }
  HASH_ROWS=()
  for t in "${TEMPLATES[@]}"; do
    id="$(cast call "$REGISTRY" 'templateId(string,uint32)(bytes32)' "$t" 1 --rpc-url "$RPC_URL")"
    h="$(cast call "$REGISTRY" \
      'get(bytes32)((bytes32,string,uint32,bytes32,bytes32,string,bool,uint64))' \
      "$id" --rpc-url "$RPC_URL" 2>/dev/null | tr -d '()' | cut -d, -f4 | tr -d ' ')"
    HASH_ROWS+=("$t|$h")
  done
  BUILT=()
  SOURCE_NOTE="every row REGISTERED ON CHAIN: read from GovernanceTemplateRegistry at vendoring time; built from citrate-chain $CHAIN_REV."
fi

# The pinned hashes the Rust test asserts against, regenerated here so the test
# and the artifacts cannot drift apart silently.
{
  echo "// Generated by scripts/vendor-template-artifacts.sh — do not edit by hand."
  echo "//"
  echo "// Test-only: lib.rs gates this table behind #[cfg(test)]. The test in"
  echo "// deploy.rs asserts every embedded artifact hashes to exactly its row, and"
  echo "// scripts/verify-template-hashes.sh proves each row equals a fresh build of"
  echo "// CHAIN_REV with the pinned toolchain."
  echo "//"
  echo "// Source: $SOURCE_NOTE" | fold -s -w 76 | sed -e '2,$s|^|// |' -e 's/ *$//' 
  echo "pub const CHAIN_REV: &str = \"$CHAIN_REV\";"
  echo "pub const FORGE_VERSION: &str = \"$FORGE_VERSION\";"
  echo "pub const SOLC_VERSION: &str = \"$SOLC_VERSION\";"
  echo "/// Templates whose row is a build of CHAIN_REV, not (yet) a registry read."
  if [ "${#BUILT[@]}" -eq 0 ]; then
    echo "pub const BUILT_FROM_CHAIN_REV: &[&str] = &[];"
  else
    echo "pub const BUILT_FROM_CHAIN_REV: &[&str] = &["
    for b in "${BUILT[@]}"; do echo "    \"$b\","; done
    echo "];"
  fi
  echo "pub const PINNED_INIT_CODE_HASHES: &[(&str, &str)] = &["
  for row in "${HASH_ROWS[@]}"; do
    echo "    (\"${row%%|*}\", \"${row#*|}\"),"
  done
  echo "];"
} > "$ROOT/src-tauri/src/template_hashes.rs"

echo ">>> vendored ${#TEMPLATES[@]} artifacts to src-tauri/artifacts/templates/"
echo ">>> wrote src-tauri/src/template_hashes.rs"
if [ "$FROM_BUILD" -eq 1 ]; then
  echo ">>> from build: verify with scripts/verify-template-hashes.sh; NOT yet registered on chain"
else
  echo ">>> every one verified against the live registry"
fi
