---
created: 2026-07-22
branch: main
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
repo: citrate-quorum
tier: T1
---

# citrate-quorum

> **Governance for agents.** A Tauri desktop application where humans in control
> (HIC) and heterogeneous AI agents hold governed meetings, run the Agentile
> loop, and deploy plain-English governance protocols as CREATE2 smart contracts
> on Citrate — with every agent action gated, traced, signed, and anchored.

**Tier:** T1 — money, keys, identity, governance, binary distribution. Full audit
before release.

## Honest current status (2026-08-01)

**QRM-S0 through S7 are merged.** The previous version of this section said "this
repo is a scaffold, no application code exists yet" and was left untouched from
2026-07-23 through seven sprints — understating the repo rather than overstating it,
but wrong either way, and wrong in the one place a newcomer reads first.

Verified by running the gate, not by reading sprint files: **384 Rust tests, 51
frontend tests, `scripts/check.sh` 21 pass / 0 fail / 1 skip.**

| Real and exercised | Not yet |
|---|---|
| 14 surfaces (Rooms, Meetings, Governance, Agents, Ledger, Wallet, Node, Calendar, Repos, Journal, Settings, Dashboard, …) | **No release. `version` is `0.0.0`; no tag, no signed installer** (QRM-S9, @rule8) |
| 9 backend crates: tenancy, license, rbac, session, clearance, audit, policy, meetings, adapter | **Auto-updater inert by design** until S9 |
| The shared **SignatureCeremony** from `citrate-core-kit` | `citrate-core-kit` is still a **local path dep**, not a pinned git dep (owner infra: deploy key) |
| The **S7 authoring pipeline**: INGEST → INTERVIEW → DRAFT SPEC → COMPILE → SIMULATE → CEREMONY → DEPLOY → BIND | Template registration needs a real `auditCID`; the registry rejects an empty one by design |
| Governance contracts live on 40204 and read by the app; the vendored address book is byte-identical to canonical | Hosted CI — Actions is down org-wide, so `scripts/check.sh` is the gate |

**Anchoring is back.** The 2026-07-27 chain handoff listed Quorum anchoring as broken
and told this team to expect two failing address tests. Both `AnchorRegistry` and
`MeetingRegistry` were redeployed later that day, the vendored book matches canonical,
and those tests pass. That handoff is superseded — read its header box, not its §2.

Nothing fabricates data (Rule 1).

## Canonical truth

The design lives in the **federation planset**, not here:

`citrate-federation/.agentile/planset/2026-07-22-citrate-quorum/`

| Doc | What |
|---|---|
| `00_OVERVIEW.md` | vision, decisions D1–D8, reuse map |
| `01_RESEARCH_BASELINE.md` | what already exists across the federation, file-cited |
| `02_ARCHITECTURE.md` | process topology, rooms, agent adapters, authoring pipeline |
| `03_GOVERNANCE_CONTRACTS.md` | factory, policy binding, vote allowance, sortition, meetings |
| `04_HIC_MODEL.md` | **Human In Control** — normative, federation-wide |
| `05_SCOPE_AND_SPRINTS.md` | QRM-S0…S9, risks R1–R14 |
| `06_COMPLIANCE_SOC2.md` | control mapping, evidence pack |
| `07_OPEN_QUESTIONS.md` | the 20 questions (answered) |
| `08_FEATURES_YOU_DIDNT_ASK_FOR.md` | gaps ranked for triage |
| **`09_DECISIONS_LOCKED.md`** | **the answers + deltas — authoritative** |
| `PROMPT_LOG.md` | append-only ledger of the human prompts driving this build |

Read `09` before `00`–`08`; where they conflict, `09` wins.

## Relationship to citrate-core

Quorum is **not a fork**. The safety-critical code — signature ceremony, key
custody, OIDC, the sidecar supervisor — is extracted into a shared
`citrate-core-kit` that both citrate-core and citrate-quorum depend on. Changes
to that code land **upstream in the kit first**, never in a divergent copy.
See `.agentile/sprints/active/sprint-qrm-s1/SCOPE.md`.

## Non-negotiables

See `CLAUDE.md`. The three that matter most:

1. **Every signature goes through the SignatureCeremony.** No agent, sidecar, or
   remote service ever holds a key or produces a signature.
2. **No mocks.** Every surface states what is real.
3. **The HIC model is enforced in code, not in prompts.** A prompt asking an
   agent to seek permission is not a control and is never counted as one.

## License

BUSL-1.1 — see `LICENSE`.
