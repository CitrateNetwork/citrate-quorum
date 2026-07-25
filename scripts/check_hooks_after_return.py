#!/usr/bin/env python3
"""citrate-quorum — no React hook after a top-level conditional return.

Why this exists: the S2D.4 honesty pass added an early return to eleven
surfaces ("this read failed — here is why"). In two of them a `useMemo` sat
*below* the insertion point, so on a failed read React rendered fewer hooks
than the previous render and crashed with "rendered fewer hooks than
expected". Neither `tsc` nor the unit tests can see it: the types are fine and
the crash only fires when that surface is open AND its read fails — exactly the
path a happy-path check never takes.

The mature fix is eslint's `react-hooks/rules-of-hooks`. This repo has no
eslint yet, so this is a narrow tripwire for the specific bug that bit us, and
it should be deleted the day eslint lands.

What it looks at: inside an `export function <Component>`, a CONDITIONAL return
at the component's top level — either

    if (cond) return ...;                       (one line)
    if (cond) {                                 (block, `}` back at 2 spaces)
      ...
      return ...;
    }

— and then any hook call at the top level after that. Indentation is measured
exactly, so returns nested inside callbacks or JSX are not mistaken for
component-level control flow (an earlier version of this script did exactly
that and was useless).

Limits, stated plainly: it reasons about indentation, not scope, and only
about `export function` components. It is a smoke alarm, not a proof.

Exit 0 = clean, 1 = at least one violation.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

HOOK = re.compile(
    r"^  (?:const|let|var)?\s*[\w{}\[\],:\s]*=?\s*"
    r"(useState|useEffect|useMemo|useCallback|useRef|useReducer|useContext|useLayoutEffect|useCeremony|useDomain)\s*\("
)
COMPONENT = re.compile(r"^export function ([A-Z]\w*)")
IF_ONELINE = re.compile(r"^  if \(.*\)\s*return\b")
IF_BLOCK = re.compile(r"^  if \(.*\)\s*\{\s*$")


def scan(path: Path) -> list[str]:
    problems: list[str] = []
    lines = path.read_text().splitlines()

    i = 0
    n = len(lines)
    while i < n:
        m = COMPONENT.match(lines[i])
        if not m:
            i += 1
            continue
        name = m.group(1)
        # Component body runs until a line that is exactly "}".
        end = i + 1
        while end < n and lines[end] != "}":
            end += 1

        guard_line: int | None = None
        j = i + 1
        while j < end:
            line = lines[j]
            if IF_ONELINE.match(line):
                if guard_line is None:
                    guard_line = j
                j += 1
                continue
            if IF_BLOCK.match(line):
                # Find the matching "  }" and see whether the block returns.
                k = j + 1
                returns = False
                while k < end and lines[k] != "  }":
                    if lines[k].strip().startswith("return"):
                        returns = True
                    k += 1
                if returns and guard_line is None:
                    guard_line = j
                j = k + 1
                continue
            if guard_line is not None and HOOK.match(line):
                problems.append(
                    f"{path}:{j + 1}: hook runs after the conditional return at "
                    f"line {guard_line + 1} in <{name}> — when that return is "
                    f"taken React renders fewer hooks and crashes.\n"
                    f"      {line.strip()[:100]}"
                )
            j += 1
        i = end + 1
    return problems


def main() -> int:
    root = Path(__file__).resolve().parent.parent / "src"
    if not root.is_dir():
        print("no src/ — nothing to scan")
        return 0
    problems: list[str] = []
    for f in sorted(root.rglob("*.tsx")):
        problems.extend(scan(f))
    if problems:
        print("React hooks after a top-level conditional return:\n")
        for p in problems:
            print("  " + p)
        return 1
    print("no hooks after a top-level conditional return")
    return 0


if __name__ == "__main__":
    sys.exit(main())
