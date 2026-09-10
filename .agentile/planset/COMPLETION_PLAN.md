---
created: 2026-07-26T20:45:00Z
branch: docs/completion-planset
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
---

# Completion planset — every surface real, nothing stubbed

> Written 2026-07-26 after the owner opened the packaged app and found half the
> sidebar showing "not wired yet". That reaction was correct and this document
> is the answer to it: what is actually stubbed, why, and the order that closes
> it fastest.

## Where it actually stands

**Phase 0 is done** (2026-07-26, branch `feat/qrm-phase0-live-surfaces`). Seven
of the fourteen stubs are closed; six remain, all of them behind an external
gate rather than behind us.

| Surface | State | What it needs |
|---|---|---|
| Dashboard | **live** | — |
| Meetings | **live** | — (full lifecycle + on-chain anchor) |
| Journal | **live** | — |
| Ledger | **live** | — (decision document + real Verify + correlation) |
| Agents | **live** | — |
| Settings | **live** | — (tenancy reads `TenantHierarchy`; see the owner action below) |
| Node | **live** | — (height/peers/client/sync, blocks, RPC activity) |
| Wallet | **live** (reads) | sending needs a transfer intent — its own WP |
| Rooms | **live** | — (real MLS group on the relay; voice/at-rest/MR-4 named as absent) |
| Governance | **dark** (5) | the S6/S7 contracts + pipeline |
| Calendar | **dark** (2) | Google/Graph OAuth |
| Repos | **dark** (3) | GitHub App |

### What Phase 0 shipped, and what it deliberately did not

