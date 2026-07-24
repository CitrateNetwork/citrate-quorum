---
created: 2026-07-24
branch: main
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
---

# Imported design prototype — provenance

Source (canonical, editable): Claude Design project **"Working prototype requirements"**
`https://claude.ai/design/p/012229c8-2830-4ad4-880e-3e202bb53bcd`

Imported 2026-07-24 for the QRM-S2D integration. This `design/` folder mirrors the
prototype so it can be rendered/referenced locally; the **implementation** lives in
`src/` (bridge seam + surfaces ported from these files).

| File | Role |
|---|---|
| `CitrateQuorum.dc.html` | the prototype UI (Claude Design canvas format) |
| `quorum-sim.js` | the bridge seam — all sim data + streams (§6 of the design brief) |
| `DESIGN_NOTES.md` | the design team's decisions, deviations, TODO(wire), demo beats |
| `CLAUDE.md` | project instruction (loader ring geometry) |

Design-system CSS, tokens, and brand SVGs were already vendored into `src/styles`
and `src/assets/brand` in WP-S1.3 (they are the shared citrate-core design system).

The prototype's own `uploads/` referenced the **citrate-core** planset as prior art;
the canonical spec for THIS app is the quorum planset
(`citrate-federation/.agentile/planset/2026-07-22-citrate-quorum/`).
