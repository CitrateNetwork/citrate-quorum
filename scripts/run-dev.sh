#!/usr/bin/env bash
# created: 2026-07-28 | branch: feat/qrm-s7-live-run
# author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
# status: active
#
# citrate-quorum — run the packaged app against an ISOLATED custody keyring.
#
# ## Why this exists
#
# `KEYRING_SERVICE` is a constant in citrate-core-kit — `"ai.citrate.core"` — and
# quorum consumes the kit unchanged. So quorum and citrate-core write
# `custody-master-key` and `custody-generation` to the SAME keyring service under
# the SAME account names. Two consequences, both bad:
#
#   1. You cannot reset one app's vault without destroying the other's.
#   2. `rm -rf ~/.local/share/ai.citrate.quorum` does NOT give a clean install.
#      The envelope goes; the keyring's generation high-water stays. That anchor
#      is MONOTONE by design (DR-1 anti-rollback), so the new gen-0 vault is
#      fail-closed until it climbs past the stale mark — the app comes back with
#      "custody envelope corrupt or tampered", which reads like a signing bug and
#      is not one. The kit says so in its own comment at `custody.rs` §init.
#
# So this does not delete anything from the developer's keyring. It gives quorum
# its own dbus session and its own gnome-keyring, both rooted in ONE directory,
# and points `XDG_DATA_HOME` there so the app data lands beside it.
#
# The upshot: a true clean install is `--fresh`, which removes that one directory
# and nothing else. The real `ai.citrate.core` vault is never read or written.
#
# `verify_meetings.py` does the same thing with a `mkdtemp`, because a test wants
# a new vault every run. This keeps the home so the operator identity survives a
# restart — you import a recovery phrase once, not once per launch.
#
# ## What this is not
#
# A dev launcher. The keyring passphrase is generated on first use and stored
# 0600 BESIDE the keyring it unlocks, which is isolation, not protection: anyone
# who can read the directory can unlock it. That is the correct trade for a
# dogfooding vault on the owner's own box, and the wrong one for anything else.
#
# Usage:
#   scripts/run-dev.sh            # launch (keeps existing identity + evidence)
#   scripts/run-dev.sh --fresh    # wipe the isolated home first, then launch
#   scripts/run-dev.sh --where    # print the paths, launch nothing
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2
ROOT="$PWD"

HOME_DIR="${QUORUM_DEV_HOME:-$HOME/.local/share/citrate-quorum-dev}"
DATA="$HOME_DIR/.local/share"
APPDATA="$DATA/ai.citrate.quorum"
PASSFILE="$HOME_DIR/keyring.pass"
BIN="$ROOT/target/release/citrate-quorum"

case "${1:-}" in
  --where)
    echo "isolated home : $HOME_DIR"
    echo "app data      : $APPDATA"
    echo "binary        : $BIN"
    echo "shared vault  : $HOME/.local/share/ai.citrate.core  (NEVER touched by this script)"
    exit 0
    ;;
  --fresh)
    # The app must not be holding the old data while we remove it.
    # `pkill -x` matches the process NAME exactly: `pkill -f citrate-quorum`
    # matches this script's own command line and kills the shell running it.
    pkill -x citrate-quorum 2>/dev/null && sleep 1
    rm -rf "$HOME_DIR"
    echo "wiped $HOME_DIR — this is a genuine clean install"
    ;;
esac

command -v dbus-run-session >/dev/null || { echo "dbus-run-session not found" >&2; exit 2; }
command -v gnome-keyring-daemon >/dev/null || { echo "gnome-keyring-daemon not found" >&2; exit 2; }
[ -x "$BIN" ] || { echo "no release binary at $BIN — run: npm run tauri build -- --bundles deb" >&2; exit 2; }

mkdir -p "$DATA"
if [ ! -f "$PASSFILE" ]; then
  ( umask 077; head -c 32 /dev/urandom | base64 | tr -d '\n' > "$PASSFILE" )
  echo "minted a keyring passphrase at $PASSFILE (0600)"
fi

echo "isolated home : $HOME_DIR"
echo "app data      : $APPDATA"
echo "display       : ${DISPLAY:-:1}"
echo "the developer's ai.citrate.core vault is NOT touched"

# THE line that makes this isolation rather than theatre. Both gnome-keyring
# (which stores under $XDG_DATA_HOME/keyrings) and Tauri's `app_data_dir()`
# (which resolves to $XDG_DATA_HOME/<identifier>) follow it, so one variable
# relocates the keyring AND the evidence store together. Without it the script
# prints reassuring paths and the app writes to the shared ones anyway.
export XDG_DATA_HOME="$DATA"
export DISPLAY="${DISPLAY:-:1}"
export WEBKIT_DISABLE_COMPOSITING_MODE=1
export WEBKIT_DISABLE_DMABUF_RENDERER=1

# WebKitGTK needs both of these under Xvfb/headless GL or the WebView renders
# black. Harmless on a real display.
exec dbus-run-session -- bash -c '
  # The daemon has to be up inside THIS session before the app asks it for
  # anything; a race here surfaces as "no keyring available" on first unlock.
  gnome-keyring-daemon --unlock --components=secrets < "$1" >/dev/null 2>&1
  exec "$2"
' _ "$PASSFILE" "$BIN"
