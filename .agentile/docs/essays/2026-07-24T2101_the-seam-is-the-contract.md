---
created: 2026-07-24T21:01:00Z
branch: docs/qrm-journals-retros-essays
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# The seam is the contract

> A boundary you can delete is the only boundary you can trust.

## Frame

Most software boundaries are conveniences. We draw an interface between two
modules because it is tidy, because it lets two people work in parallel, because
it might one day let us swap an implementation. These are good reasons, but they
are soft: nothing enforces the boundary, and over time surfaces reach through it,
implementations leak upward, and the "interface" becomes a suggestion the code
no longer honors.

citrate-quorum's bridge seam is not that kind of boundary. It is a load-bearing
contract with a mechanical test of its own integrity, and the discipline it
encodes is worth naming because it generalizes: **the trustworthiness of a seam
is measured by what happens when you delete one side of it.**

## Two adapters, one interface

The bridge is one interface — `bridge/domains.ts`, twelve typed domains, every
method a `Promise` or a `Subscribe<E>` stream — with two implementations behind
it. The `sim` adapter serves scripted prototype data. The `tauri` adapter calls
real Rust, and throws `Unavailable` for anything not yet wired. A runtime switch
picks one at the boundary: `sim` in dev and web, `tauri` in the packaged app.

The surfaces — every screen a user sees — import *only* the interface. Not the
`sim` data, not the `tauri` commands, not the mode switch. A surface cannot tell
which adapter is answering it, and that ignorance is the point. Flipping a domain
from scripted to live changes one adapter and zero surfaces.

## The test that makes it real

An interface that surfaces *could* reach around is not a contract; it is a
naming convention. What turns citrate-quorum's seam into a contract is a single
acceptance property, enforced in CI:

> Delete `src/bridge/sim/` and the application must still compile.

Run it and you get exactly one error, at `bridge/index.ts`, where the mode
switch names the now-missing adapter. Zero surfaces break. That result is a
proof: if no surface breaks when the fake data is deleted, then no surface was
depending on the fake data. The prototype is not load-bearing. It cannot become
load-bearing, because the next person who wires a surface to `sim` data directly
will break this test.

Three more scans back it up — no `invoke` in surfaces, no `sim` data outside the
bridge, no hardcoded chain hex in surfaces. Together they make "surfaces depend
only on the interface" a fact the build checks, not a habit the team maintains.

## Why a governance app in particular needs this

Every product wants clean boundaries. A T1 governance app *needs* this specific
one, because its prototype is made of the one substance the product forbids:
convincing fake data. Rule 1 says no surface may render fabricated data as real.
A design prototype is nothing but fabricated data rendered to look real — that is
its job. Without a deletable seam, "port the prototype" and "obey Rule 1" are in
direct conflict, and the conflict resolves, quietly, in favor of whatever shipped.

The seam dissolves the conflict. Design-time realism lives entirely inside
`bridge/sim/`, quarantined. Run-time honesty is the tauri adapter's `Unavailable`
for anything unbuilt. The same twelve interfaces serve both, so the prototype's
realism and the product's honesty are the same shape viewed from two sides — and
the deletion test guarantees the sides never fuse.

## What this implies

- **New domains get an interface first, an adapter second.** The interface is
  the artifact that outlives both adapters; write it as if the sim adapter did
  not exist.
- **"Not wired yet" is a first-class state, not a gap.** The tauri adapter
  should throw `Unavailable` (or render a branded honest panel) rather than
  return empty or borrowed data. An honest missing surface beats a dishonest
  present one.
- **The deletion test is a permanent CI gate, not a one-time check.** Its value
  is entirely in running on every change; a seam is only as trustworthy as the
  last commit that proved it deletable.

## What this does NOT imply

- It does not imply the sim adapter is throwaway. It is the substrate the whole
  frontend was QA'd on and the reference for what each live adapter must
  reproduce. "Deletable" means "not load-bearing," not "worthless."
- It does not imply every boundary in the app should have a deletion test. This
  discipline is worth its cost exactly where one side is *untrusted by
  construction* — prototype data, mocks, fixtures. Applying it everywhere would
  be ceremony.

## References

- Sprint QRM-S2D retro: `.agentile/sprints/completed/sprint-qrm-s2d/RETRO.md`
- Journal it grew out of: `.agentile/docs/journals/2026-07-24T2055_prototype-to-react-via-the-seam.md`
- Code: `src/bridge/{domains.ts,mode.ts}`, the S2D.4 acceptance check in `scripts/check.sh`
- Companion essay: `.agentile/docs/essays/2026-07-24T2103_honest-by-construction.md`
