---
created: 2026-07-27T02:00:00Z
branch: feat/qrm-s6-governance-contracts
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
sprint: QRM-S6
---

# Sprint QRM-S6 — Governance contracts (Phase 2, part 1 of 2)

> **Sprint goal.** The contracts that make a plain-English governance protocol a
> deployed, template-bounded, spec-bound object on chain — and the runtime hook
> that makes an agent unable to ignore it.

Phase 2 of `.agentile/planset/COMPLETION_PLAN.md` is **two** planset sprints:
**QRM-S6** (these contracts) and **QRM-S7** (the authoring pipeline that drives
them). This SCOPE covers S6. S7 gets its own when S6's gate is met.

Normative spec: `03_GOVERNANCE_CONTRACTS.md` in the federation planset
(`plan/quorum-s0`, not in this checkout — read with `git show`).

Planset exit gate for S6: *"100% invariant coverage; Slither/semgrep clean;
audit report in-repo."*

## Measured against reality before writing this

| What the plan assumes | What is actually true (checked 2026-07-27) |
|---|---|
| The `rbac/` six + `cit_agent/` exist to build on | True. `TenantHierarchy` (root **seeded** 2026-07-26), `ClassificationRegistry`, `RoleEscalation`, `MultiSigEnvelope`, `AgentDecisionRegistryV2`, `ContradictionLedger`, `AgentSBT`, `CapsuleRegistry` are deployed and booked. |
| `MeetingRegistry` §6 is net-new | **Already written, deployed and in use** (`0x7cef67f4…`) — QRM-S5/S6 did it. §6 of the planset is DONE; it is the shape the rest should follow. |
| The other six contracts exist as specs only | True. `GovernanceTemplateRegistry`, `GovernanceProtocolFactory`, `PolicyBinding`, `VoteAllowance`, `Sortition`, `CapabilityGrant` — **none written**. |
| "each contract ships with a TLA+ spec … the ratchet enforces it" (§9) | **Mostly true — corrected 2026-07-27, see below.** citrate-chain has a real TLA+ practice: **399 `.tla` files**, 48 of them a CI-runnable subset under `specs/tla/` across 6 domains, **7 of those under `specs/tla/contracts/`**, with a TLC runner (`specs/tla/run_all.sh`, vendored `tla2tools.jar`, Java 8 present). What is *not* true is the BFR-02 half: `ClassificationLadder.tla` and the other specs the `rbac/` contracts cite in their NatSpec **do not exist in this repo** — those citations point at a canonical collection (`.agentile/formal/specs/`) that is not checked in anywhere I can find. |
| Quorum's Governance surface consumes them | Its five bridge methods (`protocols/clauses/simulate/ingest/interview`) are all `Unavailable` stubs — QRM-S7's job, not S6's. |

**Consequence of row 4, stated up front:** the S6 gate says "100% invariant
coverage". This sprint reads it as **every invariant named in NatSpec has a
Foundry test that fails when the invariant is deliberately broken** — provable,
and checkable by a reviewer — plus a TLA+ spec under `specs/tla/contracts/` for
the two contracts whose *state machines* genuinely warrant one (`VoteAllowance`,
`Sortition`). A Foundry test and a TLA+ spec answer different questions: the
test pins behaviour at specific inputs, the spec explores the reachable state
space. For a contract that is a pure function of its inputs
(`ThresholdApproval`, `ClassificationGate`) there is no interesting state space
and a spec would be ceremony; for a commit-reveal draw or a revocable franchise
there very much is.

> **Correction, 2026-07-27.** The first version of this SCOPE asserted "there are
> no TLA+ specs in citrate-chain at all" and concluded that §9's "follow the
> BFR-02 precedent" pointed at a precedent that was never set. **That was wrong**
> — a bad `find` scoped to `contracts/`. There are 399 `.tla` files, a runnable
> 48-spec subset, and a working TLC runner. The reading of the gate above does
> not change, but the reason does: TLA+ is chosen where the state machine
> warrants it, **not** because the tooling is absent. It is present and it works.
>
> The BFR-02-specific gap is real and worth flagging to whoever owns `rbac/`:
> those contracts cite `.tla` files by path in their NatSpec that are not in the
> repository. A cited spec that cannot be opened is weaker than no citation,
> because a reader assumes it was checked.

**Open question for the owner:** with the tooling this healthy, TLA+ for all six
contracts is a real option rather than a stretch. It is still a materially bigger
sprint. Say now if you want it, rather than discovering it at the gate.

## Work packages — the checklist this sprint is tracked by

Each is a vertical slice: contract + NatSpec-cited invariants + Foundry tests
that break when the invariant breaks + address-book entry. **A WP is not done
until its invariant tests fail on a deliberately broken build** (negative
control), because that is the only thing that proves the test tests anything.

