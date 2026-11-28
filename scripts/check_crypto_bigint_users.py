#!/usr/bin/env python3
"""Keep the RUSTSEC-2026-0387 audit ignore honest (crypto-bigint < 0.7.6).

The advisory is fixed only in crypto-bigint 0.7.6. Here it comes in through
elliptic-curve 0.13 (the k256 / p256 / p384 0.13 line), and no 0.13.x release
accepts crypto-bigint 0.7, so the lockfile cannot move it. The audit step
ignores the advisory, and this check fails CI when the reason for that ignore
stops being true:

1. a crate other than elliptic-curve 0.13.x depends on a crypto-bigint below
   0.7.6 (the ignore does not cover a new user);
2. no crypto-bigint below 0.7.6 is left in Cargo.lock (the ignore is stale:
   delete it, and this check with it);
3. today is after EXPIRES (decide again; do not renew by reflex).

The guard only has a subject while the ignore can be in effect. When neither
the repo's `.cargo/audit.toml` nor the user-level `$CARGO_HOME/audit.toml`
(which cargo-audit also reads) could be ignoring the advisory (the acceptance
was removed 2026-11-27 by owner decision, not renewed), `cargo audit` itself
reports RUSTSEC-2026-0387 and fails, so this check passes with a note.
Re-adding the ignore re-engages every rule above. A config this script cannot
read with certainty (unparseable, an unexpected shape, or a `[database]`,
`[target]` or severity setting that can filter advisories without naming
them) counts as ignoring it, so the guard runs (fail closed).

Usage: check_crypto_bigint_users.py [--lock Cargo.lock] [--self-test]
"""
import argparse
import os
import sys
import tomllib
from datetime import date
from pathlib import Path

ADVISORY = "RUSTSEC-2026-0387"
EXPIRES = date(2026, 11, 9)
FIXED = (0, 7, 6)
ALLOWED_USER = "elliptic-curve"
ALLOWED_USER_LINE = (0, 13)


AUDIT_TOML = Path(__file__).resolve().parent.parent / ".cargo" / "audit.toml"
# Allowlist of config keys known NOT to drop an advisory from the report. Any
# other key (a `[database]` or `[target]` table, `severity_threshold`, or one a
# future cargo-audit adds) counts as "could be ignoring" (fail closed).
SAFE_TOP_KEYS = {"advisories", "output", "yanked"}
SAFE_ADVISORY_KEYS = {"ignore", "informational_warnings"}


def audit_configs() -> list[Path]:
    """Every audit.toml cargo-audit may read here: the repo's and the user's
    ($CARGO_HOME and ~/.cargo, when they differ)."""
    homes = [Path(os.path.expanduser("~")) / ".cargo"]
    if os.environ.get("CARGO_HOME"):
        homes.append(Path(os.environ["CARGO_HOME"]))
    return [AUDIT_TOML] + [h / "audit.toml" for h in dict.fromkeys(homes)]


def is_ignored(audit_text: str | None) -> bool:
    """True when this audit config could be ignoring ADVISORY (fail closed)."""
    if audit_text is None:
        return False
    try:
        cfg = tomllib.loads(audit_text)
    except tomllib.TOMLDecodeError:
        # An unparseable config cannot be shown to be free of the ignore.
        return True
    if set(cfg) - SAFE_TOP_KEYS:
        return True
    adv = cfg.get("advisories", {})
    if not isinstance(adv, dict):
        return True
    if set(adv) - SAFE_ADVISORY_KEYS:
        return True
    ignore = adv.get("ignore", [])
    if not isinstance(ignore, list) or not all(isinstance(i, str) for i in ignore):
        return True
    return ADVISORY.upper() in (i.strip().upper() for i in ignore)


def vtuple(v: str) -> tuple[int, ...]:
    return tuple(int(p) for p in v.split("-")[0].split("+")[0].split(".")[:3])


def check(lock: dict, today: date) -> list[str]:
    """Return the failures for one parsed Cargo.lock (empty list = pass)."""
    pkgs = lock.get("package", [])
    versions = [p["version"] for p in pkgs if p.get("name") == "crypto-bigint"]
    old = {v for v in versions if vtuple(v) < FIXED}
    errors = []
    if today > EXPIRES:
        errors.append(f"the {ADVISORY} ignore expired on {EXPIRES}: decide again")
    if not old:
        errors.append(
            f"no crypto-bigint below {'.'.join(map(str, FIXED))} in Cargo.lock: "
            f"the {ADVISORY} ignore is stale, remove it and this check"
        )
        return errors
    for p in pkgs:
        for dep in p.get("dependencies", []):
            parts = dep.split()
            if parts[0] != "crypto-bigint":
                continue
            # A bare name means the lock holds exactly one crypto-bigint.
            dv = parts[1] if len(parts) > 1 else (versions[0] if len(versions) == 1 else None)
            if dv is not None and dv not in old:
                continue
            ok = p["name"] == ALLOWED_USER and vtuple(p["version"])[:2] == ALLOWED_USER_LINE
            if not ok:
                errors.append(
                    f"{p['name']} {p['version']} -> crypto-bigint {dv or '?'}: "
                    f"not covered by the {ADVISORY} ignore"
                )
    return errors


