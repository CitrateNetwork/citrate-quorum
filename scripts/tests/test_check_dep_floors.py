"""Tests for scripts/check_dep_floors.py — run: python3 -m unittest discover -s scripts/tests"""
import importlib.util
import unittest
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "check_dep_floors", Path(__file__).resolve().parent.parent / "check_dep_floors.py"
)
floors = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(floors)


def lock(*pkgs: tuple[str, str]) -> str:
    out = ["version = 4", ""]
    for name, ver in pkgs:
        out += ["[[package]]", f'name = "{name}"', f'version = "{ver}"', ""]
    return "\n".join(out)


FLOOR = {"openmls": (0, 8, 2)}


class DepFloors(unittest.TestCase):
    def test_version_below_floor_is_reported(self):
        v = floors.violations(lock(("openmls", "0.8.1")), FLOOR)
        self.assertEqual(len(v), 1)
        self.assertIn("openmls 0.8.1", v[0])

    def test_version_at_or_above_floor_passes(self):
        for ver in ("0.8.2", "0.8.10", "0.9.1"):
            self.assertEqual(floors.violations(lock(("openmls", ver)), FLOOR), [], ver)

    def test_numeric_not_lexical_comparison(self):
        # "0.8.10" > "0.8.2" numerically but < lexically; "0.10.0" > "0.9.9".
        self.assertEqual(floors.violations(lock(("openmls", "0.10.0")), FLOOR), [])
        self.assertEqual(len(floors.violations(lock(("openmls", "0.7.99")), FLOOR)), 1)

    def test_every_duplicate_copy_is_checked(self):
        # Two semver-incompatible copies in one graph: the old one must not hide.
        v = floors.violations(lock(("openmls", "0.9.1"), ("openmls", "0.8.1")), FLOOR)
        self.assertEqual(len(v), 1)

    def test_prerelease_of_the_floor_is_below_it(self):
        self.assertEqual(len(floors.violations(lock(("openmls", "0.8.2-rc.1")), FLOOR)), 1)

    def test_absent_crate_is_not_a_violation(self):
        self.assertEqual(floors.violations(lock(("serde", "1.0.0")), FLOOR), [])

    def test_unparseable_version_fails_closed(self):
        self.assertEqual(len(floors.violations(lock(("openmls", "garbage")), FLOOR)), 1)

    def test_committed_lockfile_meets_every_floor(self):
        text = (Path(__file__).resolve().parents[2] / "Cargo.lock").read_text()
        self.assertEqual(floors.violations(text), [])


if __name__ == "__main__":
    unittest.main()
