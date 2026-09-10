---
created: 2026-07-24T20:47:00Z
branch: docs/qrm-journals-retros-essays
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# Sprint QRM-S2D — Retrospective

> Turning a 191 KB design-canvas prototype into a real React application
> without letting a single line of prototype data leak into the shipped app.

## Outcome

| Field | Value |
|-------|-------|
| **Goal achieved?** | YES |
| **WPs planned / closed** | design intake + bridge seam (S2D.3) + 12 surfaces + ceremony + onboarding + shell + QA — all closed |
| **Carry-forward WPs** | none for S2D; the *live-data* half of each surface belongs to its backend sprint |
| **Closing branch** | `main` at `43d1b9b` (PR #4, the packaged-build SVG fix found in QA) |

## Metrics delta

| Axis | Start | End | Δ |
|------|-------|-----|----|
| Rust tests | ~40 | ~40 | 0 (this sprint was frontend) |
| Frontend surfaces ported | 1 (skeleton) | 12 + ceremony + onboarding + shell | +14 |
| Frontend vitest tests | 0 | 0 | 0 — **verified by headless QA, not unit tests** (see below) |
| CI tripwires (wiring-contract scans) | 5 | 8 | +3 (no-invoke-in-surfaces, no-sim-data-outside-bridge, no-hardcoded-hex) |

**The frontend test count is honestly zero.** The ported UI was verified by
running the packaged app headless (chromium) and walking all four demo beats,
not by component unit tests. That is a real gap, named here rather than hidden:
component tests are owed and belong to a hardening pass. What *is* enforced
mechanically is the wiring contract — three new scans that fail the build if a
surface reaches around the bridge.

## What worked

- **The bridge seam as a hard boundary.** Every byte of prototype data lives in
  `src/bridge/sim/`. Surfaces import only `src/bridge/domains.ts` (typed
  interfaces). Flipping a domain from scripted data to live Rust changes *one
  adapter* and *zero surfaces*. The S2D.4 acceptance test — `rm -rf
  src/bridge/sim` must leave a compiling app — passed: one error at
  `bridge/index.ts`, no surface breaks.
- **Porting the Ceremony first.** The SignatureCeremony overlay was the highest-
  risk surface because it is the one HITL gate, so it went first. Everything
  that signs (deploy 2-of-3, ratify, grant, revoke, send > 150 → HIC-1, stake)
  routes through `useCeremony().request(intent)` — proven end-to-end on sim
  before the lower-risk surfaces were touched.
- **QA caught a real packaged-build bug the dev server hid.** Brand SVGs
  referenced as literal `/src/assets/...` paths resolved in Vite dev and 404'd
  in `dist`. Only a headless run of the *built* app surfaced it (PR #4). The
  lesson: "works in `npm run dev`" is not "works packaged."

## What didn't work

- **The first QA harness screenshotted the wrong screens.** A localStorage
  bypass ran *after* page load and a hash-only `goto` didn't remount the app, so
  every "surface" screenshot was actually the onboarding screen. The QA looked
  green while testing nothing. Fixed with `evaluateOnNewDocument` to set the
  entered-flag before load. Cost: one full QA cycle of false confidence.
- **Two registers (Charter light / Instrument dark) added CSS-surface-area we
  under-budgeted.** Getting `data-register` theming right across 12 surfaces
  took longer than the port of the surfaces themselves.

## What surprised us

- **The prototype's own `// TYPE:` annotations became the TypeScript contract
  almost verbatim.** The design canvas carried type annotations next to its
  scripted data; those lifted cleanly into `src/bridge/types.ts`. The design
  team had, without framing it that way, written the interface our Rust is now
  implemented against.
- **A packaged Tauri build is a genuinely different target from the Vite dev
  server** — not just "the same thing, bundled." Asset resolution, CSP, and
  path handling all differ. This reframed QA from "click through it" to "click
  through the *artifact you ship*."

## Carry-forward

| Item | Where it goes | Why deferred |
|------|---------------|--------------|
| Live data behind each surface | The surface's backend sprint (S2/S3/S4/S6/S8) | Each needs its own chain/relay/OAuth source |
| Frontend component tests | A hardening pass (toward S9) | The port was QA-verified; unit coverage is owed |

## Decisions ratified mid-sprint

- **`src/bridge/sim/` deletion must leave a compiling app (S2D.4).** Promoted
  from a nice-to-have to a CI-enforced acceptance property. It is the mechanical
  guarantee that the prototype cannot become load-bearing.
- **The tauri adapter throws `Unavailable` for every unwired domain** rather
  than returning empty or fabricated data. An unbuilt surface in the packaged
  app shows an honest error (Rule 1).

## Action items for next sprint

- [x] Wire the first live domain (Ledger) to real Rust — done in the S2 backend-wiring PR (#8).
- [ ] Add component tests for the Ceremony overlay and the escalation toast (hardening).

## Notes

This sprint is the reason the backend sprints could move fast: the contract the
Rust is written against (`bridge/domains.ts`) was frozen here, so wiring a crate
became "implement an interface" rather than "design an interface." Related
journal: `.agentile/docs/journals/2026-07-24T2055_prototype-to-react-via-the-seam.md`.
Related essay: `.agentile/docs/essays/2026-07-24T2101_the-seam-is-the-contract.md`.