| # | WP | Deliverable | Status |
|---|---|---|---|
| **S6.1** | `IGovernanceProtocol` + `GovernanceTemplateRegistry` | The audit boundary: only audited bytecode may ever govern. TR-1 immutability. | ☑ citrate-chain [#106](https://github.com/CitrateNetwork/citrate-chain/pull/106) — 13 tests, 3 negative controls, **merged** |
| **S6.2** | `GovernanceProtocolFactory` | CREATE2 deploy, GF-1 deterministic, GF-2 template-bounded, GF-3 spec-bound, GF-4 tenant-gated, GF-5 single-owner-per-salt. | ☑ citrate-chain [#107](https://github.com/CitrateNetwork/citrate-chain/pull/107) — 15 tests, 3 negative controls (R-C discharged: GF-2 mismatch reverts, and dropping `params` from the init hash is caught by `predict` diverging from the real CREATE2) |
| **S6.3** | Seed templates ×2 — `ThresholdApproval`, `ClassificationGate` | Two of the eight, chosen because they exercise both verdict shapes (RequireApproval, Deny) and the clearance ladder we already enforce off-chain. The other six are S6.7. | ☑ chain [#108](https://github.com/CitrateNetwork/citrate-chain/pull/108) + quorum [#39](https://github.com/CitrateNetwork/citrate-quorum/pull/39) — 28 tests, 6 negative controls. **R-D discharged**: `subjectKey` cross-pinned to one literal asserted in both repos. |
| **S6.4** | `PolicyBinding` | `(tenant, actionClass) → protocol[]`, `check()` → `{Allow, Deny, RequireApproval, RequireVote}`. **The enforcement table must ship with it**: advisory / binding / attested, stated per call site, never blurred. | ☑ chain [#109](https://github.com/CitrateNetwork/citrate-chain/pull/109) — PB-1..6, 20 tests end-to-end, 5 negative controls. Table shipped as [`ENFORCEMENT.md`](./ENFORCEMENT.md). |
| **S6.5** | `CapabilityGrant` | The on-chain half of the HIC envelope. CG-1…4. Mirrors `quorum-policy`'s in-process rules so the two cannot drift. | ☑ chain [#110](https://github.com/CitrateNetwork/citrate-chain/pull/110) — 20 tests, 5 negative controls |
| **S6.6** | `VoteAllowance` | Delegated franchise, VA-1…4, revoke immediate and unconditional. TLA+ for VA-1…4. | ☑ chain [#112](https://github.com/CitrateNetwork/citrate-chain/pull/112) — 16 tests + TLA+ (2,079,364 states, 0 errors), 6+3 negative controls |
| **S6.7** | Remaining six seed templates | `BudgetedAutonomy`, `SegregationOfDuties`, `TimeBoundedElevation`, `ChangeControlBoard`, `SupplierAdmission`, `IncidentEscalation`. | ☑ chain [#113](https://github.com/CitrateNetwork/citrate-chain/pull/113) — 34 tests, 6 negative controls; `QuorumIdentity` extracted so 4 contracts cannot drift on the subject key |
| **S6.8** | `Sortition` | Commit-reveal over a checkpoint-finalized seed. SO-1…3, including SO-3 "a draw that never finalizes is VOID, not best-effort". TLA+. | ☑ chain [#118](https://github.com/CitrateNetwork/citrate-chain/pull/118) — 25 tests + TLA+ (1,485 states), 7+3 negative controls |
| **S6.9** | Deploy to 40204 + address book + quorum sync | Frozen book entry per contract; `scripts/sync-addresses.sh` picks them up; no address hardcoded in quorum. | ☑ chain [#121](https://github.com/CitrateNetwork/citrate-chain/pull/121) + quorum [#41](https://github.com/CitrateNetwork/citrate-quorum/pull/41) — all 7 live on 40204, RBAC set redeployed, tenant root re-seeded, 53/53 + 24/24 verified to have code |
| **S6.10** | Internal audit pass + report in-repo | Slither + semgrep clean, or every finding triaged in writing. The gate says "audit report in-repo" and that is a file, not a claim. | ☑ [`AUDIT.md`](./AUDIT.md) — 39 Slither + 104 semgrep findings all triaged; the one substantive finding (`encode-packed-collision`) closed with a test |

## Explicitly NOT in this sprint

- **The authoring pipeline** (ingest → interview → spec → compile → simulate →
  ceremony → deploy → bind) and the executive "what would have been blocked"
  replay. That is QRM-S7 and it is what lights up quorum's Governance surface.
  S6 ships contracts with tests, not a UI.
- **Voting-power basis** (Q7 — SALT/stSALT vs an org token vs role-weighted). The
  allowance layer is basis-agnostic by design and stays that way here.
- **A third-party audit.** S6.10 is an INTERNAL pass. R11 stands: there is still
  no external audit anywhere in the federation, and nothing here may be described
  as audited in the sense a customer means.

## Risks

| # | Risk | Mitigation |
|---|---|---|
| R-A | "100% invariant coverage" is ambiguous (see above) | Defined in this SCOPE before any code; negative-controlled tests are the evidence |
| R-B | Six contracts + eight templates is a lot of surface to get right at once | Strict WP order; each merges on its own with its own tests rather than one heroic PR |
| R-C | Template-bounded compilation is the whole safety story (D4/R3) — if `initCodeHash` pinning is wrong, "only audited bytecode governs" is false | GF-2 gets a negative control: deploy with mismatched creation code and prove the factory reverts |
| R-D | These contracts duplicate rules `quorum-policy` already enforces in Rust | Every duplicated invariant cites its Rust counterpart by name in NatSpec, so a future divergence is visible in review |
