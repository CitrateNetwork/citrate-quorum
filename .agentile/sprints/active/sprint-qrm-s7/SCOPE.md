---
created: 2026-07-27T18:00:00Z
branch: docs/qrm-s7-scope
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
sprint: QRM-S7
---

# Sprint QRM-S7 — the authoring pipeline (Phase 2, part 2 of 2)

> **Sprint goal.** A doc dump becomes a deployed, bound governance protocol,
> end to end — and the executive can see what it *would have blocked*.

QRM-S6 shipped the contracts. They are live on 40204 and **nothing calls them**:
quorum's Governance surface is five `Unavailable` stubs, no template is
registered, and no protocol is bound. S7 is what turns that from a deployed
library into a product.

Normative spec: `02_ARCHITECTURE.md` §5 and `05_SCOPE_AND_SPRINTS.md` (QRM-S7)
in the federation planset (`plan/quorum-s0`; read with `git show`).

Planset exit gate: *"A doc dump becomes a deployed, bound protocol on devnet,
end to end."*

## The pipeline

```
INGEST → INTERVIEW → DRAFT SPEC → COMPILE → SIMULATE → CEREMONY → DEPLOY → BIND
```

## Measured against reality before writing this

| What the plan assumes | What is actually true (checked 2026-07-27) |
|---|---|
| The S6 contracts are available to compile against | **True and live.** Registry, factory, PolicyBinding, CapabilityGrant, VoteAllowance, Sortition, MeetingRegistry, AnchorRegistry all deployed and booked, tenant root seeded, `PolicyBinding.check()` answering on chain. |
| There is a `SpecRegistry` to bind `specCID` into (step 8) | **True.** Live at `0x2938320a…`, 8,997 bytes of code, already in the main book. |
| The ceremony can carry a deploy intent (step 6) | **Partly.** `IntentKind::Transaction` exists in the kit — but its own comment says *"recoverable EIP-155 form deferred to B1.4"*. Whether a deploy transaction can actually be signed end-to-end is **WP-0's first question**, and if it cannot, that is a kit change and an owner gate, not something to route around. |
| Quorum's Governance surface consumes the pipeline | **False, and it cannot yet.** `GovernanceDomain` is five **no-argument** methods (`protocols/clauses/simulate/ingest/interview`), all `na()` stubs. A pipeline needs `ingest(file)`, `interview(answer)`, `compile(specId)`, `deploy(specId)`. **The bridge contract was frozen at S2D.3 and changing it is an owner decision** — see the gate below. |
| Templates can be registered so the factory can deploy them | **Blocked, deliberately.** `GovernanceTemplateRegistry.register` requires a non-empty `auditCID`, by design. We have no audit artifacts. See "The auditCID problem". |

## Two gates — ANSWERED 2026-07-27 (owner)

### D-1 (G-S7-1): the bridge contract changes — APPROVED

Owner approved the proposed shape and delegated the detail. Shipped in quorum
PR #44. What I decided, and why:

- **`clauses()` removed.** A spec HAS clauses; an orphan read could disagree
  with the spec it came from.
- **`ingest`/`interview` become acts, not reads.** `interview(specId, answer?)`
  is one call for start/resume/advance — a resumed interview and a fresh one
  differ only in how much has been answered, so two methods would be two names
  for one state machine.
- **`deploy` is two-phase** (`deployIntent`/`deployComplete`), matching
  `rooms.connectIntent`/`connectComplete`.
- **`CompileResult.unmapped` and `Simulation.unchanged` are types, not
  intentions** — R-A and R-B are now impossible to satisfy by omission.

### G-S7-1 (original statement): the bridge contract has to change

`GovernanceDomain`'s five methods take no arguments because they were shaped for
the sim adapter. Every stage of this pipeline needs parameters. The contract is
frozen at S2D.3 and unfreezing it is the owner's call, not mine.

**Proposed shape** (owner to approve, amend or reject):

```ts
ingest(files: IngestRequest[]): Promise<IngestResult>;
interview(specId: string, answer?: InterviewAnswer): Promise<InterviewTurn>;
spec(specId: string): Promise<GovernanceSpec>;
compile(specId: string): Promise<CompileResult>;   // includes unmapped clauses
simulate(specId: string, range: DateRange): Promise<Simulation>;
deploy(specId: string): Promise<CeremonyHandle>;   // routes through the ceremony
protocols(): Promise<Protocol[]>;                  // unchanged
```

### D-2 (G-S7-2): devnet-only CID — DECIDED

Owner, 2026-07-27: **option 3.** This is a working demo, some things stay
unfinished until the client makes key calls, and the owner is separately
auditing the templates with Trail of Bits tooling — which will produce a better
answer *and* a repeatable in-flow audit step.

