---
created: 2026-07-23
branch: main
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: pending — opens when the design prototype PR lands
sprint: QRM-S2D
repo: citrate-quorum
tier: T1
---

# QRM-S2D — Prototype integration, wiring and QA

> **Sprint goal.** Turn the design team's React prototype into this app's real
> frontend **without a redesign round and without re-implementing its data
> layer** — then freeze the bridge contract our Rust will be written against.

**Canonical detail:** `citrate-federation/.agentile/planset/2026-07-22-citrate-quorum/`
— `10_DESIGN_BRIEF.md` (what was asked for, incl. the §9 acceptance checklist)
and `05_SCOPE_AND_SPRINTS.md` §4a (the work-package table). This file is the
repo-local pointer plus the intake procedure.

## Why this is its own sprint

Design-to-production handoff fails the same way in most projects: the prototype's
data assumptions get quietly re-implemented instead of wired, and six weeks later
nobody can say which screens are real. The objective check against that is
**S2D.4** — delete `src/bridge/sim/` and the app must still compile and show
honest empty/unavailable states everywhere. If it doesn't, the prototype's data
leaked into the surfaces and the integration is not done.

## Work packages

`S2D.1` intake audit · `S2D.2` token + shell reconciliation ·
**`S2D.3` bridge contract freeze** · `S2D.4` sim honesty pass ·
`S2D.5` per-surface wiring tickets · `S2D.6` QA pass ·
`S2D.7` ceremony conformance · `S2D.8` demo dry-run.

Full acceptance criteria per WP in the planset §4a. S2D.1→S2D.3 are strictly
ordered; the rest parallelize.

## Intake procedure (S2D.1) — run before merging anything

1. Check out the design PR branch; do **not** merge it yet.
2. Run `npm ci && npm run typecheck && npm test && npm run build`. Record results.
3. Walk the §9 acceptance checklist in `10_DESIGN_BRIEF.md` box by box.
4. Run the three source-scan tests: no `invoke` in surfaces · no prototype data
   outside `bridge/sim/` · no hard-coded hex outside the vendored token files.
5. `diff` the three vendored style files against citrate-core's. Any delta is
   either reverted or folded **upstream into citrate-core** — never forked here.
6. Write `.agentile/sprints/active/sprint-qrm-s2d/INTAKE_REPORT.md`: what passes,
   what fails, what we accept as-is and why. Every failing box gets a ticket or a
   written waiver signed by the owner.
7. Only then merge.

## The one thing that must not slip

**S2D.3 — the bridge contract freeze.** After it, `bridge/domains.ts` and
`bridge/types.ts` are the interface every Rust command is written against. A
change after the freeze costs backend rework, so it requires an owner decision,
not a PR comment. Resolve every `// TODO(wire):` from the prototype *before*
tagging the freeze.

## Exit gate

Prototype merged and building in-repo · bridge contract tagged and frozen ·
`bridge/sim/` deletable without breaking the build · 12 per-surface wiring tickets
open · QA report with per-state evidence committed · ceremony conformance signed
off · the four demo beats walked end-to-end and rated.

## Dependencies

- **Opens on:** the design prototype PR (`design/prototype-v1`).
- **Blocks:** QRM-S3 (rooms cannot be wired against an unfrozen contract).
- **Parallel with:** QRM-S2 (identity → clearance) and the S1 kit extraction.
