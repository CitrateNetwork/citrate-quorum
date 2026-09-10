---
created: 2026-07-26T16:35:00Z
branch: feat/qrm-s3-rooms
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
sprint: QRM-S3
---

# Sprint QRM-S3 — Retrospective (Rooms)

> The sprint where the blocker turned out to be a curl command, the thing that
> *was* blocked was never in the plan, and a four-member room found a bug that
> had capped every channel in the federation at two.

## Outcome

| Field | Value |
|-------|-------|
| **Goal achieved?** | **Yes.** The planset gate — *"two humans + two agents in a room; relay reads nothing (proof test)"* — passes. Rooms is live on the packaged binary against the deployed relay. |
| **WPs closed** | S3.1 session + backend · S3.2 create/join/leave + KeyPackage · S3.3 agent seats · S3.4 transcript surfaced · S3.5 surface live · S3.6 server-blindness proof |
| **Carry-forward** | Voice/STT (WP5) · at-rest transcript encryption (WP4) · AgentSBT-attested seats · MR-4 classification-bounded admission · the wallet-bound human seat (needs a kit `personal_sign` path) |
| **Closing branch** | `feat/qrm-s3-rooms` → quorum PR; citrate-comms PR #42 (3 commits) upstream |

## Metrics delta

| Axis | Start | End | Δ |
|------|-------|-----|----|
| Rust tests (quorum) | 122 | **129** + 2 integration | +9 |
| Frontend tests | 46 | **51** | +5 |
| Upstream tests (comms-session) | 1 | **8** | +7 |
| Gate | 20/0/1 | **19/1/1** | the 1 fail is a real advisory, deliberately unsilenced |
| Live surfaces | 11 | **12** | +1 (Rooms) |
| Upstream crates | — | **+2** | comms-wire, comms-session |

## What worked

**The "what the plan said / what is actually true" table, written before any
code.** Three rows; two were wrong. The relay was not down, and the comms crates
were not consumable. Both would have been discovered anyway — one as a pleasant
surprise, one as a mid-sprint wall — but discovering them in the first hour is
what let the upstream extraction happen first instead of being improvised around.

**Going upstream instead of copying.** `NetSession` was inside a Slint app crate
and `RelayClient` inside the relay server crate. The tempting move was to
reimplement the session in quorum against `comms-core` — a few hundred lines,
no cross-repo PR, and a second implementation of MLS orchestration that would
have drifted from the one citrate-comms ships. Extracting `comms-wire` +
`comms-session` cost more up front and left the federation with one session
implementation, one wire protocol, and a test that fails if a client ever links
the server again.

**The exit gate found a real bug because it was specific.** "Two humans and two
agents" is four members. Every test in citrate-comms had added exactly one peer —
the single case where adding peers in a loop works. With two, the second joiner
fetches a ratchet tree from a later epoch than its Welcome and fails validation.
Any channel in the federation was capped at two members, and nobody knew. A
vaguer gate ("agents can chat") would have passed.

**Writing the negative control into the proof.** The server-blindness test plants
the secret in the relay's store and asserts the search finds it, then removes it.
Without that, a proof that walked the wrong directory would pass forever while
proving nothing — the exact failure mode this repo already has a journal about.

## What didn't work

**The custody design I scoped could not be built, and I found out late.** The
SCOPE opened with "the human seat's relay login is a ceremony", which is what the
planset wants. It cannot be done today: the relay recovers the SIWE signer from
an EIP-191 (keccak256, 65-byte recoverable) signature, and the kit's gated signer
produces a 64-byte non-recoverable ECDSA signature over a SHA-256 prehash. I had
already built the upstream two-phase login for it before checking what the kit's
signer actually emits. The two-phase API is right and stays; the sprint shipped
with relay identities that are honestly labelled as not being the chain identity,
and the SCOPE carries the amendment rather than being quietly rewritten.

The lesson is narrow and cheap: *check the shape of the signature, not just the
existence of a signer.* Both are "the ceremony can sign things" at a glance.

