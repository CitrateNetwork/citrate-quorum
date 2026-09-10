---
created: 2026-07-26T15:00:00Z
branch: feat/qrm-phase0-live-surfaces
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
sprint: QRM Phase 0 (completion planset)
---

# Phase 0 — Retrospective (four dark surfaces made live)

> There is no `SCOPE.md` in this directory, deliberately. Phase 0 was not a
> planset sprint; its scope of record is the Phase 0 table in
> `.agentile/planset/COMPLETION_PLAN.md`, written 2026-07-26 after the owner
> opened the packaged app and found half the sidebar saying "not wired yet".
> Duplicating it here would create a second source of truth for a slice that
> already had one (Rule 9).

> The slice where making a read *succeed* broke a signing flow three surfaces
> away — and where wiring four surfaces turned out, again, to mean auditing
> everything they already claimed.

## Outcome

| Field | Value |
|-------|-------|
| **Goal achieved?** | **Yes.** All four Phase 0 stubs are live on the packaged binary against chain 40204: `node.*`, `wallet.summary`, `settings.tenancy`, `ledger.decision`/`correlation` (+ a real `verifyDecision`). 7 of 14 stubbed bridge methods closed. |
| **Verified how** | `verify_meetings.py` 17/17 including the on-chain leg; the packaged `.deb` driven surface by surface; rendered values cross-checked against 40204 with `cast`. |
| **Carry-forward** | Sending SALT (needs a transfer intent — deliberately out of scope, see below); `AnchorRegistry` anchoring of the whole evidence chain's Merkle root; `ClassificationRegistry` read for the on-chain half of clearance; `session.current` (OIDC RP, Phase 3). |
| **Closing branch** | `feat/qrm-phase0-live-surfaces` → PR #32 |

## Metrics delta

| Axis | Start | End | Δ |
|------|-------|-----|----|
| Rust tests | 104 (+1 live-only) | **122** (+3 live-only) | +18, +2 live |
| Frontend tests | 38 | **46** | +8 |
| Gate checks | 20/0/1 | **20/0/1** | 0 (21/0/0 with `QUORUM_SMOKE=1`) |
| Live surfaces | 7 | **11** | +4 (Node, Wallet, Settings, Ledger complete) |
| Stubbed bridge methods | 14 | **7** | −7 |
| Address books read | 1 | **2** | +1 (BFR, never merged) |
| Fabricated claims removed | — | **11** | — |

## What worked

**Reading the chain before designing the surface.** Ten minutes of `cast` and a
raw `eth_getBlockByNumber` against 40204 decided the whole Node surface: the
node returns `blueScore`, `selectedParentHash` and `mergeParentHashes`, and
nothing that says "checkpoint". The prototype's per-block blue/anticone flag and
checkpoint marker had no source, so they are gone rather than approximated. The
same ten minutes found that `TenantHierarchy.root()` is the zero word, which
turned "wire the tenancy read" into "wire it, and report the deployment gap it
exposes".

**Pinning the ABI decoder against a real encoder, not against my own
arithmetic.** `getNode` returns a dynamic struct holding a `string` and an
`address[]`, so both tail offsets are relative to the struct start, not the
return start. A paper derivation that is one word out still decodes — it just
reads the wrong field, silently. The fixture in
`chain::tests::abi_fixture_decodes_to_the_encoded_values` is `cast abi-encode`
output, pasted verbatim, with the command that produced it in the doc comment.

**Making Verify do two things and say which.** Replaying the chain from genesis
and proving one record into the Merkle root are different claims, and a single
"✓ verified" collapses them. `quorum-audit` gained `merkle_proof` /
`verify_inclusion` so the second claim is real; the button reports both, and the
inclusion proof is what an auditor holding one record could check without us.

**Two address books, explicitly not merged.** `ComputeVerifier`,
`ComputeMarketplace` and `TEEAttestationRegistry` exist in both the main and BFR
books at *different* addresses. A merged lookup would have silently picked a
winner — precisely the failure rule 8 exists to prevent. There is a test that
fails if the collision ever disappears, so the justification cannot go stale
without someone noticing.

## What didn't work

**I broke the packaged signing flow by making a read succeed.** The vault and
signing-identity panels had been reachable from the Settings *error* path,
rendered under the tenancy failure by a thoughtful accommodation that was
correct when it was written. `verify_meetings.py` had been clicking them there
and had never once opened the Identity tab. Tenancy going live removed the
error plate, the script typed a passphrase into empty space, the vault stayed
locked, and the on-chain registration failed: 16/17. Written up in
`journals/2026-07-26T1440_…`; the lesson is that an error path is a render path,
and a render path acquires users.

