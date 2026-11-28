#!/usr/bin/env python3
"""Tell a NEW Rust advisory apart from the known, blocked ones.

Since the expired libcrux / crypto-bigint acceptance was removed (owner
decision 2026-11-27, not renewed), `cargo audit` is red on advisories that
cannot be fixed from this repository. citrate-comms moved to openmls 0.9.2 /
openmls_rust_crypto 0.6 (comms #61), which cleared six of the original nine;
three remain (see BLOCKED). While that red lasts, one more advisory
(or one more affected crate under a known ID) would leave the job exactly as
red as before. This check fails, with its own message, on any reported
(advisory, crate, version) that is NOT in BLOCKED below.

This is not an acceptance. It suppresses nothing: `cargo audit` still runs
first and still fails on every entry. When an entry in BLOCKED stops being
reported, this check fails too, so the list can only shrink. It also refuses
to judge when a user-level audit.toml exists, because cargo-audit would apply
that file's ignores to the report before this script sees it.

Usage: check_known_advisories.py audit.json      (output of `cargo audit --json`)
       check_known_advisories.py --run-cargo-audit (runs it in the repo root)
       check_known_advisories.py --self-test
"""
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# (advisory, crate, version): each reported today and blocked upstream
# (PR #21 / L2-WP02a).
BLOCKED = {
    # lockfile only: hpke-rs-libcrux 0.7 is not compiled (cargo tree -i libcrux-kem is empty);
    # clears with openmls_rust_crypto 0.7 / hpke-rs 0.8 (openmls 0.10) in citrate-comms
    ("RUSTSEC-2026-0330", "libcrux-kem", "0.0.9"),
    ("RUSTSEC-2026-0331", "libcrux-kem", "0.0.9"),
    # elliptic-curve 0.13 (k256 / p256 / p384); on the wallet secret-key path, not accepted
    ("RUSTSEC-2026-0387", "crypto-bigint", "0.5.5"),
}


def user_audit_configs() -> list[Path]:
    """User-level audit.toml files cargo-audit may read ($CARGO_HOME, ~/.cargo)."""
    homes = {Path(os.path.expanduser("~")) / ".cargo"}
    if os.environ.get("CARGO_HOME"):
        homes.add(Path(os.environ["CARGO_HOME"]))
    return sorted(p for p in (h / "audit.toml" for h in homes) if p.exists())


def reported(report: dict) -> set[tuple[str, str, str]]:
    out = set()
    for v in report["vulnerabilities"]["list"]:
        out.add((v["advisory"]["id"], v["package"]["name"], v["package"]["version"]))
    return out


def check(report: dict) -> list[str]:
    got = reported(report)
    errors = [f"NEW: {i} in {n} {v} is not in the blocked set" for i, n, v in sorted(got - BLOCKED)]
    errors += [f"{i} in {n} {v} is no longer reported: delete it from BLOCKED"
               for i, n, v in sorted(BLOCKED - got)]
    return errors


def _report(*entries: tuple[str, str, str]) -> dict:
    return {"vulnerabilities": {"list": [
        {"advisory": {"id": i}, "package": {"name": n, "version": v}} for i, n, v in entries]}}


def self_test() -> None:
    cases = [
        ("exactly the blocked set", _report(*BLOCKED), 0),
        ("one new advisory", _report(*BLOCKED, ("RUSTSEC-2099-0001", "foo", "1.0.0")), 1),
        ("known id on a second crate version",
         _report(*BLOCKED, ("RUSTSEC-2026-0387", "crypto-bigint", "0.6.4")), 1),
        ("one blocked advisory fixed", _report(*sorted(BLOCKED)[1:]), 1),
        ("empty report", _report(), len(BLOCKED)),
    ]
    bad = [f"{n}: expected {w}, got {len(check(r))}" for n, r, w in cases if len(check(r)) != w]
    if bad:
        print("self-test FAILED:", *bad, sep="\n  ")
        sys.exit(1)
    print(f"self-test: {len(cases)} cases pass")


def fail(msg: str) -> None:
    print(msg)
    sys.exit(1)


def main() -> None:
    if sys.argv[1:] == ["--self-test"]:
        self_test()
        return
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(2)
    users = user_audit_configs()
    if users:
        fail("refusing to judge: a user-level audit.toml can hide advisories from the "
             f"report ({', '.join(map(str, users))}); move it aside and re-run")
    if sys.argv[1] == "--run-cargo-audit":
        # cargo audit exits non-zero while anything is reported; the JSON on
        # stdout is what this check judges.
        try:
            proc = subprocess.run(["cargo", "audit", "--json"], cwd=ROOT,
                                  capture_output=True, text=True)
        except OSError as e:
            fail(f"could not run cargo audit: {e}")
        text, stderr = proc.stdout, proc.stderr
    else:
        with open(sys.argv[1], encoding="utf-8") as f:
            text, stderr = f.read(), ""
    if not text.strip():
        fail("cargo audit produced no JSON" + (f": {stderr.strip()[-600:]}" if stderr.strip() else ""))
    try:
        errors = check(json.loads(text))
    except (json.JSONDecodeError, KeyError, TypeError) as e:
        fail(f"cargo audit JSON is not in the expected shape ({type(e).__name__}: {e})")
    if errors:
        fail("\n".join(errors))
    print(f"only the {len(BLOCKED)} known, blocked advisories are reported")


if __name__ == "__main__":
    main()