The `NodeDomain` contract changed, which the S2D.3 freeze note says is an owner
decision. It was changed because none of it survived contact with a real chain:
`blocks(height): Block[]` cannot be synchronous when every row is an
`eth_getBlockByNumber`; there is no peer LIST to return (`net_peerCount` gives a
count and 40204's public RPC exposes no enumeration); and there are no node logs
to stream, because this app supervises no node. The per-block "blue/anticone"
flag and checkpoint marker are gone for the same reason — the RPC returns
`blueScore`, `selectedParentHash` and `mergeParentHashes`, and nothing that says
"checkpoint".

Making four surfaces live also meant deleting what the design prototype had left
sitting on them: uptime and validator tiles, a paymaster budget, staking
bond/APR/rewards, a transaction history, agent spend bars, an IdP federation with
a seat count, a bundled model, a licensed-seat count, an evidence pack with a
period and a record count, and a Verify button that was a 900ms timer which
always ended in "✓ verified". Each is now either a real read or a plate that
names what is absent.

**Owner action — DONE 2026-07-26.** `TenantHierarchy`'s root was the zero word;
it is now seeded (citrate-chain PR #105, tx `0x9c301e12…`, block 148151):
`keccak256("Citrate")`, the three 2-of-3 timelock keys as admins, threshold 2,
ceiling CUI. The id and the admin set can never change (`initRoot` is one-shot,
there is no `setAdmins`); the ceiling can be raised later. Note that
`admin_threshold` is DATA on that contract, not enforcement — any one admin key
can create a node today.

`ClassificationRegistry` is now read too: Settings → Identity shows the
operator's clearance, and distinguishes **"no record"** from **"cleared to
Public"** — `getClearance` collapses those, `getRecord` does not, and they need
different fixes. Both still enforce as Public.

## Why it looks like this

Sprints S1–S6 went **deep on one column** — the governance spine: policy gate →
evidence chain → capability grants → agent adapters → meetings → signing →
on-chain anchor. That column is genuinely finished, end to end, on a packaged
binary with an on-chain commitment.

The cost was breadth. Every other surface was ported from the design prototype
in S2D and left honestly stubbed, waiting for its own sprint. Nothing is
*broken*; a lot is simply *absent*, and the honest-empty plates make the absence
loud — which is correct behaviour and also exactly why the app reads as thinner
than it is.

## The reorg: seven of the fourteen are already unblocked

This is the finding that changes the plan. **Half the remaining stubs were
blocked by things that have since been built and nobody re-checked.**

### Phase 0 — the free wins (no external gate, days not weeks) — **DONE**

| # | Stub | Was blocked by | Outcome |
|---|---|---|---|
| 1 | `node.peers/logs/blocks` | no chain access in the app | **live** as `node.status/blocks/activity` |
| 2 | `settings.tenancy` | `TenantHierarchy` not deployed | **live**; the contract has no root yet and the surface says so |
| 3 | `wallet.summary` | no wallet existed | **live**: address + exact balances, no invented history |
| 4 | `ledger.decision/correlation` | nothing — local reads over a chain we already hold | **live**, plus a Verify that really replays the chain and re-proves inclusion |

Verified on the packaged `.deb`, not on the gate: the rendered block hash,
proposer, peer count and client version were cross-checked against `cast`
against 40204, and the wallet balance matched `eth_getBalance` to the wei.

### Phase 1 — Rooms (QRM-S3) — **DONE 2026-07-26**

Ungated as of 2026-07-26 (owner supersession).

**The relay was never down.** This plan said `wss://comms.citrate.ai` was 502 on
`/`, `/health` and "on a WebSocket upgrade", and that it needed restarting before
Rooms could be built. It is WebSocket-only: a plain HTTP GET gets an empty reply
and Caddy correctly returns 502, and the "upgrade" probe had been sent over
HTTP/2, where upgrades do not exist. An HTTP/1.1 upgrade returns `101 Switching
Protocols` and the relay's CBOR challenge immediately. Two live tests — SIWE
login, KeyPackage publication, two-party channel create/join/message — pass
against it in 1.5 seconds. **Nothing was restarted.**

Shipped: a real MLS group on that relay, humans and agents as cryptographically
indistinguishable members, driven on the packaged `.deb`. The exit gate (two
humans + two agents; the relay reads nothing) passes as a test that walks the
relay's real on-disk store, with a negative control.

Two upstream changes were required first (citrate-comms PR #42): extracting
`comms-wire` + `comms-session` so a client need not link the relay server or a UI
toolkit, and **fixing a bug that limited any channel to two members** — adding
peers one at a time gave each joiner a different ratchet tree than its Welcome.
Every test upstream had added exactly one peer.

The operator's seat **is their wallet address**: `rooms_connect_intent` builds the
SIWE message, the SignatureCeremony signs it (citrate-core PR #89 made
`personal_sign` a real EIP-191 recoverable signature — owner sign-off, @rule8),
and the relay recovers the wallet. One approval per session.

**MR-4 is enforced** (2026-07-26): an agent seat is admitted only if a LIVE
capability grant clears it to the room's classification, the room opens or is
refused as a whole, and a room's classification never drops once open. The
operator's own clearance is not verified — `ClassificationRegistry` is deployed
but unread and `TenantHierarchy` has no root — so the room's classification is
their recorded declaration, and the surface says exactly that.

Not built, and named on the surface: voice/STT, durable transcripts, AgentSBT
attestation, and being ADDED to someone else's room (the KeyPackage binding attestation signs a BLAKE3 digest, and no
"sign an arbitrary digest" primitive exists — deliberately).

**The hpke-rs advisories are cleared** (citrate-comms PR #43, owner decision):
OpenMLS 0.6 → 0.8 puts `hpke-rs` at 0.6.1 and removes all four, including the 9.3
critical. It is a trade, not a sweep — `hpke-rs 0.6.1` pulls libcrux crates with
three high-severity advisories of their own that cannot be raised from here (they
need `hpke-rs >= 0.7`, which `openmls_rust_crypto` does not yet accept). The
relay was never exposed either way: it links `comms-core` without `mls`. Details
in `docs/audits/2026-07-26_hpke-rs-advisories-in-the-mls-stack.md`.

### Phase 2 — Governance (QRM-S6/S7)

The largest remaining piece, and the one the demo's first beat depends on.
Needs the governance contracts (TemplateRegistry, Factory, PolicyBinding,
VoteAllowance, Sortition) written and audited, then the authoring pipeline.
`MeetingRegistry` and `AnchorRegistry` are already done and prove the shape.

### Phase 3 — Calendar + Repos (QRM-S8)

Both blocked on **external** consent, not on us:
- **G3** Google Workspace + Microsoft Graph tenant admin consent — multi-week
  procurement at a customer. Start the request now; it is a classic demo-day
  blocker.
- **G6** confirm the customer's source-control host, then a GitHub App.

`session.current` also lands here (OIDC RP).

## The rule this plan is written under

A surface is "done" when it reads real data on the **packaged binary** and says
where the data came from. Not when the crate compiles, not when tests pass.
Every sprint here ends with `verify_meetings.py`-style drive on a real `.deb`,
because that is what has caught every serious defect in this repo so far.

## Immediate correction — done

The Rooms plate read *"gated on the G1 export-control legal opinion"*, stale as
of the supersession. Corrected in the Phase 0 PR: it now names the actual
blocker, the unreachable relay. A surface that misstates why it is empty is the
same defect class as one that fabricates data.
