---
created: 2026-07-24T21:03:00Z
branch: docs/qrm-journals-retros-essays
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# Honest by construction

> Honesty that depends on discipline decays. Honesty that depends on the build
> is a property of the software.

## Frame

Rule 1 of this codebase — no mocks, no fabricated data presented as real, no
placeholder that pretends to work — reads like a conduct rule, the kind of thing
you put in a CONTRIBUTING file and hope people remember. Treated that way, it
fails, because the pressure to fake it is strongest exactly when a demo is due
and nobody is looking closely. The interesting move in citrate-quorum is to stop
treating honesty as conduct and start treating it as *architecture*: to arrange
the code so that the honest thing is the path of least resistance and the
dishonest thing does not compile, does not pass the gate, or does not have
anywhere to live. This essay collects the several mechanisms that, together, make
honesty structural rather than aspirational.

## Empty is a real answer

The smallest and most important one: an empty result is returned as empty. The
Ledger reads a tenant's hash chain; if the chain has no records, the surface
shows no records. There is no seeding, no "sample data so it doesn't look
broken," no demo rows. `ledger_rows` on an unknown tenant returns `[]`, and the
test `empty_tenant_ledger_is_honestly_empty` pins it there. This sounds trivial
until you notice how much fake data enters real products through the side door of
"it looked empty so I added examples." Deciding, and testing, that empty is a
first-class state closes that door.

## The gate reports skips as skips

The local gate, `scripts/check.sh`, has an honesty rule of its own: a check that
cannot run reports `SKIP` with a reason. It never reports `PASS`. This is the
difference between "we verified this" and "we did not verify this," and collapsing
the two — a green check that actually ran nothing — is the most common way a CI
dashboard lies. The gate's own output is therefore held to Rule 1: it states what
it actually did. When the frontend has zero component tests, the honest number is
zero, printed as zero, not disguised by a check that trivially passes.

## Unbuilt surfaces say so

The tauri adapter throws `Unavailable` for every domain not yet wired. A packaged
build with an unbuilt Rooms surface does not show borrowed data or a hopeful
spinner forever; it shows an honest error. The unbuilt state is represented, in
the shipped artifact, as unbuilt. Combined with the deletable sim seam (see the
companion essay), this means the app has exactly two honest modes — real data, or
a named absence — and no third mode where something looks real and is not.

## The compiler and the ratchets do the remembering

Honesty-as-discipline asks humans to remember, under deadline, to be honest.
Honesty-as-construction moves the remembering into tools that do not get tired.
The `no_mocks` ratchet scans for fabricated-data patterns. The `no_unwraps`
ratchet keeps panics out of production paths. The test-monotone ratchet forbids
the count from dropping, so "I deleted the failing test" stops being an option.
The wiring-contract scans forbid a surface from reaching around the bridge. Even
`dead_code` warnings, which the workspace denies, turned out to enforce honesty:
two of them, chased down instead of suppressed, were a cross-tenant leak and a
missing ungoverned-count — capabilities defined but not honestly wired. The
build, not the author, caught them.

## Why a governance product cannot treat this as optional

For most software, a little demo fakery is a venial sin. For a T1 governance
product whose deliverable is *audit evidence* — a tamper-evident record an
outside auditor will sample — it is the cardinal one. The entire value
proposition is that when the Ledger says an action happened under a given
authority, it did, and the hash chain proves no one edited the claim afterward.
A single fabricated row, anywhere, poisons that proposition retroactively: if one
number can be fake, an auditor must treat all of them as possibly fake. Honesty
here is not a virtue signal; it is the product. Which is exactly why it must be
enforced by construction and not by intention — the product cannot be one
deadline away from a fake row.

## What this implies

- **Every new surface answers the "where does this number come from?" question
  in code** (Rule 11: surface names its command, command names its source). A
  number with no stateable origin does not ship.
- **New checks obey the gate's own honesty rule**: SKIP-with-reason, never a
  hollow PASS.
- **When a lint fires in governance code, chase it before you silence it.** The
  `dead_code` episode is the template: the warning was doing threat modeling.

## What this does NOT imply

- It does not imply the product is feature-complete because it is honest.
  Honesty and completeness are orthogonal; most of citrate-quorum is honestly
  *not built yet*, and says so. The claim is only that what is present is real.
- It does not imply zero placeholders. Placeholders are fine — *named* ones. The
  ban is on placeholders that pretend to be finished, not on admitting something
  is unfinished.

## References

- Rule 1 and the compliance-language rule: `CLAUDE.md`
- The gate: `scripts/check.sh` (ratchets, wiring-contract scans, SKIP-honesty)
- The dead-code episode: `.agentile/docs/journals/2026-07-24T2100_wiring-the-crates-dead-code-as-signal.md`
- Companion essays: `2026-07-24T2101_the-seam-is-the-contract.md`, `2026-07-24T2102_a-prompt-is-not-a-control.md`
