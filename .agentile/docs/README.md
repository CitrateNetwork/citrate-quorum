---
created: 2026-07-24T21:05:00Z
branch: docs/qrm-journals-retros-essays
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
---

# citrate-quorum — development record

The chronology of journals, essays, and sprint retrospectives *is* this
project's development history (Agentile Rule 12). This index is a map, not a
substitute — read the documents.

## Retrospectives (`../sprints/completed/*/RETRO.md`)

Authored at sprint close. Honest reporting: "what didn't work" carries as much
weight as "what worked."

| Sprint | Outcome | Retro |
|--------|---------|-------|
| QRM-S1 — spine + kit extraction + tenancy/RBAC/license + local gate | YES (8/8 WPs) | `../sprints/completed/sprint-qrm-s1/RETRO.md` |
| QRM-S2D — design prototype → real React frontend (12 surfaces + ceremony) | YES | `../sprints/completed/sprint-qrm-s2d/RETRO.md` |
| QRM-S2 — backend logic layer (7 crates) + Tauri command wiring | YES (build-on-devnet half) | `../sprints/completed/sprint-qrm-s2/RETRO.md` |
| QRM-S4 — agent adapter layer (bridge, escalation queue, grants, 5 adapter paths) | YES; 9 bugs found by running the packaged binary | `../sprints/completed/sprint-qrm-s4/RETRO.md` |
| QRM-S5 — meetings + minutes (domain, store, ratification, briefs) | **HALF at close; gate closed 2026-07-26** by QRM-S6 (contracts deployed, wallet + vault + signer wired) | `../sprints/completed/sprint-qrm-s5/RETRO.md` |
| Phase 0 (completion planset) — four dark surfaces made live on 40204 | YES (7 of 14 stubs closed); 2 findings from the packaged drive | `../sprints/completed/sprint-qrm-phase0/RETRO.md` |
| QRM-S3 — Rooms (real MLS group on the relay, humans + agents as peers) | YES (exit gate passes); found a federation-wide 2-member cap and a critical advisory | `../sprints/completed/sprint-qrm-s3/RETRO.md` |

## Decisions (`decisions/`) — owner calls that change what the rules permit

| When | Decision |
|------|----------|
| 2026-07-26 | [G1 no longer blocks QRM-S3](decisions/2026-07-26_g1-superseded-for-rooms.md) — Rooms may be built; the compliance-claim prohibition stands |
| 2026-07-26 | [The NodeDomain contract changes shape](decisions/2026-07-26_nodedomain-contract-change.md) — **proposed**, awaiting ratification: the S2D.3-frozen domain could not be implemented against a real chain |

## Audits (`audits/`) — a claim re-tested rather than re-read

| When | Title | What it found |
|------|-------|---------------|
| 2026-07-25 | [Every "pre-existing" dismissal in this repo's PR history](audits/2026-07-25_preexisting-dismissal-audit.md) | 7 of 9 resolved; CI has **never** run in this repo (100/100 `startup_failure`) and PR #1's "unrelated" attribution was wrong. |
| 2026-07-26 | [Four hpke-rs advisories, one critical, under the federation's MLS stack](audits/2026-07-26_hpke-rs-advisories-in-the-mls-stack.md) | **OPEN, owner decision.** `openmls 0.6` pins `hpke-rs 0.2.0` (RUSTSEC-2026-0071, 9.3 nonce reuse). Affects the deployed relay and client, not just Rooms. Deliberately not silenced. |

## Journals (`journals/`) — short-form session reflections

The durable takeaway from a work session that would not survive in a commit
message.

| When | Title |
|------|-------|
| 2026-07-24 20:50 | [Two apps, one signer](journals/2026-07-24T2050_kit-extraction-two-apps-one-signer.md) — the kit boundary as a compiler-enforced control |
| 2026-07-24 20:55 | [Porting a prototype without inheriting its data](journals/2026-07-24T2055_prototype-to-react-via-the-seam.md) |
| 2026-07-24 21:00 | [The compiler found two governance bugs](journals/2026-07-24T2100_wiring-the-crates-dead-code-as-signal.md) — dead-code warnings as threat modeling |
| 2026-07-25 05:30 | [Running it is the test](journals/2026-07-25T0530_running-it-is-the-test.md) — nine bugs a green gate could not see |
| 2026-07-25 05:35 | [The half-built control](journals/2026-07-25T0535_the-half-built-control.md) — a question recorded and never asked, then an answer silently lost |
| 2026-07-25 05:40 | [A proxy that declines to have an opinion](journals/2026-07-25T0540_a-proxy-that-declines-to-have-an-opinion.md) — building against a spec four days before it broke |
| 2026-07-26 00:15 | [Nine sentences that were not true](journals/2026-07-26T0015_nine-sentences-that-were-not-true.md) — wiring a surface is an audit of what it already claimed |
| 2026-07-26 00:20 | [The idea applied once](journals/2026-07-26T0020_the-idea-applied-once.md) — I fixed this bug last sprint, on one of four paths |
| 2026-07-26 00:25 | [The verification that proved nothing](journals/2026-07-26T0025_the-verification-that-proved-nothing.md) — a green check over a state it created itself |
| 2026-07-26 14:40 | [The panel that only existed because something failed](journals/2026-07-26T1440_the-panel-that-only-existed-because-something-failed.md) — making a read succeed broke a signing flow three surfaces away |
| 2026-07-26 16:30 | [The blocker that was a curl command](journals/2026-07-26T1630_the-blocker-that-was-a-curl-command.md) — a negative result about someone else's system is the most perishable thing in a plan |

## Essays (`essays/`) — conceptual arguments that outlive their sprint

⚠️ Essays are historical context, not governance. The operating rules live in
`CLAUDE.md` and the planset, never here.

| When | Title | The claim it defends |
|------|-------|----------------------|
| 2026-07-24 21:01 | [The seam is the contract](essays/2026-07-24T2101_the-seam-is-the-contract.md) | A boundary you can delete is the only boundary you can trust. |
| 2026-07-24 21:02 | [A prompt is not a control](essays/2026-07-24T2102_a-prompt-is-not-a-control.md) | Governance made of instructions to the agent is documentation, not a control. |
| 2026-07-24 21:03 | [Honest by construction](essays/2026-07-24T2103_honest-by-construction.md) | Honesty enforced by the build is a property of the software; honesty by discipline decays. |
| 2026-07-24 21:04 | [Build on devnet](essays/2026-07-24T2104_build-on-devnet.md) | A dead chain stops deployment, not construction — build what the chain would only confirm. |
| 2026-07-26 00:30 | [Half a gate is not a gate](essays/2026-07-26T0030_half-a-gate-is-not-a-gate.md) | Measure an exit criterion against reality BEFORE building, so the honest report is written down before it is inconvenient. |
| 2026-07-26 15:00 | [A check that cannot fail](essays/2026-07-26T1500_a-check-that-cannot-fail.md) | A verification whose failing branch is unreachable is an anti-control: it manufactures the confidence a real check would have had to earn. |