So: register templates on devnet with a CID whose content states in plain text
that this is an unaudited devnet deployment. When the Trail of Bits pass lands,
that CID is replaced by real audit evidence and this decision is revisited.

**The constraint that survives:** nothing registered under a devnet CID may be
described to a customer as audited, in the product or anywhere else. The field
is named `auditCID` and a reader will assume it means an audit — so the CID's
own content has to say what it is.

### G-S7-2 (original statement): the `auditCID` problem

The registry refuses a template without an audit CID. That refusal is the whole
point of the contract — S6.2's NatSpec says so, and S6.9 deliberately registered
**zero** templates rather than pass a placeholder.

We wrote the eight templates ourselves. There is no external audit (R11), so
there is nothing honest to put in that field today. Three options, none of which
I will pick unilaterally:

1. **Register nothing until a real audit exists.** Honest; leaves the factory
   unusable, so S7's exit gate cannot be met on mainnet-shaped terms.
2. **Pin the S6.10 internal audit report** (`AUDIT.md`) to IPFS and use that CID,
   with the field's meaning documented as "the audit evidence we have, which is
   an internal pass". Meets the gate; risks the word "audit" being read by a
   customer as more than it is.
3. **Register on devnet only**, with a CID that says in plain text that this is
   an unaudited devnet deployment.

I lean to **(3) for the sprint and (1) for anything a customer touches**, but it
is an owner decision because it decides what the word "audited" means in our
product.

### D-3 (G-S7-3): the tenant-admin key — OWNER ACTION, 2026-07-28

**The live run is blocked on this and nothing else.**

`GovernanceProtocolFactory.deployProtocol` (GF-4) and `PolicyBinding.bind` both
require `msg.sender` to be an admin of the tenant. The root tenant `Citrate`
(`keccak256("Citrate")` = `0x16b1608b…`) has three, set immutably at
`initRoot`:

```
0xF4FE9B2c6441Ff7c081B60716a78193127919783
0x269deEe81cb8Eb5899b2D17945b951608E41774B
0x671F3F4f9cBb0509a28eE4fa0b416daBBc9375C5
```

Each has **nonce 0 and 50 CIT** — funded, never used. Their private keys are not
in the workspace's env file, not in the local signing keystore (which holds the
deployer, governance, and devops keys — `0x4fAB35c8` / `0xa7ec54f1` /
`0x60434243`), and nothing in the workspace records them: they were passed as
`CIT_AGENT_TIMELOCK_OWNER_{0,1,2}` at deploy time. `initRoot` is one-shot and consumed, so there is no second root,
and `createNode` requires an admin of the parent.

Owner decision, 2026-07-28: **supply one of the three.** It is used ONCE, to
`createNode` a child tenant under `Citrate` whose admin is quorum's own operator
wallet. Every deploy and bind after that is signed by quorum through the
ceremony — the architecturally correct end state, and the root key never touches
the app.

### D-4 (G-S7-4): SpecRegistry — DECIDED 2026-07-28

`SpecRegistry.registerSpec` is `onlyGovernance`, and `governance()` on the live
contract is `0x4fAB35c8` — the deployer, not quorum's operator wallet. The app
can never call it.

Owner decision: **drop it from the app.** `specCID` and `specHash` already
travel into `GovernanceProtocolFactory`'s `Deployment` record, which is what
makes a deployment inseparable on chain from the spec it implements — the
binding that actually matters. `SpecRegistry` is a federation-level registry
this product does not write to, and S7.7's scope is the `PolicyBinding` half.

## Work packages

Each is a vertical slice with an honest failure mode. **The rule inherited from
S6 stands: a WP is not done until its tests fail on a deliberately broken
build.**

