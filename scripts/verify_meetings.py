#!/usr/bin/env python3
"""citrate-quorum — the QRM-S5 meeting lifecycle on the PACKAGED binary.

    npm run tauri build -- --bundles deb
    uv run --with python-xlib python3 scripts/verify_meetings.py

Runs the sprint's exit criterion end to end: schedule a meeting whose agenda is
generated from this repo's real sprint files, attest a quorum, open (freezing
the agenda under a BLAKE3 hash), close (composing minutes), and ratify through
the SignatureCeremony.

Why this exists separately from `smoke_packaged.py`: the smoke proves the app
starts, the gate refuses, and no surface is blank. This proves a specific
multi-step domain flow, which needs form input and therefore coordinates.

**Every assertion reads `meetings.json` or `chain.jsonl`.** Nothing here trusts
the screen: a UI click is believed only when something outside the UI proves it.
The one place that rule was relaxed — "the ceremony reaches On record" — is
marked, and it is exactly the step that turned out to be misleading (see below).

Coordinates are MEASURED against the real 1200x780 window, never guessed. The
content crop origin is (226,56); form fields were measured off a screenshot of
the form itself. If the layout moves, these move.

Known ordering trap, found by this script: `ceremony.request()` resolves when
the operator DISMISSES the ceremony, not when the modal reaches "on record".
Clicking Sign alone leaves the record untouched — the backend write happens
after Close.
"""
import importlib.util, json, sys, time
from pathlib import Path
spec = importlib.util.spec_from_file_location("smoke", "scripts/smoke_packaged.py")
smoke = importlib.util.module_from_spec(spec); sys.modules["smoke"] = smoke
spec.loader.exec_module(smoke)

CX, CY = 226, 56
A = lambda x, y: (CX + x, CY + y)

# The agenda is generated from `.agentile/sprints/active/*/SCOPE.md`, so
# pointing this at the repo made the run depend on whether a sprint happened to
# be active. Closing QRM-S5 emptied `active/` and every assertion below failed —
# correctly, but for a reason that had nothing to do with the product.
#
# A fixture workspace makes it deterministic. This is a real directory with a
# real SCOPE.md parsed by the real generator; only the INPUT is fixed, so it
# stays a genuine end-to-end read. Seven work packages, because the detail-view
# coordinates below are measured against an agenda of that height.
FIXTURE = Path("/tmp/quorum-verify-workspace")
SCOPE = """---
created: 2026-07-26
branch: verify
author: verification fixture
status: active
sprint: VERIFY
---

# Sprint VERIFY

| WP | What | Acceptance |
|----|------|-----------|
| **S9.1** | first work package | holds |
| **S9.2** | second work package | holds |
| **S9.3** | third work package | holds |
| **S9.4** | fourth work package | holds |
| **S9.5** | fifth work package | holds |
| **S9.6** | sixth work package | holds |
| **S9.7** | seventh work package | holds |
"""


def build_fixture() -> str:
    d = FIXTURE / ".agentile/sprints/active/sprint-verify"
    d.mkdir(parents=True, exist_ok=True)
    (d / "SCOPE.md").write_text(SCOPE)
    return str(FIXTURE)


REPO = build_fixture()
APP = Path.home() / ".local/share/ai.citrate.quorum"

def rec():
    tdir = APP / "evidence/tenants"
    if not tdir.is_dir(): return None
    for d in tdir.iterdir():
        f = d / "meetings.json"
        if f.exists():
            m = json.loads(f.read_text())
            return m[0] if m else None
    return None

def chain_len():
    tdir = APP / "evidence/tenants"
    if not tdir.is_dir(): return 0
    for d in tdir.iterdir():
        f = d / "chain.jsonl"
        if f.exists():
            return len([l for l in f.read_text().splitlines() if l.strip()])
    return 0

ok = True
def check(name, cond, detail=""):
    global ok
    print(("\033[32m  PASS\033[0m  " if cond else "\033[31m  FAIL\033[0m  ") + f"{name:<52}" + (f"\033[2m{detail}\033[0m" if detail else ""))
    ok = ok and bool(cond)

