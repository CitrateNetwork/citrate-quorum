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
# The python runner some checks need. Defined here because the rust section uses
# it too (audit-ignore honesty); redefined identically further down where the
# ratchets run, so neither section depends on the other's ordering.
if command -v uv >/dev/null 2>&1; then PYRUN=(uv run python3); else PYRUN=(python3); fi
echo
echo "rust"
HAS_CARGO='[ -f Cargo.toml ] && command -v cargo'
# `cargo fmt --all` walks PATH DEPENDENCIES too, not just this workspace's members.
# Since QRM-S3 that includes citrate-comms, whose crates have their own rustfmt
# settings — so `--all` demanded we reformat another repo to keep this gate green.
# `cargo metadata --no-deps` lists exactly this workspace's own packages, which is
# what "our formatting" means.
# `cargo fmt --all` walks PATH DEPENDENCIES too, not just this workspace's members.
# Since QRM-S3 that includes citrate-comms, whose crates have their own rustfmt
# settings — so `--all` demanded reformatting another repo to keep this gate green.
# `cargo metadata --no-deps` lists exactly this workspace's own packages, which is
# what "our formatting" means.
if command -v uv >/dev/null 2>&1; then FMTPY=(uv run python3); else FMTPY=(python3); fi
run "cargo fmt"    "no Cargo.toml yet (WP-S1.3)" "$HAS_CARGO" \
    bash -c 'cargo metadata --no-deps --format-version 1 \
      | '"${FMTPY[*]}"' -c "import json,sys; print(chr(10).join(p[chr(34)+chr(109)+chr(97)+chr(110)+chr(105)+chr(102)+chr(101)+chr(115)+chr(116)+chr(95)+chr(112)+chr(97)+chr(116)+chr(104)+chr(34)] for p in json.load(sys.stdin)[chr(34)+chr(112)+chr(97)+chr(99)+chr(107)+chr(97)+chr(103)+chr(101)+chr(115)+chr(34)]))" \
      | while IFS= read -r m; do cargo fmt --check --manifest-path "$m" || exit 1; done'

run "cargo clippy" "no Cargo.toml yet (WP-S1.3)" "$HAS_CARGO" cargo clippy --workspace --all-targets --locked -- -D warnings
run "cargo test"   "no Cargo.toml yet (WP-S1.3)" "$HAS_CARGO" cargo test --workspace --locked
run "cargo audit"  "no Cargo.toml yet, or cargo-audit not installed" \
    "$HAS_CARGO && command -v cargo-audit" cargo audit
# `cargo audit` exits 0 once an advisory is in `.cargo/audit.toml`, so the ignore
# file is exactly where a security finding goes to be forgotten. This fails when
# the acceptance expires, when an ignore goes stale, or when the upstream blocker
# it rests on lifts — see the file's own header.
run "audit ignores are honest" "no cargo-audit, or no python runner" \
    "command -v cargo-audit && command -v ${PYRUN[0]:-python3}" \
    "${PYRUN[@]:-python3}" "$ROOT/scripts/check_audit_ignores.py"

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

# QR-B-005: this T1 repo's README and CLAUDE.md name `04_HIC_MODEL.md` as the
# normative, federation-wide control model and `09_DECISIONS_LOCKED.md` as
# authoritative — both living in citrate-federation's planset, NOT copied here
# ("Link, don't copy", CLAUDE.md rule 10). If the linked planset is missing, the
# standard the code claims to enforce is unreachable from the federation's current
# state, and the graded control model silently becomes whatever the code happens
# to do. Skips honestly when the federation repo is not checked out alongside (the
# same discipline as the rbac-drift row); FAILS when it IS present but the
# canonical documents this repo points at do not resolve.
FED_PLANSET="$ROOT/../citrate-federation/.agentile/planset/2026-07-22-citrate-quorum"
run "linked planset resolves" "citrate-federation not checked out alongside" \
    "[ -d '$ROOT/../citrate-federation' ]" \
    bash -c '
      fed="'"$FED_PLANSET"'"
      miss=0
      for d in 04_HIC_MODEL.md 09_DECISIONS_LOCKED.md 10_DESIGN_BRIEF.md; do
        [ -f "$fed/$d" ] || { echo "missing canonical doc this repo links: $fed/$d"; miss=1; }
      done
      exit $miss'

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
# The scan above looks for fixtures by NAME, so it never saw the way fabricated
# data actually shipped: plain literals inline in a surface. The packaged app
# rendered "1,204 actions recorded" and "62% budget burn" against a brand-new
# empty tenant, and this gate was green the whole time.
#
# This catches the stat-shaped literals — thousands separators and percentages —
# which is the form a fabricated metric almost always takes. It CANNOT catch
# every invented value (a bare "4" is indistinguishable from a real one), so it
# is a tripwire for the common case, not a proof of honesty. The proof is
# running the packaged app against an empty tenant and reading what it claims.
#
# WIDENED, QRM-S7.8. The previous patterns anchored a quote on BOTH sides of the
# number, so they matched `"14208"` and missed everything a fabricated stat
# actually looks like. Both of these sat in Governance.tsx while this check
# reported PASS:
#
#     ["Block", "1,284,067 · anchored"]      <- text after the digits
#     <div>0.7% of all decisions</div>       <- JSX text, never quoted
#
# So the number is now matched wherever it appears, quoted or not. A control
# that only catches the form nobody writes is not a control.
run "no fabricated stats in surfaces" "no src/surfaces yet (design prototype)" "$HAS_SRC" \
    bash -c '
      # Thousands-separated numbers have no CSS analogue, so they are scanned
      # everywhere. Percentages are scanned only outside style/geometry context,
      # where "100%" is a layout value rather than a claim about the world.
      seps=$(grep -rnE "[^0-9a-zA-Z_.][0-9]{1,3}(,[0-9]{3})+" src/surfaces src/components 2>/dev/null || true)
      pcts=$(grep -rnE "[0-9]+(\.[0-9]+)?%" src/surfaces src/components 2>/dev/null \
             | grep -vE "style=|width:|height:|inset:|translate|gradient|%\)" || true)
      # Drop whole-line comments and JSX comment bodies: a comment QUOTING the
      # fabricated value it removed is documentation, not a claim on screen.
      hits=$(printf "%s\n%s" "$seps" "$pcts" \
             | grep -v "^$" \
             | grep -vE "^\S*:[0-9]*: *(//|\*|/\*)" || true)
      if [ -n "$hits" ]; then
        echo "stat-shaped literals in a surface — is this read from the bridge?"
        echo "$hits"
        exit 1
      fi
      exit 0'