**The packaged drive had never been repeatable, and nobody knew.** Wiping the
app-data directory is not a clean install: `custody-master-key` and
`custody-generation` survive in the *shared* `ai.citrate.core` keyring, so the
second custody run of any flow hits the anti-rollback guard and reports "custody
envelope corrupt or tampered". Correct behaviour; useless test. Every previous
run of the custody half either happened to be first or passed for a reason
nobody checked. It now re-execs under a private D-Bus session with its own empty
keyring — the alternative, deleting a developer's keyring entries, would have
destroyed the citrate-core vault sealed under the same service.

**Eleven more claims on surfaces that were never true.** Uptime "99.2% · 30d";
"Validating — live" on an app that validates nothing; a local hash chain at
"#48,214" on an empty tenant; "gov-lora v2.3 · next exchange in ~18s" with no
model runtime; a paymaster budget at 64%; staking bond/APR/rewards with no
staking contract in the book; a transaction history; agent spend bars; "Meridian
Okta · OIDC · healthy" with SCIM at "1,204 seats"; "Seats licensed 1,500"; and
an evidence pack quoting a period and "14,208" records. The check gate's
fabricated-stats scan caught none of them: it looks for quoted literals with
thousands separators, and most of these were prose in JSX or embedded in a
longer string.

**A Verify button whose failure state was unreachable.** `setTimeout(() =>
setState("ok"), 900)`. It had `bad` and `pending` states defined in the same
component, and no code path could ever reach them. Essay:
`essays/2026-07-26T1500_a-check-that-cannot-fail.md`.

## What surprised us

**The app told me exactly what was wrong, and that is why it took ten minutes.**
When the chain leg failed, the ceremony said "registered in MeetingRegistry
FAILED — wallet: custody vault locked or unavailable". Not "an error occurred".
Every honesty rule this repo enforces was paying for itself in that one
sentence: the ratification had genuinely happened locally, the chain write had
genuinely not, and the reason named the subsystem. The honesty work is usually
justified as being for the customer. It was for me that afternoon.

**The tenancy read's most valuable output was an empty table.** `TenantHierarchy`
is deployed and holds no root, because `initRoot` has never been called — a fact
that lived nowhere until a surface tried to read it. The plate now names the
contract, the chain, the reason and whose decision it is. A surface that reported
"no tenants" would have hidden a deployment gap behind a plausible-looking empty
state.

**Deleting features made the product read as more finished.** The Wallet lost
Send, Stake, a transaction history and three tiles, and it is now the first
version of that surface an operator could trust. Everything remaining is a live
read with its source named.

## Decisions ratified mid-slice

- **`NodeDomain` changed shape**, against the S2D.3 freeze, because none of it
  survived a real chain. Recorded as
  `decisions/2026-07-26_nodedomain-contract-change.md`; owner ratification
  pending, and the reasoning is written down so it can be refused.
- **An unbuilt thing gets a plate that names it and where it lands** — not a
  softened version of what it would have been. Five plates on Settings alone.
- **A token that does not answer is omitted and named**, never rendered as a
  zero balance. A zero is a claim about someone's money.
- **`net_peerCount` returning nothing is not zero peers.** `null` survives the
  whole way to the surface, which renders "—" and says the endpoint did not
  answer.
- **Balances are string arithmetic.** No float touches money; the exactness test
  pins a 24-digit wei balance an `f64` cannot hold.
- **Sending is out of scope, and the panels that pretended to send are gone.**
  A transfer intent is ~60 lines mirroring `meeting_register_intent`, and money
  movement earns its own work package rather than riding in on a read-only
  slice.

## Action items for the next phase

- [ ] **Phase 1 (Rooms, QRM-S3)** — the relay at `wss://comms.citrate.ai` is the
      real blocker; G1 no longer is.
- [ ] Decide whether to seed `TenantHierarchy.initRoot` (owner: root admins,
      M-of-N threshold, classification ceiling — and only the deployer may call
      it).
- [ ] A transfer intent for the Wallet, if sending matters before Phase 2.
- [ ] Anchor the evidence chain's Merkle root to `AnchorRegistry` periodically;
      the Node surface currently says plainly that this is not wired.
- [ ] Read `ClassificationRegistry` so the on-chain half of clearance is in
      force — the Settings Identity tab says it is not.
