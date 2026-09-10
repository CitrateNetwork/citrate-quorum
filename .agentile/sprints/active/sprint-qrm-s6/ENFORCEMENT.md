---
created: 2026-07-27T03:00:00Z
branch: feat/qrm-s6-policy-binding
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
sprint: QRM-S6
---

# The enforcement table

> Ships with `PolicyBinding` (QRM-S6.4) because the planset requires it to, and
> because this is the single most tempting thing to blur in a sales conversation.

A governance protocol says what *should* happen. Whether that is binding depends
entirely on **where it is called from**. There are three classes, they are not
interchangeable, and a sentence like "our agents are governed" is not true or
false until you know which one is meant.

| Class | Where `check` runs | What a verdict is worth |
|---|---|---|
| **Binding** | On chain, by the contract that is about to act, in the same transaction | Cannot be bypassed. A `Deny` means the action reverts. |
| **Advisory** | citrate-quorum's policy gate, in the agent adapter, before an off-chain action | Binds every agent that goes through our adapter. An agent operating outside it is bound by nothing here. |
| **Attested** | After the fact — the decision record carries the verdict the action got | Provable, not preventive. |

## Where each call site actually sits today

Stated as of 2026-07-27. This table is the thing to update when a call site
moves, and a claim that outruns it is a claim we cannot support.

| Call site | Class | Status |
|---|---|---|
| `PolicyBinding.check` from an on-chain contract before it acts | Binding | **No such caller exists yet.** The contract is deployed-ready; nothing on chain consults it. |
| quorum's policy gate in the agent adapter (`quorum-policy`, in process) | Advisory | Live. Every tool invocation passes it before execution (CLAUDE.md rule 4). |
| MR-4 room admission (`rooms.rs::mr4_admits`) | Advisory | Live. Refuses an agent without a grant that clears the room. |
| Decision records in the ledger, carrying the verdict and HIC level | Attested | Live. |
| Meeting ratification anchored in `MeetingRegistry` | Attested | Live, on chain. |
| `PolicyBinding` consulted by quorum's gate | Advisory | **Not wired.** QRM-S7 does this. |

Two honest readings follow from that table:

1. **Nothing in the product is binding today except what a contract does to its
   own state.** The governance contracts this sprint built are correct and
   tested, and no agent action is currently prevented by them, because the
   callers do not exist yet.
2. **Advisory is not nothing.** An action that happened without a verdict is
   visible as such, and an action taken against a `Deny` is provably a violation
   rather than a disagreement. That is worth more than it sounds — but it is not
   prevention, and must never be described as prevention.

## Enterprise reality

Most agent work is off chain: writing code, sending mail, filing tickets. So
most enforcement will be **advisory + attested** no matter how much of this we
build, with binding enforcement reserved for what touches chain state, money, or
capability grants. That is not a gap to be closed; it is the shape of the
problem. Claiming otherwise would require an agent's every keystroke to be a
transaction.

## What we may and may not say

**May say:**
- "Every agent action through our adapter is checked against the tenant's
  governance protocols before it executes, and recorded with the verdict it
  got."
- "Protocols are deployed only from audited, registered bytecode, and a
  deployment is inseparable on chain from the plain-English spec it implements."
- "Designed to generate SOC 2 Type 2 control evidence in your environment."

**May not say:**
- "Agents cannot violate policy." They can, by not going through the adapter.
- "Enforced on chain," without naming which actions. Today, none through
  `PolicyBinding`.
- "SOC 2 certified" or "SOC 2 compliant" — we ship into the customer's control
  environment; they are the audited entity.
- Anything asserting compliance with an export-control regime. That is a legal
  conclusion and no engineering decision supplies it. `ClassificationGate`'s
  foreign-national rule is a **deployment parameter**, not an encoded regime.
- "Audited," in the sense a customer means. R11 stands: there is no external
  audit anywhere in the federation. S6.10 is an internal pass.

## Two design decisions this table forced

**Unbound is not the same as allowed.** An action class nobody has bound returns
`Allow` with reason `PB_UNGOVERNED`, which a caller must not treat as
`PB_ALLOWED`. Quorum's rule 5 turns that into an `ungoverned` record and an
alert. The alternative — deny everything unbound — sounds safer and is worse: a
tenant would be dead until every action class its agents might ever touch had
been enumerated, which in practice produces one permissive catch-all binding and
a false sense of coverage. A tenant that genuinely wants fail-closed opts in with
`setDefaultDeny`.

**A protocol that reverts is a `Deny`, not a skip.** Otherwise the way to escape
a rule is to break the contract that enforces it.
