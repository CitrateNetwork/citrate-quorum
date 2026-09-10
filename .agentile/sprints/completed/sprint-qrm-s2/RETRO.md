---
created: 2026-07-24T20:49:00Z
branch: docs/qrm-journals-retros-essays
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# Sprint QRM-S2 — Retrospective (backend logic + command wiring)

> Seven pure-logic crates and the Tauri command surface that makes the
> governed-action loop real — built entirely while the chain was wedged, by
> refusing to let "no chain" mean "no progress."

## Scope note

This retro covers the backend logic layer and its wiring: the crates delivered
across PRs #3/#5/#6/#7 (session, clearance, audit, policy — mapped to the
federation planset's S2/S4/S6 work-package table) and the command-surface
wiring in PR #8. They were sequenced together because they compose into one
pipeline; splitting them across three "sprints" would have been bookkeeping
theatre. See `SCOPE.md` in this folder.

## Outcome

| Field | Value |
|-------|-------|
| **Goal achieved?** | YES (the client-side / build-on-devnet half). Live chain reads + on-chain anchoring are explicitly carried forward. |
| **WPs planned / closed** | session resolver, on-chain clearance decoders, audit spine, policy engine, command wiring — all closed |
| **Carry-forward WPs** | `session_resolve` live chain-read variant; revocation re-check timer; the Solidity contracts; all downstream domain wiring |
| **Closing branch** | `main` at `6a8a38c` (PR #8) |

## Metrics delta

| Axis | Start | End | Δ |
|------|-------|-----|----|
| Rust tests (workspace) | ~40 | **79** | +~39 |
| Backend crates | 3 (tenancy/license/rbac) | 7 (+ session/clearance/audit/policy) | +4 |
| Live Tauri commands (quorum-specific) | 0 | 13 | +13 |
| Live frontend domains | 0 | 1 (Ledger) | +1 |
| CI tripwires | 8 | 8 | 0 |

Every crate was born tested. The +39 is not backfilled coverage; it is the
tests that were written alongside the logic, including the adversarial suites
(session WP6, policy CG/VA invariants, audit tamper-evidence).

## What worked

- **Build-on-devnet as a discipline, not an excuse.** The chain is wedged
  ([[project-srp-state-root-purity]]) and there is no live IdP. Rather than
  block, we built every piece that a chain would only *confirm*: the
  least-of-ceilings grant resolver, the policy verdict engine, the BLAKE3
  hash-chain audit spine, the Merkle anchor. The governed-action loop
  (`policy.evaluate → DecisionRecord → per-tenant HashChain`) runs today, with
  no chain, and anchoring its Merkle root is the only remaining last mile.
- **Clockless, injectable-time pure logic.** Every crate takes `now_ms` as a
  parameter and does no I/O. That is why 79 tests run in milliseconds and why
  the same code is exhaustively testable and trivially wireable into Tauri
  commands that supply `SystemTime` at the boundary.
- **Dead-code warnings were treated as design signals.** Wiring the crates
  surfaced two `#[warn(dead_code)]` hits. Instead of `#[allow]`-ing them, we
  followed each to a real gap: an unused `tenant` field meant grants were keyed
  by agent alone (a cross-tenant authorization leak, Rule 6), and an unused
  `ungoverned_count` method meant the Ledger had no honest source for its
  headline gap number (Rule 5). Both are now fixed with tests. The compiler
  found two governance bugs.

## What didn't work

- **`session_resolve`'s live variant is still a stub that fails closed.**
  Honest and safe — an unreadable chain collapses to Public / non-FN — but it
  means the enterprise clearance axis is not yet *proving* anything against a
  real `ClassificationRegistry`. Carried forward, gated on the reroll.
- **A `'static` lifetime on the Tauri `State` alias broke the command macro.**
  A small self-inflicted detour: `State<'static, …>` does not satisfy the
  generated command's borrow; the fix is the elided per-invocation lifetime.
  Twenty minutes lost to a macro error message that pointed at the call site,
  not the alias.

## What surprised us

- **The frontend `Decision` type and the Rust `LedgerRow` DTO matched field-for-
  field with zero adaptation.** Because S2D froze the bridge contract from the
  prototype's own type annotations, the Rust DTO written months-of-context later
  serialized straight into the shape the surface already renders. The seam paid
  off exactly as designed.
- **GitHub's write API went down mid-merge.** Reads worked; PR creation (both
  GraphQL and REST) returned empty bodies for ~30 minutes. The branch was
  pushed and locally gated the whole time, so the outage blocked *paperwork*,
  not *work* — a background retry loop opened PR #8 the moment the API
  recovered. A useful reminder that the local gate, not CI, is what actually
  tells us the work is sound.

## Carry-forward

| Item | Where it goes | Why deferred |
|------|---------------|--------------|
| `session_resolve` live chain-read | Post-reroll wiring | Needs `RpcClient::citrate` + the frozen address book against a live chain |
| Revocation re-check timer | Same | Needs a live grant source to re-check against |
| Governance Solidity contracts | citrate-chain (Foundry), S6 on-chain | Different repo, different toolchain, gated on chain |
| Rooms / agents / calendar / repos domain wiring | S3/S4/S8 | Each gated on comms relay / adapters / OAuth |

## Decisions ratified mid-sprint

- **Grants are keyed by `(tenant, agent)`, never agent alone.** Multi-tenant
  isolation is structural, with a test that a grant in tenant A does not govern
  the same agent in tenant B.
- **In-memory backend state for now.** Durable persistence (crash-atomic, like
  the gateway money store) is a named later WP; the pipeline's correctness does
  not depend on where the chain is stored, so we did not block on it.

## Action items for next sprint

- [ ] Wire `session_resolve`'s live `ClearanceReader` path once the reroll lands a readable chain.
- [ ] Add the revocation re-check timer against the live grant store.
- [ ] Persist the per-tenant HashChains durably before any real evidence is recorded.

## Notes

This sprint is where the abstract HIC model became executable. The single most
important artifact is `src-tauri/src/backend.rs`: read it top-to-bottom and you
can watch a proposed agent action become a verdict, become a hash-chained
record, become a new chain head — with no chain in the loop. Related journal:
`.agentile/docs/journals/2026-07-24T2100_wiring-the-crates-dead-code-as-signal.md`.
Related essays: `2026-07-24T2103_honest-by-construction.md`,
`2026-07-24T2104_build-on-devnet.md`.