**Closed the same day, after the owner signed off** (citrate-core PR #89). The
fix was the one the kit's own header had named years of commits ago — "add a
recoverable message signer" — and it took an afternoon once someone needed it.
The operator's room seat is now their wallet address, approved once per session
through the ceremony. Worth noting what found the last bug in that chain: the
ceremony's "NOT RECORDED" plate, which reported `missing required key
signatureHex` instead of claiming success. The kit renames `sig_hex` to `sigHex`
on the wire; the DTO said `sig_hex`, so `undefined` travelled two calls before
failing. An honest failure plate is a debugger.

**`cargo fmt --all` reformatted a second repository.** Twice — once in
citrate-comms, where it swept 20 files I had not touched into my extraction
commit, and once in quorum, where path dependencies dragged citrate-comms into
the gate's formatting scope. The first was caught by reading `git status` before
pushing; the second by the gate going red. Both are the same trap, and this is
the second time it has cost time in this federation (citrate-core has it noted
too). The gate now formats exactly this workspace's own packages.

**A critical advisory was sitting under the crypto stack, invisible.** Linking
the MLS engine made `cargo audit` fail with four `hpke-rs` advisories, one 9.3
(nonce reuse in HPKE context). It was there before this sprint, in the deployed
relay and the shipped client — it simply was not in any dependency graph that a
gate ran against. Nothing about that is comfortable, and it is written up rather
than suppressed.

## What surprised us

**The relay had been "down" for as long as anyone had been asking it for a web
page.** Journal: `2026-07-26T1630_the-blocker-that-was-a-curl-command.md`.

**Deleting most of the Rooms surface made it more convincing.** The port had a
push-to-talk button, an audio-consent gate, a frozen agenda with a "✓ verified"
hash, a live delegated vote with weights and proofs, a contradiction between two
attested sources, and draft minutes. None of it existed. What replaced it — a
room, a roster where each seat shows its own MLS key, a transcript, a composer —
is smaller and is the first version an operator could believe.

**Three MLS keys on screen is a better proof than any sentence about
encryption.** The roster renders each seat's signature public key. Two agents and
a human are visibly three different cryptographic members, on a screenshot, on
the packaged binary. That is the whole "agents are members, not wiretaps" claim,
rendered.

## Decisions ratified mid-sprint

- **A room identity is not the chain identity**, and the surface says so rather
  than implying a binding we did not make.
- **An agent's seat key is held by the app.** The planset already specified this;
  writing it into the doc comments and the roster's own labelling is what stops
  it being read as "agents hold keys now".
- **A batch add is one Commit.** Not an optimisation — the per-peer loop is
  incorrect, and the upstream fix says so in the code.
- **A real advisory keeps the gate red.** No `audit.toml` ignore. A suppressed
  advisory is invisible the day someone can act on it.
- **The transcript is not persisted, and the surface says that too.** Writing
  plaintext room transcripts into `evidence/` would be worse than keeping none.

## Action items for the next slice

- [ ] **Decide the hpke-rs advisories** (upgrade OpenMLS 0.6→0.8 in citrate-comms
      / assess reachability / accept with an expiry). Recommended: upgrade.
- [x] **MR-4 classification-bounded admission** — done 2026-07-26, immediately
      after this retro. An agent seat needs a live grant clearing it to the
      room's classification; the room opens or is refused as a whole; the
      operator's clearance is still unverified and the surface says so. The rule
      is a pure function with its own tests, and it was proved on the packaged
      binary by trying to open a room with two ungranted agents.
- [ ] A kit `personal_sign` path (EIP-191, recoverable) so a human's room seat
      can be their wallet — @rule8, needs security sign-off.
- [ ] Durable MLS state, so a rejoined seat is the same member rather than a new
      one with the same label.
- [ ] Record room lifecycle (open/join/leave) into the evidence chain with a
      correlation id, so a room is governed the way a meeting is.
