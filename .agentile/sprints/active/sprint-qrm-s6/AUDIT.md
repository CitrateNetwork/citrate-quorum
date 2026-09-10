---
created: 2026-07-27T16:30:00Z
branch: audit/qrm-s6-internal
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
sprint: QRM-S6
wp: S6.10
---

# QRM-S6 internal audit pass

> **This is an INTERNAL pass. Nothing here is an external audit, and nothing in
> this sprint may be described to a customer as audited.** R11 stands: there is
> no third-party audit anywhere in the federation. The word "audit" in this file
> means "two static analysers were run and every finding was triaged in
> writing", which is what the sprint gate asks for and is all it asks for.

## Scope

The twelve contracts QRM-S6 produced, plus `MeetingRegistry`:

`IGovernanceProtocol` · `GovernanceTemplateRegistry` · `GovernanceProtocolFactory` ·
`PolicyBinding` · `CapabilityGrant` · `VoteAllowance` · `Sortition` ·
`QuorumIdentity` · and the eight seed templates
(`ThresholdApproval`, `ClassificationGate`, `BudgetedAutonomy`,
`SegregationOfDuties`, `TimeBoundedElevation`, `ChangeControlBoard`,
`SupplierAdmission`, `IncidentEscalation`).

## Tools and raw counts

| Tool | Version/config | Ran over | Findings in scope |
|---|---|---|---|
| Slither | 101 detectors, `--filter-paths lib/\|test/\|script/` | whole `contracts/` project (208 contracts, 714 findings) | **39** |
| semgrep | `p/smart-contracts`, 50 rules | the 17 in-scope files | **104** (103 INFO, 1 ERROR) |

**The two tools independently converge on exactly one substantive finding**, and
it is the same one. Everything else is style, gas, or a false positive. That
convergence is the most useful thing this pass produced.

## The one finding that needed real work

### `encode-packed-collision` — Slither High/High ×2, semgrep ERROR ×1

`GovernanceProtocolFactory.predict` and `.deployProtocol` both call
`abi.encodePacked(creationCode, params)` with two dynamic `bytes` arguments. The
general shape of the warning is correct: concatenated dynamic bytes can be
re-split without changing the result, so `("ab","c")` and `("a","bc")` produce
identical output.

**Verdict: real collision, no exploit — and now proven by test rather than
asserted.**

Two things make it harmless here:

1. **The concatenation is not a hashing choice.** `creationCode ‖ params` is the
   EVM's own init-code layout. `abi.encode` would produce bytes that are not
   deployable at all, so "use abi.encode instead" is not available.
2. **GF-2 hashes `creationCode` independently.** The registry pins
   `keccak256(creationCode)` alone. An attacker who shifts bytes across the
   boundary changes that hash and fails the registry check — even though the
   init code, and therefore the CREATE2 address, is byte-identical.

So the collision buys nothing: you can reach the same address only by supplying
a `creationCode` that is no longer the audited one, which is precisely what the
registry exists to refuse.

**Action taken:** added
`test_GF2_reSplittingCreationCodeAndParamsCannotEvadeTheRegistry`, which moves
one byte across the boundary and asserts all three legs — the re-split really
does collide, it really does predict the same address, and it is *still* refused
with `CreationCodeMismatch`. A future reader (or auditor) who flags this again
will find the answer as an executable test.

## Everything else, triaged

### False positive

**`uninitialized-state` — `PolicyBinding._bound` (Slither High/High).**
`_bound` is a `mapping`. Mappings have no uninitialized state in Solidity; it is
written in `bind()`. Slither flags private mappings not assigned in a
constructor. No action.

### By design, and bounded

**`calls-loop` ×4** — `PolicyBinding._ask`, `ThresholdApproval._tally`,
`IncidentEscalation.check`. External calls inside loops are the *point* of these
contracts (fan out to bound protocols, tally approvers, scan incidents). Each
loop is deliberately bounded, and the bound is a named invariant:
`MAX_PROTOCOLS_PER_ACTION = 16` (PB-5), approver set ≤ 64, `maxScan` on incident
history. `PolicyBinding._ask` additionally wraps each call in `try/catch` so a
failing protocol is a `Deny`, not a revert (PB-6). No action.

