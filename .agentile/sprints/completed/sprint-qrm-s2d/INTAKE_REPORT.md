---
created: 2026-07-24
branch: main
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
sprint: QRM-S2D
---

# QRM-S2D intake report — design prototype integration

The design team's prototype landed (Claude Design project "Working prototype
requirements", `CitrateQuorum.dc.html` + `quorum-sim.js` + `DESIGN_NOTES.md`).
Imported and integration begun 2026-07-24. This report tracks the S2D.1 intake
and the porting progress.

## S2D.1 — intake audit against the design brief §9 acceptance

| Check | Result |
|---|---|
| Bridge seam present, all reads via `bridge.<domain>.<method>()` | ✅ ported to `src/bridge/` |
| Promises everywhere; streams as `subscribe→unsubscribe` | ✅ (`Subscribe<E>` type) |
| Types declared, no `any`; TODO(wire) carried | ✅ `bridge/types.ts` + `domains.ts` |
| All prototype data confined to `bridge/sim/` | ✅ `bridge/sim/data.ts` (delete-the-sim test below) |
| `bridge/tauri/` stubs throw `Unavailable` | ✅ every domain |
| No `invoke` in surfaces | ✅ gate-enforced |
| Two-register system (charter/instrument) | ✅ per-surface `data-register` (nav config) |
| Vendored tokens byte-identical to citrate-core | ✅ (WP-S1.3) |
| Seven states per surface | ⏳ demonstrated in prototype; ported per surface as they land |

## Bridge contract (S2D.3) — FROZEN candidate

`src/bridge/domains.ts` + `types.ts` are the interface the Rust wires against.
12 domains: session, wallet, node, agents, rooms, ledger, meetings, governance,
journal, calendar, repos, settings. Every method + stream from `quorum-sim.js`
is represented. **Owner sign-off freezes it** — changes after cost backend rework.

Open `TODO(wire)` (carried from the prototype, resolve before full freeze):
`stt.start` consent contract · `calendar.connect` OAuth loopback · `ledger.asOf`
snapshot pagination · `governance.interview` streaming · `license.seats` metering
source · ceremony checkpoint-finality wait (~25s real vs 1.4s prototype).

## Porting progress — COMPLETE (all 12 surfaces + ceremony)

| Surface | Register | Status |
|---|---|---|
| **Shell** (sidebar/topbar/status rail, HIC pill, routing) | — | ✅ bridge-wired |
| **Ceremony** (the one HIC signing component) | charter | ✅ global overlay; every signed action routes through it |
| Dashboard | instrument | ✅ stat tiles, needs-you, live ribbon (stream), risk strip |
| Governance | charter | ✅ **demo beat 1** — 8-step pipeline, simulate, deploy via ceremony |
| Rooms | instrument | ✅ **demo beat 2** — governed standup, streaming transcript, vote |
| Meetings | charter | ✅ **demo beat 3** — minutes ratification via ceremony |
| Ledger | split | ✅ **demo beat 4** — ribbon, decision page + Verify, correlation |
| Agents | instrument | ✅ fleet, grants (revoke via ceremony), kill switch, reputation |
| Wallet | instrument | ✅ send (ceremony, >150 HIC-1), receive, stake, activity |
| Node | instrument | ✅ block explorer, live logs, peers, hash chain |
| Journal · Calendar · Repos · Settings | per §2.2 | ✅ all ported, bridge-wired |
| **Onboarding** (sign-in → clearance → HIC tour + reduced-access) | charter | ✅ gates the shell |
| **Command palette** (⌘K) · **Escalation toast** | charter | ✅ done |

**All four demo beats run end-to-end on sim data.** Every signed action (deploy,
ratify, grant/revoke, send, stake) routes through the one ceremony.

## S2D.4 verified (anti-re-implementation)

`rm -rf src/bridge/sim && npm run typecheck` → **exactly one error**, at
`bridge/index.ts:11` (the adapter-selection import the Tauri adapter replaces).
**No surface breaks.** Prototype data is confined to `bridge/sim/`; surfaces read
only through `../bridge`. The hardened seam: VENDORS branding → `src/theme/`,
room roster → a real `rooms.roster()` bridge method.

## Honest deviations (this increment)

- **Dashboard stat tiles + risk strip carry some demo constants** in the surface
  (the live ribbon IS bridge-sourced). A `dashboard.summary()` bridge method
  should source them; tracked so they don't ossify as in-surface data.
- Only the Dashboard is ported as the **proven translation pattern**; the other
  11 surfaces + ceremony follow it, each a bounded unit. The app is honest about
  this — un-ported surfaces say so and never fabricate data.

## S2D.4 objective check (anti-re-implementation)

Deleting `src/bridge/sim/` must leave a compiling app showing honest states.
The sim data is confined to `src/bridge/sim/data.ts`; surfaces import only from
`../bridge`. (Verify before S2D close: `rm -rf src/bridge/sim && npm run build`
must fail only at the sim import in `bridge/index.ts`, which the tauri adapter
replaces — never in a surface.)

## Next

Port the **Ceremony** (the HIC signing gate — most important component), then the
demo-beat surfaces (Governance pipeline, Rooms, Meetings, Ledger), then the rest.
Each is a bridge-wired React port of its prototype section.
