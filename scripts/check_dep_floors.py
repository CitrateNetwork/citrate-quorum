#!/usr/bin/env python3
"""citrate-quorum — minimum-version floors on the lockfile.

`cargo audit` only knows what the RustSec database knows. When an upstream crate
ships a patched release before the advisory DB covers it, nothing stops the
lockfile drifting back below that release. A floor here closes that gap: every
copy of a listed crate in `Cargo.lock` must be at or above its floor.

A floor is not a pin — any newer version passes. Remove an entry once the
advisory DB covers it (then `cargo audit` holds the line on its own).

Run by `scripts/check.sh` and the rust-deps-audit workflow. Exits non-zero with
each violation.
"""
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# crate -> lowest acceptable release.
FLOORS: dict[str, tuple[int, ...]] = {
    "openmls": (0, 8, 2),  # dependency update to the patched release
}

_SEMVER = re.compile(r"^(\d+)\.(\d+)\.(\d+)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$")


def _below(version: str, floor: tuple[int, ...]) -> bool:
    m = _SEMVER.match(version)
    if m is None:
        return True  # cannot prove it meets the floor: fail closed
    core = (int(m[1]), int(m[2]), int(m[3]))
    if core != floor:
        return core < floor
    return m[4] is not None  # a pre-release of the floor precedes the floor


def violations(lock_text: str, floors: dict[str, tuple[int, ...]] = FLOORS) -> list[str]:
    out = []
    for pkg in tomllib.loads(lock_text).get("package", []):
        floor = floors.get(pkg.get("name", ""))
        if floor is not None and _below(str(pkg.get("version", "")), floor):
            want = ".".join(map(str, floor))
            out.append(f"{pkg['name']} {pkg.get('version')} is below the floor {want}")
    return out


def main() -> int:
    found = violations((ROOT / "Cargo.lock").read_text())
    for v in found:
        print(f"  {v}")
    if not found:
        print(f"  {len(FLOORS)} floor(s) met")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