def _lock(*pkgs: tuple[str, str, list[str]]) -> dict:
    return {"package": [{"name": n, "version": v, "dependencies": d} for n, v, d in pkgs]}


def self_test() -> None:
    ok_day = date(2026, 10, 25)
    base = [("crypto-bigint", "0.5.5", []), ("elliptic-curve", "0.13.8", ["crypto-bigint"])]
    cases = [
        ("allowed user only", _lock(*base), ok_day, 0),
        ("expired", _lock(*base), date(2026, 11, 10), 1),
        ("new direct user", _lock(*base, ("rsa", "0.9.8", ["crypto-bigint"])), ok_day, 1),
        ("elliptic-curve 0.14 on old bigint",
         _lock(("crypto-bigint", "0.5.5", []), ("elliptic-curve", "0.14.3", ["crypto-bigint"])), ok_day, 1),
        ("stale: only fixed bigint",
         _lock(("crypto-bigint", "0.7.10", []), ("elliptic-curve", "0.14.3", ["crypto-bigint"])), ok_day, 1),
        ("two versions, new user on the fixed one",
         _lock(("crypto-bigint", "0.5.5", []), ("crypto-bigint", "0.7.10", []),
               ("elliptic-curve", "0.13.8", ["crypto-bigint 0.5.5"]),
               ("rsa", "0.10.0", ["crypto-bigint 0.7.10"])), ok_day, 0),
        ("two versions, new user on the old one",
         _lock(("crypto-bigint", "0.5.5", []), ("crypto-bigint", "0.7.10", []),
               ("elliptic-curve", "0.13.8", ["crypto-bigint 0.5.5"]),
               ("rsa", "0.9.8", ["crypto-bigint 0.5.5"])), ok_day, 1),
    ]
    bad = []
    ignore_cases = [
        ("no audit.toml", None, False),
        ("audit.toml without the advisory", '[advisories]\nignore = ["RUSTSEC-2026-0207"]\n', False),
        ("advisory only in a comment", f'# {ADVISORY} was accepted once\n[advisories]\nignore = []\n', False),
        ("advisory ignored", f'[advisories]\nignore = ["{ADVISORY}"]\n', True),
        ("unparseable audit.toml", "[advisories\n", True),
        ("lowercase id", f'[advisories]\nignore = ["{ADVISORY.lower()}"]\n', True),
        ("advisories not a table", "advisories = 1\n", True),
        ("ignore not a list", "[advisories]\nignore = 5\n", True),
        ("ignore as a string", f'[advisories]\nignore = "{ADVISORY}"\n', True),
        ("custom advisory database", '[database]\nfetch = false\n', True),
        ("severity threshold", '[advisories]\nseverity_threshold = "high"\n', True),
        ("unknown future key", '[advisories]\nfilter = ["x"]\n', True),
        ("output settings only", '[output]\nquiet = true\n', False),
    ]
    for name, text, want in ignore_cases:
        if is_ignored(text) != want:
            bad.append(f"{name}: expected is_ignored={want}")
    for name, lock, day, want in cases:
        got = len(check(lock, day))
        if got != want:
            bad.append(f"{name}: expected {want} failure(s), got {got}")
    if bad:
        print("self-test FAILED:", *bad, sep="\n  ")
        sys.exit(1)
    print(f"self-test: {len(cases) + len(ignore_cases)} cases pass")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--lock", default="Cargo.lock")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        self_test()
        return
    if not os.path.exists(args.lock):
        print(f"{args.lock}: no such lockfile")
        sys.exit(1)
    in_effect = [p for p in audit_configs()
                 if is_ignored(p.read_text() if p.exists() else None)]
    if not in_effect:
        print(f"{ADVISORY} is not ignored in any audit.toml cargo-audit reads here: "
              f"nothing to guard (cargo audit reports it directly)")
        return
    with open(args.lock, "rb") as f:
        lock = tomllib.load(f)
    errors = check(lock, date.today())
    if errors:
        print(f"{ADVISORY} ignore check failed:", *errors, sep="\n  ")
        sys.exit(1)
    print(f"{ADVISORY}: crypto-bigint < 0.7.6 only via {ALLOWED_USER} 0.13; ignore expires {EXPIRES}")


if __name__ == "__main__":
    main()