**`timestamp` ×8** — every instance is an expiry or window comparison
(`CapabilityGrant.isLive`, `VoteAllowance.grant`, `ThresholdApproval` expiry,
`TimeBoundedElevation`, `ChangeControlBoard` review/timelock,
`IncidentEscalation` SLA). Proposer timestamp influence is seconds; these
windows are hours to days. **Where timestamp manipulation would actually
matter — `Sortition` — the contract deliberately does not use timestamps at
all**, keying off block numbers and `blockhash` precisely because a proposer can
nudge a timestamp. That is the design answer to this detector and it is already
in the code. No action.

**`assembly` ×1** — the `create2` opcode in `deployProtocol`. Solidity has no
high-level CREATE2 that takes raw init code. Reviewed; three lines; covered by
the GF-1/GF-5 tests including the negative control that proves `predict` and the
real deployment are the same computation. No action.

**`missing-inheritance` ×5** — Slither notes the deployed `rbac/` contracts
"should inherit" the minimal interfaces the templates declare
(`IMultiSigEnvelope`, `IClassificationRegistry`, `IRoleEscalation`,
`IContradictionLedger`, `ITenantHierarchy`). Deliberate: those contracts are
already deployed, and the interfaces are declared locally so a customer can
point a template at their own implementation. Making them inherit would require
redeploying BFR-02. The drift risk this creates is handled the other way — the
test suites exercise these interfaces against the **real** contracts, so a
layout change breaks a test. No action.

**`cyclomatic-complexity` ×1** — `CapabilityGrant.issue`. Every branch is a
distinct, documented refusal (zero principal, zero consumer, on-behalf-of
without admin, empty/duplicate/oversized action classes, budget over `u64`,
ceiling above tenant, already expired). Splitting it would hide the validation
sequence a reader needs to follow. Accepted deliberately.

### Style and gas, not accepted-as-is but not fixed here

**`uninitialized-local` ×7** — counters and flags relying on Solidity's zero
initialization (`approvals`, `eligible`, `n`, `signed`, `pendingCount`). Correct
as written; explicit `= 0` would be noise.

**semgrep INFO ×103** — `unnecessary-checked-arithmetic-in-loop` (42),
`array-length-outside-loop` (29), `state-variable-read-in-a-loop` (13),
`non-payable-constructor` (12), `use-nested-if` (6),
`use-prefix-increment-not-postfix` (1). All gas micro-optimisation on
**`view` functions with bounded loops**. `unchecked` blocks and cached lengths
would save gas that no caller pays, at the cost of readability in exactly the
code an auditor will read most closely. Slither's `cache-array-length` ×5 and
`naming-convention` ×5 are the same class.

Not fixed, and that is a judgement rather than an oversight: these contracts are
read far more often than they are called, and every one of these changes trades
clarity for gas in a `view`. If a binding on-chain enforcement site is ever added
that calls `PolicyBinding.check` in a transaction, the loop-gas findings should
be revisited **then**, with a real gas budget to measure against.

## What this pass did NOT do

- **No external audit.** See the banner.
- **No formal verification beyond what already shipped.** `VoteAllowance` and
  `Sortition` carry TLA+ specs with complete state-space exploration
  (2,079,364 and 1,485 distinct states, negative-controlled). The other ten
  contracts do not, by the reasoning fixed in `SCOPE.md`: a pure function of its
  inputs has no interesting state space.
- **No economic or governance-capture analysis.** Slither and semgrep do not
  model "what if the tenant admin is hostile". The contracts state their trust
  assumptions in NatSpec — most sharply `ThresholdApproval`, which documents that
  `MultiSigEnvelope.sign` does not authenticate signers, so an `Allow` means
  "N signatures are recorded", not "the chain verified them". **That remains the
  largest known gap in the set and no static analyser will find it.**

## Result against the gate

The sprint gate asks for "Slither/semgrep clean, or every finding triaged in
writing", plus an audit report in-repo.

- 39 Slither + 104 semgrep findings in scope, **all triaged above**.
- One substantive finding, closed with a test rather than an argument.
- Full contracts suite after the change: **2787 passed, 0 failed.**
