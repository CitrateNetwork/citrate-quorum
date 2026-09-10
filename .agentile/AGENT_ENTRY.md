---
created: 2026-07-22
branch: main
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
repo: citrate-quorum
tier: T1
---

# Agent Entry — citrate-quorum

> Start here whenever you (human or AI) are working in **citrate-quorum**.

## What this repo is

**Governance for agents.** A Tauri desktop application — the "Zoom for agents" —
where humans in control (HIC) and heterogeneous AI agents hold governed meetings,
run the Agentile loop, and deploy plain-English governance protocols as CREATE2
smart contracts on Citrate. Every agent action is policy-gated before execution,
signed through a human ceremony where required, recorded, hash-chained, and
anchored on chain.

Repo tier: **T1** — money, keys, identity, governance, binary distribution.

## What to read, in order

1. **This file** (you're here).
2. **`README.md`** — honest current status. The repo is a scaffold; no app code yet.
3. **`CLAUDE.md`** — this repo's hard rules. Rules 3, 4, 5 and 6 are the ones
   people get wrong.
4. **The federation planset (canonical truth)** —
   `citrate-federation/.agentile/planset/2026-07-22-citrate-quorum/`.
   Read **`09_DECISIONS_LOCKED.md` first** — it supersedes conflicting text
   anywhere else in the planset. Then `00_OVERVIEW.md`, then the rest.
5. **`04_HIC_MODEL.md`** in that planset — the HIC model is normative for the
   whole federation, not just this app. If you are about to write anything that
   lets an agent act, read it first.
6. **The active sprint** — `.agentile/sprints/active/sprint-qrm-s1/SCOPE.md`.
7. **Federation control plane** — `citrate-federation/agentile/AGENT_ENTRY.md`
   and `rules/CORE_RULES.md`.

## Core invariants

- **I-1 (custody):** citrate-quorum owns the keystore and every signature. No
  agent, sidecar, daemon, or remote service ever holds a user key or signs.
- **I-2 (ceremony):** the SignatureCeremony is the only signing path, shared with
  citrate-core via `citrate-core-kit`. A source-scan test asserts there is no
  second one.
- **I-3 (gated agents):** every agent tool invocation passes the policy gate in
  the adapter, before execution, and produces a decision record. Prompt-level
  instructions are not controls.
- **I-4 (HIC per action):** every record carries principal, grant, and HIC level.
  No live grant → recorded `ungoverned` and alerted.
- **I-5 (honesty, Rule 1):** every surface states what is real; unbuilt surfaces
  ship named honestly or not at all.
- **I-6 (tenancy):** no global singletons for tenant-scoped state; every domain
  seam takes a TenantContext.

## Where the code will live

- `src-tauri/` — Rust backend, domain seams, adapters, sidecar supervision.
- `src/` — React shell, surfaces, agent harness.
- Safety-critical shared code — **not here.** `citrate-core-kit` (ceremony,
  custody, oidc, supervisor, rpc). Changes land upstream in the kit first.

## Before you write code

Ask: does this let an agent do something? If yes, the policy gate, the grant
check, and the decision record are part of the same PR — not a follow-up.
