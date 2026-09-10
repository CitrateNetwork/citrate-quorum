---
created: 2026-07-25T17:40:00Z
branch: feat/qrm-s5-meetings
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
sprint: QRM-S5
---

# Sprint QRM-S5 — Meetings + minutes

> **Sprint goal.** Make a meeting a governed object: an agenda generated from
> real sprint files and frozen at open, attendance that is attested rather than
> asserted, minutes composed from the governed record, and a ratification that
> is a human signature over a hash — not a button that sets a flag.

Executed against the federation planset (`plan/quorum-s0`,
`.agentile/planset/2026-07-22-citrate-quorum/`, not in this checkout). The
normative lifecycle is `02_ARCHITECTURE.md` §3.1; brief sources are §3.2.

The planset's S5 exit gate: *"a real internal Citrate standup runs in Quorum and
produces ratified, anchored minutes."*

## The gate splits — say so up front

**`ratified` is reachable this sprint. `anchored` is not.** Two dependencies sit
outside this repo:

| Needed for | Contract | Status |
|---|---|---|
| minutes hash on chain | `AnchorRegistry` | Exists in `citrate-chain/contracts/src/cit_agent/`, **not in the frozen 40204 book** — never redeployed after the 2026-07-23 reroll |
| the meeting register | `MeetingRegistry` | **Not written.** It is a QRM-S6 deliverable (`05_SCOPE_AND_SPRINTS.md`) |

So S5 delivers the full local spine and an anchor that states honestly that it
is unavailable and why (Rule 1, Rule 8). It never renders a fabricated block
number or root. Closing the second half of the exit gate is a QRM-S6 + deploy
task, and this sprint's retro must record the gate as **half met**, not met.

Two further halves are gated elsewhere and are explicitly *not* claimed here:

- **Minutes from a live transcript.** §3.1 has the Notetaker composing minutes
  from the room transcript. Rooms are QRM-S3, gated on the G1 export-control
  legal opinion. S5 composes minutes from the *governed record* — the tenant's
  evidence chain — which is real and verifiable but is not the transcript.
- **Agent-attested attendance via AgentSBT proofs.** No SBT verification path
  exists yet. An agent's attendance is attested by its decision records in the
  meeting window — evidence it acted, not a signature that it was present.

## Work packages

| WP | What | Acceptance |
|----|------|-----------|
| **S5.1** | `quorum-meetings` crate — lifecycle state machine, agenda freeze + BLAKE3 `agendaHash`, MR-4 classification monotonicity, quorum rule | A meeting cannot leave `scheduled` without a frozen agenda; a mutation after freeze is an error, not a silent overwrite; classification above the lowest attested clearance is refused |
| **S5.2** | Agenda generated from real sprint files (`.agentile/sprints/active/*/SCOPE.md`) | Agenda items carry the file they came from as `src`; no source configured yields an empty agenda with an honest reason, never invented items |
| **S5.3** | Durable per-tenant meeting store, keyed like evidence (`blake3(tenant)`) | Meetings survive the app exiting; a ratified meeting reloads ratified |
| **S5.4** | Minutes composed from the governed record + ratification through the `SignatureCeremony` | Ratification is a HIC-1 act with its own decision record; a ratified meeting is immutable; an inquorate meeting cannot be ratified |
| **S5.5** | Tauri commands + `bridge/tauri` wiring; the Meetings surface goes live | `meetings.list()`/`get()` stop throwing `Unavailable`; an empty tenant shows an honest empty state, not a fabricated register |
| **S5.6** | Anchor honesty | The anchor field resolves `AnchorRegistry` from the frozen address book at runtime; absent code = an honest unavailable state naming the reason |
| **S5.7** | Standup briefs from journals + retros (§3.2) | A brief cites the artifact it came from; an agent with no journals produces an empty brief, not a generated one |

## Out of scope (named, not silently dropped)

Calendar sync (S8) · IPFS `minutesCID` (self-hosted Kubo, WO decision) ·
`MeetingRegistry` + `AnchorRegistry` writes (S6 + deploy) · live transcript and
the in-room Notetaker (S3, G1-gated) · votes and sortition (S6) · two-way
external calendar conflict resolution (S8).

## Risks

- **R-A · The gate reads as met when it is half met.** The demo's fourth beat is
  "ratified, anchored minutes"; ratified alone looks identical on screen until
  you read the anchor row. Mitigation: the anchor state is rendered, not hidden,
  and the retro records the gate as half met.
- **R-B · Minutes composed from the evidence chain get mistaken for a
  transcript.** They are a different, narrower claim. Mitigation: every minutes
  block names its source (Rule 11).
- **R-C · Agenda generation invents structure.** A parser that guesses at
  malformed sprint files produces plausible agenda items from nothing.
  Mitigation: parse conservatively and drop what does not match, with the count
  of dropped lines surfaced rather than swallowed.
