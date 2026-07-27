#!/usr/bin/env python3
"""citrate-quorum — keep `.cargo/audit.toml` honest.

An ignored advisory is an accepted risk, and an accepted risk that nobody
re-examines is just a hidden one. `cargo audit` exits 0 once an advisory is in
the ignore list, so the ignore file is exactly where a security finding goes to
be forgotten.

This fails the gate when any of the following is true:

1. **The expiry has passed.** Forces a fresh decision rather than renewal by
   default.
2. **An ignored advisory is no longer reported.** The dependency moved, or the
   advisory was withdrawn — either way the ignore is stale and must be deleted,
   because a stale ignore silently covers whatever takes that ID's place in
   someone's reasoning.
3. **The documented blocker lifted.** Every ignore here exists because
   `hpke-rs 0.6.x` will not take a fixed `libcrux`. The moment the lockfile
   carries `hpke-rs >= 0.7`, these are fixable and the ignore is no longer
   honest.

Run by `scripts/check.sh`. Exits non-zero with the reason.
"""
import json
import re
import shutil
import subprocess
import sys
import tempfile
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
AUDIT_TOML = ROOT / ".cargo/audit.toml"

# The condition that makes every ignore in this file unnecessary. Encoded, not
# just described, so it cannot quietly become untrue.
BLOCKER_CRATE = "hpke-rs"
BLOCKER_FIXED_AT = (0, 7, 0)


def fail(msg: str) -> None:
    print(f"  {msg}")
    sys.exit(1)


def parse_ignores(text: str) -> tuple[list[str], date | None]:
    ids = re.findall(r'"(RUSTSEC-\d{4}-\d{4})"', text)
    m = re.search(r"EXPIRES:\s*(\d{4})-(\d{2})-(\d{2})", text)
    expiry = date(int(m[1]), int(m[2]), int(m[3])) if m else None
    return ids, expiry


def version_tuple(v: str) -> tuple[int, ...]:
    return tuple(int(p) for p in re.findall(r"\d+", v)[:3])


def main() -> None:
    if not AUDIT_TOML.exists():
        print("  no .cargo/audit.toml — nothing is being ignored, which is the ideal state")
        return

    text = AUDIT_TOML.read_text()
    ignored, expiry = parse_ignores(text)
    if not ignored:
        print("  audit.toml carries no ignores")
        return
    if expiry is None:
        fail("audit.toml has ignores but no `EXPIRES: YYYY-MM-DD` — an acceptance "
             "without an end date is a permanent hole")

    if date.today() > expiry:
        fail(f"the audit-ignore acceptance EXPIRED on {expiry}. Re-decide: is each "
             f"advisory still unfixable, still accepted, and still correctly "
             f"described? Then move the date. Do not renew by reflex.")

    # `cargo audit` DROPS ignored advisories from its report entirely — with the
    # ignore file in place the count is 0 and they appear nowhere, so asking the
    # normal run whether an ignore is still needed always answers "no". Run it
    # against a copy of the lockfile from a directory that has no `.cargo/audit.toml`
    # to get the unfiltered truth. (Discovered by this script failing on its first
    # run, which is the correct behaviour for a check that cannot be wrong quietly.)
    with tempfile.TemporaryDirectory() as tmp:
        shutil.copy(ROOT / "Cargo.lock", Path(tmp) / "Cargo.lock")
        proc = subprocess.run(
            ["cargo", "audit", "--json", "-f", "Cargo.lock"],
            cwd=tmp,
            capture_output=True,
            text=True,
        )
    if not proc.stdout.strip():
        fail(f"cargo audit produced no JSON: {proc.stderr.strip()[:400]}")
    report = json.loads(proc.stdout)

    reported = {v["advisory"]["id"] for v in report["vulnerabilities"]["list"]}
    stale = [i for i in ignored if i not in reported]
    if stale:
        fail("these advisories are ignored but no longer reported — delete them from "
             f".cargo/audit.toml: {', '.join(stale)}")

    # Has the blocker lifted?
    for pkg in report.get("lockfile", {}).get("dependencies", []) or []:
        if pkg.get("name") == BLOCKER_CRATE:
            if version_tuple(pkg.get("version", "0")) >= BLOCKER_FIXED_AT:
                fail(f"{BLOCKER_CRATE} is now {pkg['version']} — the blocker these "
                     f"ignores rest on has LIFTED. The libcrux advisories should be "
                     f"fixable; re-check and remove the ignores.")

    print(f"  {len(ignored)} accepted, expires {expiry}, blocker still in place")


if __name__ == "__main__":
    main()
