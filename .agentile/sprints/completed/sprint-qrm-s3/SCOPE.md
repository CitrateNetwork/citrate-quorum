---
created: 2026-07-26T15:40:00Z
branch: feat/qrm-s3-rooms
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
sprint: QRM-S3
---

# Sprint QRM-S3 — Rooms (Phase 1 of the completion planset)

> **Sprint goal.** A room is a real MLS group on the citrate-comms relay, with
> humans and agents as cryptographically indistinguishable members, and the relay
> able to prove it read nothing.

Executed against the federation planset (`plan/quorum-s0`,
`.agentile/planset/2026-07-22-citrate-quorum/`, not in this checkout). The
normative model is `02_ARCHITECTURE.md` §3; the sprint line is
`05_SCOPE_AND_SPRINTS.md` §3 (QRM-S3) and its WP list in §4.

Planset exit gate: *"Two humans + two agents in a room; relay reads nothing
(proof test)."*

## Measured against reality before writing this

The S5 retro's rule: check the dependencies before scoping, so the honest report
is written down before it is inconvenient.

| What the plan said | What is actually true |
|---|---|
| **G1 export-control legal opinion blocks S3** | Superseded by the owner 2026-07-26 (`docs/decisions/2026-07-26_g1-superseded-for-rooms.md`). The compliance-CLAIM prohibition stands and nothing here claims export-control compliance. |
| **`wss://comms.citrate.ai` is DOWN (502) — restart it first; BLOCKED / NEEDS LARRY** | **The relay is up and always was.** It is WebSocket-only, so a plain HTTP `GET /` gets an empty reply and Caddy correctly reports 502. An HTTP/1.1 upgrade returns `101 Switching Protocols` + the relay's CBOR challenge. `comms-session`'s two live tests — SIWE login, KeyPackage publication, two-party channel create/join/message — pass against it in 1.5s. Nothing was restarted. |
| `comms-client`, `comms-core`, `comms-proto` exist and can be consumed | They exist. `comms-client` could **not** be consumed: `NetSession` lived inside a Slint app crate, and `RelayClient` inside the relay server crate (RocksDB + axum + OS keyring). Fixed upstream first — citrate-comms PR #42 extracts `comms-wire` + `comms-session`. |

So the gate is reachable this sprint. What is *not* reachable is stated below
rather than discovered at close.

## The two custody decisions this sprint rests on

**1. A room identity is not the chain identity.** ⚠️ *Amended mid-sprint — the
original text is below, because a scope that is quietly rewritten to match what
got built is not a scope.*

Each seat gets its own secp256k1 key, generated in-process and sealed in the
custody vault under `room-identity/`. It signs exactly two things, both to the
relay: the SIWE login and the attestation binding its MLS public key to itself.
It authorises nothing on chain. The vault's *wallet* — the key that ratifies
minutes — is not used by `rooms.rs` at all, and two source-scan tests assert it.

