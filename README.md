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

## Honest current status (2026-07-22)

**This repo is a scaffold. No application code exists yet.** What is here:

| Present | Not present |
|---|---|
| Repo governance (this file, `CLAUDE.md`, `AUDIT_TIER.md`) | Any Rust or TypeScript source |
| Agent entry point (`.agentile/AGENT_ENTRY.md`) | The Tauri app |
| The QRM-S1 sprint scope | `citrate-core-kit` (extraction is S1's main work package) |
| Docs CI (frontmatter + link checks) | Code CI (lands with the code in S1) |

Nothing in this repo currently talks to a chain, a relay, an identity provider,
or a model. When it does, this table changes in the same PR. (Rule 1.)

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
