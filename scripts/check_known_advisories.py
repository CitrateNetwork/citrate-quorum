#!/usr/bin/env python3
"""Tell a NEW Rust advisory apart from the known, blocked ones.

Since the expired libcrux / crypto-bigint acceptance was removed (owner
decision 2026-11-27, not renewed), `cargo audit` is red on nine advisories
that cannot be fixed from this repository until citrate-comms moves to
openmls 0.9 / openmls_rust_crypto 0.6. While that red lasts, one more advisory
would leave the job exactly as red as before. This check fails, with its own
message, on any reported vulnerability that is NOT in BLOCKED below.

This is not an acceptance. It suppresses nothing: `cargo audit` still runs
first and still fails on all nine. When an entry in BLOCKED stops being
reported, this check fails too, so the list can only shrink.

Usage: check_known_advisories.py audit.json      (output of `cargo audit --json`)
       check_known_advisories.py --run-cargo-audit (runs it in the repo root)
       check_known_advisories.py --self-test
"""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Each is reported today and blocked upstream (see PR #21 / L2-WP02a).
BLOCKED = {
    "RUSTSEC-2026-0124",  # libcrux-chacha20poly1305 0.0.7 (hpke-rs 0.6.1)
    "RUSTSEC-2026-0207",  # libcrux-sha3 0.0.8
    "RUSTSEC-2026-0208",  # libcrux-sha3 0.0.8
    "RUSTSEC-2026-0209",  # libcrux-aesgcm 0.0.7
    "RUSTSEC-2026-0211",  # libcrux-aesgcm 0.0.7
    "RUSTSEC-2026-0212",  # libcrux-secrets 0.0.5
    "RUSTSEC-2026-0330",  # libcrux-kem 0.0.7
    "RUSTSEC-2026-0331",  # libcrux-kem 0.0.7
    "RUSTSEC-2026-0387",  # crypto-bigint 0.5.5 (elliptic-curve 0.13)
}


def check(report: dict) -> list[str]:
    reported = {v["advisory"]["id"] for v in report["vulnerabilities"]["list"]}
    errors = [f"NEW advisory not in the blocked set: {i}" for i in sorted(reported - BLOCKED)]
    errors += [f"{i} is no longer reported: delete it from BLOCKED" for i in sorted(BLOCKED - reported)]
    return errors


def _report(*ids: str) -> dict:
    return {"vulnerabilities": {"list": [{"advisory": {"id": i}} for i in ids]}}


def self_test() -> None:
    cases = [
        ("exactly the blocked set", _report(*BLOCKED), 0),
        ("one new advisory", _report(*BLOCKED, "RUSTSEC-2099-0001"), 1),
        ("one blocked advisory fixed", _report(*sorted(BLOCKED)[1:]), 1),
        ("empty report", _report(), len(BLOCKED)),
    ]
    bad = [f"{n}: expected {w}, got {len(check(r))}" for n, r, w in cases if len(check(r)) != w]
    if bad:
        print("self-test FAILED:", *bad, sep="\n  ")
        sys.exit(1)
    print(f"self-test: {len(cases)} cases pass")


def main() -> None:
    if sys.argv[1:] == ["--self-test"]:
        self_test()
        return
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(2)
    if sys.argv[1] == "--run-cargo-audit":
        # cargo audit exits non-zero while anything is reported; the JSON on
        # stdout is what this check judges.
        text = subprocess.run(["cargo", "audit", "--json"], cwd=ROOT,
                              capture_output=True, text=True).stdout
    else:
        with open(sys.argv[1], encoding="utf-8") as f:
            text = f.read()
    if not text.strip():
        print("cargo audit produced no JSON")
        sys.exit(1)
    errors = check(json.loads(text))
    if errors:
        print(*errors, sep="\n")
        sys.exit(1)
    print(f"only the {len(BLOCKED)} known, blocked advisories are reported")


if __name__ == "__main__":
    main()
