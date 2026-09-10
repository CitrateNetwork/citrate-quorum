---
created: 2026-07-24T20:49:00Z
branch: docs/qrm-journals-retros-essays
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: complete
sprint: QRM-S2
---

# Sprint QRM-S2 — Backend logic layer + command wiring

> **Sprint goal.** Make the HIC governance model *executable* client-side: the
> identity→clearance resolver, the policy verdict engine, the tamper-evident
> audit spine, and the Tauri command surface that composes them into the live
> governed-action loop — all build-on-devnet, no live chain required.

This folder documents work that was executed against the federation planset's
S2/S4/S6 work-package tables
(`citrate-federation/.agentile/planset/2026-07-22-citrate-quorum/`, not in this
checkout). It is recorded as a single completed sprint because the crates
compose into one pipeline and were built and reviewed together.

## Work packages (all closed)

| WP | What | PR | Tests |
|----|------|----|-------|
| S2 · session | `quorum-session::resolve_effective_grant` — fail-closed least-of-ceilings (tier cap ∩ on-chain clearance ∩ tenant max); tier only CAPS; ITAR needs definite non-FN | #3 | 13 (incl. WP6 adversarial) |
| S2 · clearance | `quorum-clearance` — `ClassificationRegistry.getClearance` + `TenantHierarchy.getNode` ABI decoders over the kit's injectable `eth_call` transport → `EffectiveGrant`, fail-closed | #5 | 11 (vs mock RPC) |
| S4 · audit | `quorum-audit` — `DecisionRecord` + per-tenant BLAKE3 `HashChain` (tamper-evident) + Merkle root for `AnchorRegistry`; `ungoverned` is first-class | #6 | 11 |
| S6 · policy | `quorum-policy` — `CapabilityGrant` (CG-1..4) + `evaluate()` = `PolicyBinding.check` (Allow / RequireApproval / Ungoverned + reason codes) + `VoteAllowance` (VA-1..4) | #7 | 14 |
| S2 · wiring | `src-tauri/src/backend.rs` — `QuorumBackend` managed state + 13 Tauri commands; frontend `bridge/tauri` Ledger domain reads the live chain | #8 | 9 (+ app lib) |

## Acceptance (met)

- The governed-action loop runs with **no chain**: `policy.evaluate →
  DecisionRecord → per-tenant HashChain`, callable via `action_evaluate_and_record`.
- Grants are tenant-isolated (`(tenant, agent)` keyed); a test proves cross-
  tenant non-authorization (Rule 6).
- `ungoverned` actions are recorded and counted, never dropped (Rule 5).
- The Ledger surface reads real hash-chained records; an empty chain returns an
  empty ledger (Rule 1). Unwired domains throw `Unavailable`.
- Local gate `scripts/check.sh`: 18 pass / 0 fail / 0 skip.

## Explicitly carried forward (gated on the reroll / live IdP)

- `session_resolve` live `ClearanceReader` path (currently fails closed to Public).
- Revocation re-check timer.
- Durable persistence of the per-tenant HashChains.
- The on-chain governance Solidity contracts (citrate-chain, Foundry).
- Rooms / agents / calendar / repos domain wiring (S3/S4/S8).

See `RETRO.md` in this folder.