# React hooks after a top-level conditional return crash at runtime only when
# that return is taken — a path tsc and happy-path tests never walk. Two of the
# S2D.4 honesty guards shipped with exactly that bug.
#
# This replaced `scripts/check_hooks_after_return.py`, a hand-rolled tripwire
# whose own docstring said to delete it the day eslint landed. eslint's
# `react-hooks/rules-of-hooks` reasons about the real control-flow graph rather
# than indentation, so it also catches the cases the tripwire structurally
# could not: arrow-function components, hooks inside loops, and hooks after a
# `&&` short-circuit. Both directions were negative-controlled before the swap.
run "eslint (rules-of-hooks + deps)" "no node_modules yet" \
    "[ -d node_modules ]" \
    npm run --silent lint
# Surfaces + components MUST use semantic tokens (var(--...)), never literal hex.
# src/shell is excluded: the sidebar is fixed evergreen brand chrome that
# hardcodes the same on-dark palette as citrate-core's Sidebar.tsx (no semantic
# token exists for sidebar-on-evergreen text).
run "no hardcoded hex" "no src/surfaces yet (design prototype)" "$HAS_SRC" \
    bash -c '! grep -rnE "#[0-9a-fA-F]{6}\b" src/surfaces src/components 2>/dev/null'

# ---- the packaged binary ---------------------------------------------------
# Nine real bugs in QRM-S4 were found by launching the .deb and clicking, with
# every check above green. This runs that by hand-free — but it needs a built
# binary and an X server and takes minutes, so it is OPT-IN rather than part of
# the default gate. It reports SKIP with the reason when it is not enabled,
# never PASS (the honesty rule at the top of this file).
echo
echo "packaged binary"
run "packaged smoke run" "set QUORUM_SMOKE=1 (needs a release build + Xvfb; takes ~2min)" \
    "[ -n \"\${QUORUM_SMOKE:-}\" ] && [ -x target/release/citrate-quorum ] && command -v Xvfb && command -v ${PYRUN[0]}" \
    "${PYRUN[@]}" "$ROOT/scripts/smoke_packaged.py"

# ---- @rule8: no signing / updater key material -----------------------------
# WP-S1.8: the installer skeleton is UNSIGNED and has NO auto-updater. Signing
# identities + the update signing key are @rule8 secrets deferred to QRM-S9 and
# must never live in the repo/CI. This guard fails if any of that leaks in.
echo
echo "@rule8 signing hygiene"
run "no signing/updater key material" "always runs" "true" \
    bash -c '
      hits=0
      # An active updater pubkey in the tauri config (empty/false is fine).
      if grep -RInE "\"pubkey\"[[:space:]]*:[[:space:]]*\"[A-Za-z0-9+/=]+\"" src-tauri/tauri.conf.json 2>/dev/null; then
        echo "updater pubkey present in tauri.conf.json (must stay inert until S9)"; hits=1; fi
      # createUpdaterArtifacts must not be true in the skeleton.
      if grep -RInE "\"createUpdaterArtifacts\"[[:space:]]*:[[:space:]]*true" src-tauri/tauri.conf.json 2>/dev/null; then
        echo "createUpdaterArtifacts:true (updater must be inert until S9)"; hits=1; fi
      # Any private signing key material committed anywhere. scripts/ and docs/ are
      # excluded: this guard and RELEASE.md deliberately NAME these strings as prose
      # and patterns — only genuine key material elsewhere should trip the check.
      if grep -RIlE "untrusted comment: (minisign|rsign) encrypted secret key|TAURI_SIGNING_PRIVATE_KEY[[:space:]]*[:=]|BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY" \
           --exclude-dir=node_modules --exclude-dir=target --exclude-dir=.git \
           --exclude-dir=scripts --exclude-dir=docs . 2>/dev/null; then
        echo "private signing key material present in the tree"; hits=1; fi
      # Key files by extension.
      if find . -path ./node_modules -prune -o -path ./target -prune -o \
           \( -name "*.key" -o -name "*.pem" -o -name "*.p12" -o -name "*.minisign" \) -print 2>/dev/null | grep -q .; then
        echo "a key/cert file is present in the tree"; hits=1; fi
      exit $hits'

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
