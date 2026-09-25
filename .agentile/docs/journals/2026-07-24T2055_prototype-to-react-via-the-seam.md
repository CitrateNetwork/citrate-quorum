---
created: 2026-07-24T20:55:00Z
branch: docs/qrm-journals-retros-essays
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# Porting a prototype without inheriting its data

> The prototype's job was to make the product *look* real; the seam's job was
> to make sure "looks real" never became "pretends to be real."

## Context

QRM-S2D. The design team delivered a working prototype through the Claude
Design MCP — `CitrateQuorum.dc.html`, a 191 KB canvas DSL with sixteen
sections, plus a `quorum-sim.js` that scripted all the data. The task was to
turn it into this app's real React frontend. The trap, in a T1 governance app,
is obvious: prototypes are made of convincing fake data, and convincing fake
data is exactly what Rule 1 forbids in the shipped product.

## What happened

We did not port the prototype into the surfaces. We ported it into a *seam*.
Every scripted value went into `src/bridge/sim/` and nowhere else. The surfaces
were rewritten to import only `src/bridge/domains.ts` — twelve typed interfaces,
one per domain, each method returning a `Promise` or a `Subscribe<E>` stream. A
runtime switch (`bridge/mode.ts`) picks the `sim` adapter in dev/web and the
`tauri` adapter inside the packaged app; the `tauri` adapter throws `Unavailable`
for anything not yet wired to Rust.

The prototype had annotated its scripted data with `// TYPE:` comments. Those
lifted almost verbatim into `src/bridge/types.ts` — the design team had, in
effect, written the interface our Rust would later implement, without calling it
that.

We ported the Ceremony overlay first, on purpose: it is the single HIC signing
gate, the highest-risk surface, so it earned the first and most careful pass.
Every action that signs routes through `useCeremony().request(intent)`. Then the
demo-beat surfaces (Governance pipeline, Rooms, Meetings, Ledger), then the rest.

The acceptance test for the whole approach was blunt: `rm -rf src/bridge/sim`
and the app must still compile. It did — one dangling error at
`bridge/index.ts`, zero surface breaks. That is the mechanical proof that the
prototype data is not load-bearing.

Two things bit us. First, the initial QA harness set the "entered" flag *after*
page load and navigated by hash without remounting, so every surface screenshot
was secretly the onboarding screen — a green QA that tested nothing, fixed with
`evaluateOnNewDocument`. Second, brand SVGs referenced by literal `/src/...`
paths worked in Vite dev and 404'd in the packaged build; only a headless run of
the *built* artifact caught it (PR #4).

## What I learned

A prototype is safe to inherit only through a boundary that can be deleted. The
`rm -rf src/bridge/sim` test is the whole discipline in one command: if deleting
the fake data leaves a compiling app, then the fake data was never structural,
and no surface can be silently depending on it. Design-time realism and run-time
honesty stop being in tension the moment the realism is quarantined behind a
typed seam.

## What I'd do differently

QA the *packaged artifact* from the first cycle, not the dev server. Both bugs
that mattered this sprint — the SVG 404 and the localStorage-timing false green
— were invisible in `npm run dev` and only appeared in the built app under a
real browser. "Works in dev" earned zero trust this sprint and should start at
zero next sprint.

## Open questions

- The frontend still has zero component tests; it is QA-verified, not unit-
  verified. Which surfaces deserve component tests first — Ceremony and the
  escalation toast, presumably, since they encode HIC behavior?
- Should the `Unavailable`-throwing tauri stubs render a consistent, branded
  "not wired yet" panel so the honest state is also a *good* state visually?

## Pointers

- Sprint: `.agentile/sprints/completed/sprint-qrm-s2d/RETRO.md`
- Commits: `204cf0d` (seam + shell + Dashboard), `7cd89e3` (12 surfaces + ceremony, PR #2), `43d1b9b` (packaged-build SVG fix, PR #4)
- Related essay: `.agentile/docs/essays/2026-07-24T2101_the-seam-is-the-contract.md`