> **What this section said at kickoff:** *"The human seat's relay login is a
> ceremony. Both signatures go through the `SignatureCeremony`
> (`IntentKind::PersonalSign`) and are recorded as governed acts."* That is what
> the planset wants (§3: a human seat bound to their wallet address,
> ceremony-gated) and it is what the upstream two-phase login was built for.
>
> **Why it is not what shipped:** the relay authenticates SIWE by recovering the
> signer from an EIP-191 signature — keccak256, 65 bytes, recoverable. The kit's
> gated signer produces a 64-byte NON-recoverable ECDSA signature over a SHA-256
> prehash. They are not interchangeable, so a ceremony approval cannot currently
> produce a signature the relay will accept. Fixing that means adding a real
> `personal_sign` path to the kit's ceremony — @rule8 code that needs security
> sign-off, not a drive-by at the end of this sprint.
>
> The consequence, stated on the surface and in the code: a room roster labels
> each seat with the principal it belongs to and says the key is a relay
> identity. It does not claim the seat *is* the wallet, because we did not make
> that binding. The two-phase upstream API is in place and waiting for the kit.
>
> **CLOSED the same day.** The owner signed off on the kit change
> (citrate-core PR #89): `wallet::sign_personal` is a real EIP-191 recoverable
> signature, gated exactly like the other two signers. The operator's seat now
> authenticates as **their own wallet address** — `rooms_connect_intent` builds
> the SIWE message and hands it to the SignatureCeremony, the human approves the
> EIP-4361 text itself, and the relay recovers the wallet. One approval per
> session; MLS signs every message after that with the member key.
>
> What is still not wallet-bound is narrower and stated on the surface: the
> KeyPackage binding attestation signs a BLAKE3 digest, and the kit deliberately
> exposes no "sign an arbitrary 32-byte digest" primitive — one would sign a
> transaction hash just as happily. So the operator's seat can OWN rooms but
> cannot be ADDED to someone else's. Closing that is a citrate-comms protocol
> change (an EIP-191 binding attestation), not something to fake here.

**2. An agent's room identity is held by the app, never by the agent.** The
planset is explicit (§3, custody column): an agent member holds a *"per-agent MLS
key in the local keyring; **never** a chain-signing key"*. So quorum generates a
room identity per agent seat, seals it in the custody vault, and signs that
seat's SIWE handshake locally without a ceremony — because it authorises nothing
on chain and confers no authority. The agent process still holds no key (I-1),
still cannot sign (rule 3), and every *action* it takes in the room still passes
the policy gate (rule 4). What it gains is a cryptographic seat: the relay cannot
tell an agent member from a human one, which is the property the product claims.

## Work packages

| WP | Deliverable | Acceptance |
|---|---|---|
| **S3.1** | Consume `comms-session`; `rooms.rs` backend with a ceremony-gated human seat | The relay authenticates the operator's vault address; the two signatures appear in the evidence chain as governed acts |
| **S3.2** | Room create / join / leave + KeyPackage publication | A room created on one seat is joined from a Welcome by another, against the live relay |
| **S3.3** | Agent seats — app-held room identity per agent, sealed in custody | Four members in one room: two human-identity, two agent-identity |
| **S3.4** | Transcript: decrypted events surfaced + room lifecycle recorded as evidence | Room open/join/leave are decision records with a correlation id; messages are surfaced, not persisted (see below) |
| **S3.5** | The Rooms surface reads live: list, roster, transcript stream | The packaged binary shows a real room with real members and real decrypted messages |
| **S3.6** | **Server-blindness proof test** (planset WP6) | Four members exchange a known transcript; the relay's stored bytes contain no plaintext token from it |

## Explicitly NOT in this sprint

Named here so nothing is discovered at close:

- **Voice / `stt-worker` / push-to-talk (planset WP5).** No audio path at all.
  R14 (consent across jurisdictions) and Q15 (transcript-only default) apply the
  moment one exists, and neither is answered by writing a sidecar.
- **Encrypted transcript persistence at rest (planset WP4).** Messages are held
  in memory for the session and surfaced; they are not written to disk. Writing
  plaintext transcripts into `evidence/` would be worse than not persisting them,
  and doing it properly is the ENCRYPT program's at-rest work (R13). The surface
  says so rather than implying a durable archive.
- **AgentSBT-attested membership.** `AgentSBT` is deployed (`0xc2f388aa…`) but
  binding a room seat to an on-chain SBT — and verifying the `pubkey_fingerprint`
  — is its own design question. Seats carry the agent id this tenant already has
  evidence about; they do not claim an on-chain attestation they have not
  checked.
- **Presence, turn-taking, speaking order, mute/yield/force-call.** The room
  works; it is not choreographed.
- **Cross-org / A2A rooms** (planset §3.3) — sequenced last federation-wide.
- **MR-4 classification monotonicity enforcement in-room.** The classification
  ladder exists in the domain; a room carries a classification, but admission is
  not yet bounded by the lowest attested clearance. This is a **requirement, not
  an option** (the G1 supersession says so explicitly), and it is the first item
  of the next slice.

## Found while scoping: four hpke-rs advisories under the MLS stack

Linking the MLS engine made `cargo audit` fail — **not** because of anything in
this sprint, but because the exposure was previously invisible to a gate that
runs. `openmls 0.6` pins `hpke-rs 0.2.0`, which carries four advisories including
a **9.3 critical nonce reuse in HPKE context**. That is the primitive MLS uses to
seal a group secret to a joiner, and it sits under the confidentiality claim this
sprint's exit gate is about.

It is not silenced: `cargo audit` fails visibly and the write-up is
`docs/audits/2026-07-26_hpke-rs-advisories-in-the-mls-stack.md`. The fix is an
OpenMLS 0.6 → 0.8 upgrade in citrate-comms, which is its own work package
touching a deployed relay — an owner decision, recorded there.

## Risks

| # | Risk | Mitigation |
|---|---|---|
| R-A | The relay is a liveness single point (planset R10) | Room state is reconstructible from local MLS state; the surface reports a lost connection honestly rather than showing a stale roster |
| R-B | A fresh `MlsMember` per session means a rejoined seat is a *new* member | Stated on the surface; durable MLS state is a follow-up, not a silent assumption |
| R-C | "Two agents in a room" could be read as two agent *processes* holding keys | The custody decision above is written into the code's doc comments and into the roster's own labelling |
| R-D | A transcript held only in memory looks like a durable archive | The surface says what it keeps and for how long |