| # | WP | Deliverable | Status |
|---|---|---|---|
| **S7.0** | Gates + the ceremony spike | Answer G-S7-1 and G-S7-2. Prove (or disprove) that a deploy transaction can be signed through the existing ceremony. No pipeline code until this lands. | ☑ D-1 + D-2 above; contract shipped in quorum [#44](https://github.com/CitrateNetwork/citrate-quorum/pull/44). **R-C discharged**: `approve_and_broadcast` signs a real EIP-155 **legacy** tx (exactly what 40204 needs) and `LegacyTxFields.to` is `Option`, `None` = contract creation. No kit change needed. |
| **S7.1** | INGEST | Parse/chunk a doc dump, record source provenance, detect classification. Classification routes model + storage — a CUI doc must not reach a model that is not cleared for it. | ☑ [#45](https://github.com/CitrateNetwork/citrate-quorum/pull/45) |
| **S7.2** | INTERVIEW | The harness agent surveys the HIC: scope, principals, roles, thresholds, escalation, expiry, exceptions. Every answer is attributable; nothing is inferred silently. | ☑ [#46](https://github.com/CitrateNetwork/citrate-quorum/pull/46) |
| **S7.3** | DRAFT SPEC | The Governance Spec: plain-English clauses ⟷ Gherkin scenarios ⟷ typed params, side by side, HIC-editable. | ☑ [#47](https://github.com/CitrateNetwork/citrate-quorum/pull/47) |
| **S7.4** | COMPILE | spec → `(templateId, params)` against the live registry. **Any clause that does not map to a template MUST be flagged, not improvised.** This is S7's Rule 1 and its most important invariant. | ☑ [#48](https://github.com/CitrateNetwork/citrate-quorum/pull/48), [#50](https://github.com/CitrateNetwork/citrate-quorum/pull/50) |
| **S7.5** | SIMULATE | Replay historical decisions from the ledger through the proposed policy; show what would have been blocked. The executive's proof — and the only WP whose value is entirely in being *believable*. | ☑ [#49](https://github.com/CitrateNetwork/citrate-quorum/pull/49) |
| **S7.6** | CEREMONY + DEPLOY | HIC approves a decoded deploy intent; CREATE2 address shown *before* signing (GF-1 exists precisely for this). | ☑ [#51](https://github.com/CitrateNetwork/citrate-quorum/pull/51), [#52](https://github.com/CitrateNetwork/citrate-quorum/pull/52); **phase two + the two correctness bugs** in [#53](https://github.com/CitrateNetwork/citrate-quorum/pull/53) — see R-F |
| **S7.7** | BIND | protocol → tenant via PolicyBinding; the ledger shows the diff. **SpecRegistry dropped from the app** — see D-4. | ☑ [#53](https://github.com/CitrateNetwork/citrate-quorum/pull/53) |
| **S7.8** | The surface | Wire the Governance surface to the real pipeline. No stub may remain that looks live. | ☑ [#53](https://github.com/CitrateNetwork/citrate-quorum/pull/53) |
| **The gate** | The live run | A real doc → deployed, bound protocol on 40204, with evidence from outside the UI at each step. | ☐ **blocked — D-3** |

## Risks

| # | Risk | Mitigation |
|---|---|---|
| R-A | **The compiler improvises.** An LLM asked to map English to template params will happily invent a mapping that type-checks and is wrong. | S7.4's unmapped-clause flag is a hard output, not a warning; a spec with unmapped clauses cannot compile. Negative-controlled. |
| R-B | **Simulation that flatters.** A replay that only shows blocked-in-hindsight cases is a demo, not evidence. | Replay must include actions the policy would have ALLOWED, and cases where it would have changed nothing. A simulation with no null result is a red flag. |
| R-C | ~~The ceremony cannot carry a deploy transaction~~ | **DISCHARGED.** `approve_and_broadcast` is a real path: decode → live nonce/gas → EIP-155 legacy signature → `eth_sendRawTransaction` → receipt poll. |
| R-D | `auditCID` forces a dishonest field | **Answered by D-2**: devnet-only CID whose content states it is unaudited. Residual risk is linguistic, not technical — the field is *named* `auditCID`, so the CID must say what it is. |
| R-E | ~~Bridge-contract churn~~ | **Answered by D-1**: one change, all at once, shipped. Two further changes were needed and are argued in [#53](https://github.com/CitrateNetwork/citrate-quorum/pull/53): `bind` split two-phase, and `DeployResult.matchedPrediction` removed. |
| R-F | **A deploy intent that cannot succeed, and says nothing.** Found 2026-07-28. S7.6 phase one built its transaction with `params = &[]` and a `tenantId` of `blake3(scope)` — neither of which the chain accepts. The failure was invisible because `predict()` hashes `creationCode ‖ params`: the address shown to a human before signing was a well-formed CREATE2 address of a contract whose constructor reverts. GF-1's promise was void and nothing reported it. | Fixed in [#53](https://github.com/CitrateNetwork/citrate-quorum/pull/53). The structural fix is that the whole `deployProtocol` call is now dry-run **as the operator's address** before a ceremony is raised, so every gate the chain enforces (GF-2, GF-4, the constructor's own requires) is answered while it is still cheap to say why. A `from`-less `eth_call` runs as the zero address and would have passed all of them. |

## Explicitly NOT in this sprint

- **A third-party audit.** R11 stands.
- **Binding enforcement on chain.** The enforcement table (S6.4) says
  `PolicyBinding` has no on-chain caller. S7 makes the *advisory* path real; a
  contract that calls `check()` before acting is a later sprint and a bigger
  claim.
- **Voting-power basis (Q7).** `VoteAllowance` stays basis-agnostic.
