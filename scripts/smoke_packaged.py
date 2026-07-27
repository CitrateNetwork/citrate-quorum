#!/usr/bin/env python3
"""citrate-quorum — the packaged-binary smoke run.

Builds nothing. Takes the release binary, runs it headless from a CLEAN
install, drives it, and asserts the things that only break in the packaged app.

## Why this exists

Every slice of QRM-S4 shipped with a green gate — 178 Rust tests, a
type-checked frontend, twenty local checks — and nine real bugs were still
found by launching the `.deb` and clicking. None were catchable by `cargo test`
or `tsc`, and several could not exist on the sim adapter at all:

  · a synchronous throw from a Promise-typed method white-screened eleven of
    twelve surfaces (#12);
  · the Dashboard rendered "1,204 actions recorded" against an empty tenant and
    then never refreshed its counts (#13);
  · a `useMemo` below an early return crashed two surfaces on their failure
    path (#13);
  · a grant revoked in an earlier session came back looking live (#21).

This turns that hand-driving into one command.

## What it can and cannot see

There is no DevTools protocol for the packaged WebView and no OCR on this box,
so it cannot read rendered text. It works with what it CAN observe:

  · the filesystem — the evidence store, the token file's mode, the chain;
  · the agent bridge over its loopback socket — real verdicts, real refusals;
  · screenshots — pixel variance to catch a blank render, and region
    comparison to catch a pane that never updates.

Reading a *value* off the screen is out of reach; "no fabricated stats in
surfaces" stays a source scan in `check.sh`. What this adds is the behaviour
those scans cannot reach.

## The rule every UI step follows

A click is only believed when something outside the UI proves it happened —
`scope.json` appears, the chain grows, a crop changes. Coordinates drift when
layout changes; without this rule a drifted click produces a confusing failure
three steps later instead of "onboarding did not complete".

Usage:
    uv run python3 scripts/smoke_packaged.py [--binary PATH] [--display :99] [--keep]

Exit 0 = every check passed. Exit 1 = at least one failed. Exit 2 = could not run.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

APP_ID = "ai.citrate.quorum"
GREEN, RED, YELLOW, DIM, OFF = "\033[32m", "\033[31m", "\033[33m", "\033[2m", "\033[0m"

# Sidebar and onboarding hit points, in real screen pixels at 1600x1000.
# FRAGILE BY NATURE: they are coordinates into a rendered layout. That is
# tolerable only because every step below is confirmed by an out-of-band
# effect, so a drift fails loudly at the step that drifted.
ONBOARD = {
    "sso": (599, 555),
    "tenant": (498, 476),
    "operator": (498, 520),
    "set_scope": (786, 574),
    "tour_next": [(814, 497), (814, 497), (814, 473)],
    "enter": (785, 486),
}
SURFACES = [
    ("Dashboard", 98), ("Governance", 126), ("Agents", 157),
    ("Rooms", 219), ("Meetings", 249), ("Calendar", 277),
    ("Ledger", 340), ("Journal", 370), ("Repos", 400),
    ("Wallet", 460), ("Node", 491), ("Settings", 521),
]
SIDEBAR_X = 62


def app_data_dir() -> Path:
    """Where the packaged app keeps its data.

    Honours `XDG_DATA_HOME` because Tauri's `app_data_dir()` does. A caller that
    runs the app under a private home (see `verify_meetings.py`'s isolated
    keyring) would otherwise assert against a stale directory in the real one
    and read a previous run's evidence as if it were this run's.
    """
    base = os.environ.get("XDG_DATA_HOME") or (Path.home() / ".local/share")
    return Path(base) / APP_ID
# The window is 1200x780 at +0+0 inside a 1600x1000 root; inside it the shell
# is sidebar (~222px) | topbar 52px / surface / status rail 30px.
#
# The surface region ONLY. The sidebar and topbar are fixed chrome that render
# perfectly well while the surface itself is blank — an earlier version of this
# crop included them and scored a surface that rendered `null` at 4552 instead
# of 0, i.e. it missed the exact bug it exists to catch. Measured, not guessed:
# with these bounds a blank surface reads 0 and a live one 4500-7200.
CONTENT_CROP = "970x690+226+56"
# The Dashboard's stat-tile row alone, so the ledger ribbon's own updates
# cannot stand in for the counts refreshing.
TILES_CROP = "950x115+230+62"
# Comfortably between "flat colour" (0) and the live floor (~4500).
BLANK_STDDEV = 800.0


class Smoke:
    def __init__(self, binary: Path, display: str, keep: bool) -> None:
        self.binary = binary
        self.display = display
        self.keep = keep
        self.app_data = app_data_dir()
        self.work = Path("/tmp") / f"quorum-smoke-{os.getpid()}"
        self.xvfb: subprocess.Popen | None = None
        self.app: subprocess.Popen | None = None
        self.results: list[tuple[str, bool, str]] = []

    # ---- reporting ---------------------------------------------------

    def check(self, name: str, ok: bool, detail: str = "") -> bool:
        self.results.append((name, ok, detail))
        mark = f"{GREEN}  PASS{OFF}" if ok else f"{RED}  FAIL{OFF}"
        print(f"{mark}  {name:<44}{DIM}{detail}{OFF}" if detail else f"{mark}  {name}")
        return ok

    def fatal(self, msg: str) -> None:
        print(f"{RED}  ABORT{OFF}  {msg}")
        self.teardown()
        sys.exit(2)

    # ---- process control ---------------------------------------------

    def _run(self, *args: str, env: dict | None = None, timeout: int = 30) -> subprocess.CompletedProcess:
        e = {**os.environ, "DISPLAY": self.display}
        if env:
            e.update(env)
        return subprocess.run(args, capture_output=True, text=True, env=e, timeout=timeout)

    def setup(self) -> None:
        self.work.mkdir(parents=True, exist_ok=True)
        # A CLEAN install every run. Half the bugs this hunts only appear on
        # first launch, and a leftover store would hide them.
        if self.app_data.exists():
            shutil.rmtree(self.app_data)
        self.xvfb = subprocess.Popen(
            ["Xvfb", self.display, "-screen", "0", "1600x1000x24"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True,
        )
        time.sleep(3)

    def start_app(self) -> None:
        log = (self.work / "app.log").open("a")
        self.app = subprocess.Popen(
            [str(self.binary)],
            env={
                **os.environ,
                "DISPLAY": self.display,
                # Software rendering: there is no GPU behind Xvfb.
                "WEBKIT_DISABLE_COMPOSITING_MODE": "1",
                "WEBKIT_DISABLE_DMABUF_RENDERER": "1",
            },
            stdout=log, stderr=log, start_new_session=True,
        )
        time.sleep(11)

    def stop_app(self) -> None:
        if self.app and self.app.poll() is None:
            os.killpg(os.getpgid(self.app.pid), signal.SIGTERM)
            try:
                self.app.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(os.getpgid(self.app.pid), signal.SIGKILL)
        self.app = None

    def teardown(self) -> None:
        self.stop_app()
        if self.xvfb and self.xvfb.poll() is None:
            os.killpg(os.getpgid(self.xvfb.pid), signal.SIGTERM)
        if not self.keep:
            shutil.rmtree(self.work, ignore_errors=True)
            shutil.rmtree(self.app_data, ignore_errors=True)

    # ---- input + screen ----------------------------------------------

    def _xdo(self, script: str) -> None:
        """Synthetic X input via XTEST. python-xlib is fetched by uv on demand,
        so this needs no system package."""
        subprocess.run(
            ["uv", "run", "--with", "python-xlib", "python3", "-c", script],
            capture_output=True, text=True, env={**os.environ, "DISPLAY": self.display},
            timeout=60, check=False,
        )

    def click(self, x: int, y: int, settle: float = 1.0) -> None:
        self._xdo(
            "from Xlib import display,X\nfrom Xlib.ext import xtest\nimport time\n"
            f"d=display.Display('{self.display}')\n"
            f"d.screen().root.warp_pointer({x},{y})\nd.sync()\ntime.sleep(.15)\n"
            "xtest.fake_input(d,X.ButtonPress,1)\nd.sync()\ntime.sleep(.05)\n"
            "xtest.fake_input(d,X.ButtonRelease,1)\nd.sync()\n"
        )
        time.sleep(settle)

    # X11 keysym names for the punctuation a real form actually contains. The
    # first version mapped only `- . _ space`, so `XK.string_to_keysym(':')`
    # returned NoSymbol and the character was silently dropped — an RFC3339
    # timestamp typed as "2026-07-30T09" and a filesystem path as "". The
    # driver reported a green click over a field that never received the text.
    KEYSYM_NAMES = {
        " ": "space", "-": "minus", ".": "period", "_": "underscore",
        ":": "colon", "/": "slash", "@": "at", "+": "plus", "=": "equal",
        ",": "comma", ";": "semicolon", "#": "numbersign", "%": "percent",
        "!": "exclam", "?": "question", "*": "asterisk", "&": "ampersand",
        "$": "dollar", "^": "asciicircum", "~": "asciitilde", "`": "grave",
        "'": "apostrophe", '"': "quotedbl", "|": "bar", "\\": "backslash",
        "(": "parenleft", ")": "parenright", "[": "bracketleft",
        "]": "bracketright", "{": "braceleft", "}": "braceright",
        "<": "less", ">": "greater",
    }

    def type_text(self, text: str) -> None:
        """Type `text` via XTEST, including punctuation.

        Shift is decided by asking the SERVER whether the keycode produces this
        keysym unshifted, not by `ch.isupper()`. `:` and `?` need shift and are
        not uppercase letters, so the old rule typed `;` and `/` instead — a
        wrong character is worse than a dropped one, because the form still
        looks filled.
        """
        self._xdo(
            "from Xlib import display,X,XK\nfrom Xlib.ext import xtest\nimport time\n"
            f"d=display.Display('{self.display}')\n"
            f"names={self.KEYSYM_NAMES!r}\n"
            f"missing=[]\n"
            f"for ch in {text!r}:\n"
            "    ks=XK.string_to_keysym(names.get(ch,ch))\n"
            "    code=d.keysym_to_keycode(ks)\n"
            "    if not code:\n        missing.append(ch)\n        continue\n"
            # index 0 is the unshifted keysym for this keycode; if it differs,
            # the character we want lives on the shifted level.
            "    sh = d.keycode_to_keysym(code,0)!=ks\n"
            "    if sh: xtest.fake_input(d,X.KeyPress,d.keysym_to_keycode(XK.string_to_keysym('Shift_L')))\n"
            "    xtest.fake_input(d,X.KeyPress,code)\n    d.sync()\n"
            "    xtest.fake_input(d,X.KeyRelease,code)\n    d.sync()\n"
            "    if sh: xtest.fake_input(d,X.KeyRelease,d.keysym_to_keycode(XK.string_to_keysym('Shift_L')))\n"
            "    d.sync()\n    time.sleep(.04)\n"
            # A character the server cannot type must be loud. A silent drop is
            # how a green run gets recorded over a field that stayed empty.
            "if missing: raise SystemExit('untypable characters: '+repr(missing))\n"
        )
        time.sleep(0.5)

    def shot(self, name: str, crop: str | None = None) -> Path:
        raw = self.work / f"{name}.png"
        self._run("import", "-window", "root", str(raw))
        if crop:
            self._run("convert", str(raw), "-crop", crop, "+repage", str(raw))
        return raw

    def stddev(self, img: Path) -> float:
        out = self._run("identify", "-format", "%[standard-deviation]", str(img))
        try:
            return float(out.stdout.strip().split()[0])
        except (ValueError, IndexError):
            return -1.0

    def signature(self, img: Path) -> str:
        """A hash of the pixels. Two identical renders share one; a pane that
        never updated is byte-identical."""
        return self._run("identify", "-format", "%#", str(img)).stdout.strip()

    # ---- the bridge ---------------------------------------------------

    def bridge(self) -> tuple[str, str] | None:
        ep = self.app_data / "agent/endpoint.json"
        tok = self.app_data / "agent/token"
        if not ep.exists() or not tok.exists():
            return None
        try:
            return json.loads(ep.read_text())["addr"], tok.read_text().strip()
        except (json.JSONDecodeError, KeyError):
            return None

    def post_intent(self, addr: str, token: str | None, body: dict) -> tuple[int, str]:
        req = urllib.request.Request(
            f"http://{addr}/intent", data=json.dumps(body).encode(),
            headers={"Content-Type": "application/json",
                     **({"Authorization": f"Bearer {token}"} if token else {})},
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=10) as r:
                return r.status, r.read().decode()
        except urllib.error.HTTPError as e:
            return e.code, e.read().decode()
        except OSError as e:
            return 0, str(e)

    def chain_lines(self) -> int:
        tenants = self.app_data / "evidence/tenants"
        if not tenants.is_dir():
            return 0
        return sum(
            len([ln for ln in (d / "chain.jsonl").read_text().splitlines() if ln.strip()])
            for d in tenants.iterdir() if (d / "chain.jsonl").exists()
        )

    # ---- the run -------------------------------------------------------

    def run(self) -> int:
        if not self.binary.exists():
            self.fatal(f"no binary at {self.binary} — run `npm run tauri build -- --bundles deb`")
        for tool in ("Xvfb", "import", "identify", "uv"):
            if not shutil.which(tool):
                self.fatal(f"`{tool}` is required and not on PATH")

        print(f"citrate-quorum packaged smoke — {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}")
        print(f"{DIM}binary {self.binary}  ·  clean install at {self.app_data}{OFF}")
        # HONEST LIMIT: "clean install" means the app-data dir only. The custody
        # vault also keeps `custody-master-key` and `custody-generation` in the
        # OS keyring, and those SURVIVE deleting app-data — a second run then
        # meets a fresh envelope sealed against a stale master key and reports
        # "custody envelope corrupt or tampered". Nothing below touches custody,
        # so it does not affect these checks, but a custody test must clear the
        # keyring too. This script does NOT do that: silently deleting a
        # developer's keyring entries is not a smoke test's business.
        print(f"{DIM}  (app-data only — the OS keyring is not reset){OFF}\n")

        self.setup()
        self.start_app()

        print("startup")
        alive = self.app is not None and self.app.poll() is None
        if not self.check("app starts", alive, "" if alive else "exited immediately; see app.log"):
            self.teardown()
            return 1
        # The store is REQUIRED at startup: an install that cannot write
        # evidence must fail loudly rather than run looking healthy.
        self.check("evidence store opens", (self.app_data / "evidence").is_dir(),
                   str(self.app_data / "evidence"))

        b = self.bridge()
        self.check("agent bridge is listening", b is not None,
                   b[0] if b else "no agent/endpoint.json")
        if b:
            mode = oct((self.app_data / "agent/token").stat().st_mode & 0o777)
            self.check("agent token is 0600", mode == "0o600", mode)

        print("\nthe gate refuses before it is asked")
        if b:
            addr, token = b
            code, _ = self.post_intent(addr, None, {"agent": "x", "tool": "y", "classification": "Public"})
            self.check("an unauthenticated intent is refused", code == 401, f"HTTP {code}")
            code, _ = self.post_intent(addr, "0" * 64, {"agent": "x", "tool": "y", "classification": "Public"})
            self.check("a wrong bearer is refused", code == 401, f"HTTP {code}")
            # No tenant scope has been established yet.
            code, body = self.post_intent(addr, token, {"agent": "a", "tool": "shell.exec", "classification": "Public"})
            self.check("with no tenant scope, agents cannot act",
                       code == 409 and "tenant scope" in body, f"HTTP {code}")

        print("\nonboarding")
        self.click(*ONBOARD["sso"], settle=3)
        self.click(*ONBOARD["tenant"])
        self.type_text("smoke-tenant")
        self.click(*ONBOARD["operator"])
        self.type_text("Smoke Operator")
        self.click(*ONBOARD["set_scope"], settle=3)
        scope_file = self.app_data / "evidence/scope.json"
        scope = json.loads(scope_file.read_text()) if scope_file.exists() else {}
        ok = scope.get("active_tenant") == "smoke-tenant" and scope.get("operator") == "Smoke Operator"
        if not self.check("scope + operator are established", ok, json.dumps(scope)):
            # Everything downstream needs a scope; a drifted coordinate should
            # say so here rather than cascade.
            print(f"{YELLOW}  the UI steps below cannot run without a scope{OFF}")
            self.teardown()
            return 1
        for pt in ONBOARD["tour_next"]:
            self.click(*pt)
        self.click(*ONBOARD["enter"], settle=4)

        print("\nevery surface renders")
        blank: list[str] = []
        for name, y in SURFACES:
            self.click(SIDEBAR_X, y, settle=2)
            img = self.shot(f"surface_{name}", CONTENT_CROP)
            sd = self.stddev(img)
            if sd < BLANK_STDDEV:
                blank.append(f"{name}({sd:.0f})")
        self.check(f"all {len(SURFACES)} surfaces render", not blank,
                   "blank: " + ", ".join(blank) if blank else "no blank renders")

        print("\nthe agent path")
        before = self.chain_lines()
        verdict = {}
        if b:
            addr, token = b
            code, body = self.post_intent(addr, token, {
                "agent": "smoke-agent", "tool": "shell.exec",
                "classification": "Public", "correlation_id": "SMOKE-1",
            })
            verdict = json.loads(body) if code == 200 else {}
            self.check("an ungranted agent action is refused", verdict.get("may_proceed") is False,
                       verdict.get("verdict", f"HTTP {code}"))
            self.check("it is recorded as ungoverned, not dropped",
                       verdict.get("ungoverned") is True and self.chain_lines() == before + 1,
                       f"chain {before} -> {self.chain_lines()}")
            self.check("the refusal carries no key material",
                       not any(k in body.lower() for k in ("signature", "private_key", "seed", "mnemonic")))

        print("\nthe dashboard is live, not a snapshot")
        self.click(SIDEBAR_X, 98, settle=3)
        tiles_before = self.shot("tiles_before", TILES_CROP)
        sig_before = self.signature(tiles_before)
        if b:
            addr, token = b
            for i in range(3):
                self.post_intent(addr, token, {
                    "agent": "smoke-agent", "tool": "repo.write",
                    "classification": "Public", "correlation_id": f"SMOKE-{i + 2}",
                })
        # Longer than the 4s refresh beat, so a live pane has certainly moved.
        time.sleep(9)
        sig_after = self.signature(self.shot("tiles_after", TILES_CROP))
        self.check("counts refresh after a decision is recorded", sig_before != sig_after,
                   "unchanged — the tiles are a snapshot taken at mount"
                   if sig_before == sig_after else "tiles moved")

        print("\nevidence outlives the process")
        recorded = self.chain_lines()
        self.stop_app()
        self.check("records survive the app exiting", self.chain_lines() == recorded,
                   f"{self.chain_lines()} on disk with the app stopped")
        self.start_app()
        self.check("the chain replays and the scope resumes",
                   self.chain_lines() == recorded and (self.app_data / "evidence/scope.json").exists(),
                   f"{self.chain_lines()} records after restart")
        if bb := self.bridge():
            addr, token = bb
            code, body = self.post_intent(addr, token, {
                "agent": "smoke-agent", "tool": "shell.exec",
                "classification": "Public", "correlation_id": "SMOKE-RESTART",
            })
            self.check("the gate still rules after a restart",
                       code == 200 and json.loads(body).get("decision_id") == recorded,
                       f"decision #{json.loads(body).get('decision_id') if code == 200 else '?'}")

        failed = [n for n, ok, _ in self.results if not ok]
        print("\n" + "─" * 47)
        print(f"pass {len(self.results) - len(failed)}   fail {len(failed)}")
        if failed:
            print(f"{RED}FAILED:{OFF} " + ", ".join(failed))
            print(f"{DIM}screenshots and app.log kept at {self.work}{OFF}")
            self.keep = True
        else:
            print(f"{GREEN}ok{OFF}")
        self.teardown()
        return 1 if failed else 0


def main() -> int:
    ap = argparse.ArgumentParser(description="Run the packaged binary and check what only breaks there.")
    ap.add_argument("--binary", default="target/release/citrate-quorum", type=Path)
    ap.add_argument("--display", default=":99")
    ap.add_argument("--keep", action="store_true", help="keep screenshots, logs and the app-data dir")
    a = ap.parse_args()
    smoke = Smoke(a.binary, a.display, a.keep)
    try:
        return smoke.run()
    except KeyboardInterrupt:
        smoke.teardown()
        return 2


if __name__ == "__main__":
    sys.exit(main())