s = smoke.Smoke(Path("target/release/citrate-quorum"), ":99", keep=True)
s.setup()
try:
    s.start_app(); time.sleep(3)
    s.click(*smoke.ONBOARD["sso"], settle=3)
    s.click(*smoke.ONBOARD["tenant"]); s.type_text("smoke-tenant")
    s.click(*smoke.ONBOARD["operator"]); s.type_text("Smoke Operator")
    s.click(*smoke.ONBOARD["set_scope"], settle=3)
    for pt in smoke.ONBOARD["tour_next"]: s.click(*pt)
    s.click(*smoke.ONBOARD["enter"], settle=4)
    s.click(smoke.SIDEBAR_X, dict(smoke.SURFACES)["Meetings"], settle=2)

    print("\nschedule")
    s.click(*A(872, 32), settle=2)
    s.click(*A(261, 137)); s.type_text("Weekly Standup")
    s.click(*A(710, 137)); s.type_text("2026-07-30T09:00:00Z")
    s.click(*A(485, 287)); s.type_text(REPO)
    s.click(*A(78, 348), settle=3)
    r = rec()
    check("a meeting exists on disk", r is not None)
    check("agenda generated from the fixture workspace's sprint file",
          r and len(r["agenda"]) == 7, f"{len(r['agenda']) if r else 0} items")
    check("agenda is not frozen yet", r and r["agenda_hash"] is None)

    s.click(*A(200, 110), settle=3)   # open the row

    print("\nattest two humans (quorum is 2)")
    for who in ["R. Ortiz", "M. Okonkwo"]:
        s.click(*A(797, 294)); s.type_text(who)
        s.click(*A(797, 410), settle=2)          # Attest present
    r = rec()
    check("both attested attendees persisted", r and len(r["attendance"]) == 2,
          f"{len(r['attendance']) if r else 0}")
    check("they are humans, so they count for quorum",
          r and all(a["agent"] is None and a["attested"] for a in r["attendance"]))

    print("\nopen — the agenda freezes")
    s.click(*A(518, 520), settle=3)
    r = rec()
    check("state advanced to in-progress", r and r["state"] == "in-progress", r["state"] if r else "")
    check("agenda hash was committed", r and isinstance(r["agenda_hash"], str) and len(r["agenda_hash"]) > 10,
          (r["agenda_hash"][:18] + "…") if r and r["agenda_hash"] else "none")
    frozen = r["agenda_hash"] if r else None

    print("\nclose — minutes composed from the governed record")
    s.click(*A(518, 520), settle=3)          # same banner slot: Close
    r = rec()
    check("state advanced to awaiting ratification", r and r["state"] == "awaiting", r["state"] if r else "")
    check("minutes were composed", r and len(r["minutes"]) >= 1,
          (r["minutes"][0][:46] if r and r["minutes"] else ""))
    check("the frozen agenda hash did not move", r and r["agenda_hash"] == frozen)
    s.shot("lc_closed", smoke.CONTENT_CROP)

    print("\nratify — the ceremony")
    s.click(*A(550, 565), settle=3)          # Ratify — sign
    s.shot("lc_ceremony", None)              # FULL window: the modal is chrome-level
    before_chain = chain_len()
    s.click(870, 650, settle=6)              # Sign  (measured on the full window)
    # THE assertion this whole fix exists for. The ceremony displays "On
    # record" the moment it settles; the record must already exist by then.
    # Before the fix `ceremony.request()` resolved on DISMISS, so the write
    # happened after Close and this read returned "awaiting" while the dialog
    # said the record was made.
    r = rec()
    check("the record exists at Sign, before Close",
          r and r["state"] == "ratified",
          f'{r["state"] if r else "?"} (was "awaiting" before the commit fix)')
    s.click(*A(641, 638), settle=5)          # Close
    r = rec()
    check("the meeting is ratified on disk", r and r["state"] == "ratified", r["state"] if r else "")
    check("the signer is named", r and r["ratified_by"] == "Smoke Operator",
          str(r["ratified_by"]) if r else "")
    check("ratification is itself evidence", chain_len() > before_chain,
          f"chain {before_chain} -> {chain_len()}")
    check("the signed agenda hash is unchanged", r and r["agenda_hash"] == frozen)
    s.shot("lc_ratified", smoke.CONTENT_CROP)

    # WP-S5.7: the workspace named at schedule time is what journals read from,
    # so the brief is only meaningful once a meeting has established it.
    print("\nstandup briefs (S5.7)")
    ws = None
    tdir = APP / "evidence/tenants"
    for d in tdir.iterdir():
        f = d / "workspace.json"
        if f.exists():
            ws = json.loads(f.read_text()).get("workspace")
    check("the workspace persisted for the tenant", ws == REPO, str(ws))

    # Give the tenant an agent to brief. Posting an intent over the keyless
    # bridge is how an agent becomes known — the same path an adapter uses —
    # so the brief is assembled about an agent that really acted.
    b = s.bridge()
    # The response is CHECKED. An unchecked post here failed silently (the body
    # used "class" where the bridge expects "tool") and the brief then read
    # "no agent has acted in this tenant yet" — which is a true sentence about
    # a state the verification itself created. A verification step that can
    # fail quietly proves nothing.
    code, body = s.post_intent(addr := b[0], b[1], {
        "agent": "sbt-41", "tool": "repo.write", "classification": "Public",
        "correlation_id": "X-brief",
    }) if b else (0, "no agent bridge")
    check("an agent acted, so there is somebody to brief", code == 200, f"HTTP {code} {body[:60]}")
    s.click(smoke.SIDEBAR_X, dict(smoke.SURFACES)["Journal"], settle=3)
    img = s.shot("lc_journal", smoke.CONTENT_CROP)
    check("the Journal surface renders with real artifacts", s.stddev(img) > smoke.BLANK_STDDEV,
          f"stddev {s.stddev(img):.0f}")
    print("workdir:", s.work)
finally:
    s.stop_app()
print("\nRESULT:", "ok" if ok else "FAILED")
sys.exit(0 if ok else 1)
