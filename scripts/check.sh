#!/usr/bin/env bash
# created: 2026-07-23 | branch: main
# author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
# status: active
#
# citrate-quorum — THE check gate (WP-S1.7).
#
# GitHub Actions is failing org-wide on CitrateNetwork (billing), so per the
# owner's decision this script — not `.github/workflows/` — is the source of
# truth for "does this change pass". The workflow file mirrors it and takes over
# once Actions is restored.
#
# Honesty rule (CLAUDE.md rule 1): a check that cannot run reports SKIP with a
# reason. It never reports PASS. A green run on a repo with no code must be
# visibly green-because-empty, not green-because-verified.
#
# Usage:  scripts/check.sh            # run everything applicable
#         scripts/check.sh --list     # show what would run, run nothing
set -uo pipefail

cd "$(dirname "$0")/.." || exit 2
ROOT="$PWD"
LIST_ONLY="${1:-}"

PASS=0; FAIL=0; SKIP=0
FAILED_NAMES=()

c_red=$'\033[31m'; c_grn=$'\033[32m'; c_yel=$'\033[33m'; c_dim=$'\033[2m'; c_off=$'\033[0m'

# run <name> <reason-if-absent> <guard-cmd> <cmd...>
# guard-cmd decides applicability: exit 0 = run it, non-zero = skip.
run() {
  local name="$1" skip_reason="$2" guard="$3"; shift 3
  if ! eval "$guard" >/dev/null 2>&1; then
    printf '%s  SKIP%s  %-28s %s%s%s\n' "$c_yel" "$c_off" "$name" "$c_dim" "$skip_reason" "$c_off"
    SKIP=$((SKIP+1)); return 0
  fi
  if [ "$LIST_ONLY" = "--list" ]; then
    printf '%s  WOULD%s %-28s %s%s%s\n' "$c_dim" "$c_off" "$name" "$c_dim" "$*" "$c_off"
    return 0
  fi
  if "$@" >/tmp/quorum-check.$$.log 2>&1; then
    printf '%s  PASS%s  %-28s\n' "$c_grn" "$c_off" "$name"
    PASS=$((PASS+1))
  else
    printf '%s  FAIL%s  %-28s\n' "$c_red" "$c_off" "$name"
    sed 's/^/        /' /tmp/quorum-check.$$.log | tail -25
    FAIL=$((FAIL+1)); FAILED_NAMES+=("$name")
  fi
  rm -f /tmp/quorum-check.$$.log
}

echo "citrate-quorum check gate — $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
echo

# ---- docs -----------------------------------------------------------------
echo "docs"
run "frontmatter" "no markdown files" \
    "find . -name '*.md' -not -path './node_modules/*' -not -path './.git/*' | grep -q ." \
    bash -c '
      fail=0
      while IFS= read -r f; do
        [ "$(head -c 4 "$f")" = "---" ] || { echo "missing YAML frontmatter: $f"; fail=1; }
      done < <(find . -name "*.md" -not -path "./node_modules/*" -not -path "./.git/*")
      exit $fail'

# ---- rust -----------------------------------------------------------------
echo
echo "rust"
HAS_CARGO='[ -f Cargo.toml ] && command -v cargo'
run "cargo fmt"    "no Cargo.toml yet (WP-S1.3)" "$HAS_CARGO" cargo fmt --all -- --check
run "cargo clippy" "no Cargo.toml yet (WP-S1.3)" "$HAS_CARGO" cargo clippy --workspace --all-targets --locked -- -D warnings
run "cargo test"   "no Cargo.toml yet (WP-S1.3)" "$HAS_CARGO" cargo test --workspace --locked
run "cargo audit"  "no Cargo.toml yet, or cargo-audit not installed" \
    "$HAS_CARGO && command -v cargo-audit" cargo audit

# ---- node -----------------------------------------------------------------
echo
echo "node"
HAS_NODE='[ -f package.json ] && command -v npm'
run "typecheck" "no package.json yet (design prototype, QRM-S2D)" "$HAS_NODE" npm run --silent typecheck
run "vitest"    "no package.json yet (design prototype, QRM-S2D)" "$HAS_NODE" npm test --silent
run "build"     "no package.json yet (design prototype, QRM-S2D)" "$HAS_NODE" npm run --silent build

# ---- generated-code drift -------------------------------------------------
# WP-S1.5: the committed RBAC bindings must match a fresh generation from the
# chain ABIs. Skips (not fails) if the chain repo isn't checked out alongside.
echo
echo "generated code"
GEN="$ROOT/scripts/gen_rbac_bindings.py"
if command -v uv >/dev/null 2>&1; then PYRUN=(uv run python3); else PYRUN=(python3); fi
run "rbac bindings drift" "chain repo not alongside, or no python runner" \
    "[ -f '$GEN' ] && { [ -d '$ROOT/../citrate-chain/contracts/out' ] || [ -n \"\${CITRATE_CHAIN_DIR:-}\" ]; }" \
    "${PYRUN[@]}" "$GEN" --check

# ---- agentile ratchets ----------------------------------------------------
echo
echo "agentile ratchets"
AG="$ROOT/../agentile/scripts/ci"
# Pick the python runner this environment expects. Some Citrate workstations
# route python through `uv` and reject a bare `python3`; prefer uv when present
# so the ratchets run identically everywhere.
if command -v uv >/dev/null 2>&1; then PYRUN=(uv run python3); else PYRUN=(python3); fi
HAS_AG="[ -d '$AG' ] && command -v ${PYRUN[0]}"
for r in check_no_mocks check_no_unwraps check_test_ratchet check_spec_ratchet check_tripwire_ratchet; do
  run "$r" "agentile scripts not found at ../agentile, or no python runner" \
      "$HAS_AG && [ -f '$AG/$r.py' ]" "${PYRUN[@]}" "$AG/$r.py"
done

# ---- quorum-specific source scans ----------------------------------------
# These enforce the wiring contract from the design brief (10_DESIGN_BRIEF.md 6).
echo
echo "wiring contract"
HAS_SRC="[ -d src/surfaces ]"
run "no invoke in surfaces" "no src/surfaces yet (design prototype)" "$HAS_SRC" \
    bash -c '! grep -rn "from \"@tauri-apps/api" src/surfaces src/components 2>/dev/null'
run "no sim data outside bridge" "no src/surfaces yet (design prototype)" "$HAS_SRC" \
    bash -c '! grep -rln "FIXTURE\|MOCK_\|fakeData" src/surfaces src/components 2>/dev/null'
run "no hardcoded hex" "no src/surfaces yet (design prototype)" "$HAS_SRC" \
    bash -c '! grep -rnE "#[0-9a-fA-F]{6}\b" src/surfaces src/components src/shell 2>/dev/null'

# ---- summary --------------------------------------------------------------
echo
echo "───────────────────────────────────────────────"
if [ "$LIST_ONLY" = "--list" ]; then
  echo "list only — nothing was run"
  exit 0
fi
printf 'pass %d   fail %d   skip %d\n' "$PASS" "$FAIL" "$SKIP"
if [ "$FAIL" -gt 0 ]; then
  printf '%sFAILED:%s %s\n' "$c_red" "$c_off" "${FAILED_NAMES[*]}"
  exit 1
fi
if [ "$PASS" -eq 0 ]; then
  printf '%sNothing was verified — every check skipped.%s\n' "$c_yel" "$c_off"
  printf 'This is expected while the repo is a scaffold. It is NOT a passing build.\n'
  exit 0
fi
printf '%sok%s\n' "$c_grn" "$c_off"
exit 0
