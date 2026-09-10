---
created: 2026-07-26T00:30:00Z
branch: docs/qrm-s5-retro-journals
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# Sprint QRM-S5 — Retrospective (meetings + minutes)

> **Amendment, 2026-07-26 — the gate is now fully met.** Everything below is
> what was true when this sprint closed, and it is left standing: at close the
> exit gate was **half** met, and saying so was the point. The other half was
> closed afterwards by QRM-S6 work. See [§ Gate closed](#gate-closed-2026-07-26)
> at the end. Nothing above that section has been rewritten — a retrospective
> edited to look better in hindsight is worth nothing, and this one is the
> record of a decision to report a shortfall rather than dress it.

> The sprint where a meeting became a governed object — and where wiring a
> surface turned out to mean auditing everything it already claimed. Nine
> sentences on screen were false. One of them was in the signing dialog.

## Outcome

| Field | Value |
|-------|-------|
| **Goal achieved?** | **Half, at close.** The exit gate is *"a real internal Citrate standup runs in Quorum and produces ratified, **anchored** minutes."* Ratified: yes, end to end on the packaged binary. Anchored: **no**, and it was never reachable this sprint. **Closed 2026-07-26** — see the amendment at the end. |
| **WPs planned / closed** | S5.1 domain · S5.2 agenda from sprint files · S5.3 durable store · S5.4 minutes + ratification · S5.5 commands + bridge · S5.6 anchor honesty · S5.7 standup briefs — all closed |
| **Carry-forward** | ~~Anchoring (needs `AnchorRegistry` deployed + `MeetingRegistry` written, QRM-S6)~~ — **done 2026-07-26**; minutes from a live transcript (Rooms, G1-gated) — still open; AgentSBT-attested attendance — still open |
| **Closing branch** | `main` at `73722c1` (PR #25) |

## The gate is half met, and the SCOPE said so on day one

`AnchorRegistry` exists in `citrate-chain` source but is not in the frozen
40204 book — it was never redeployed after the 2026-07-23 reroll.
`MeetingRegistry` is not written at all; it is a QRM-S6 deliverable. Both were
checked before the first line of code, written into `SCOPE.md`, and named in
every PR.

That is the part worth keeping. The failure mode this sprint could have had was
not "we did not anchor" — it was "we shipped something that *looked* anchored".
Which nearly happened: see below.

## Metrics delta

| Axis | Start | End | Δ |
|------|-------|-----|----|
| Rust tests | 178 | **223** | +45 |
| Frontend tests | 24 | **36** | +12 |
| Gate checks | 20 | **20** | 0 (one swapped: a hand-rolled tripwire → eslint) |
| Live surfaces | 5 | **7** | +2 (Meetings, Journal) |
| Backend crates | 8 | **9** | +1 (`quorum-meetings`) |
| False claims removed from surfaces | — | **9** | — |

## What worked

**Checking the dependency before writing the SCOPE.** Five minutes of reading
`citrate-chain` established that half the exit gate was unreachable. Every
document since has said "half met" rather than discovering it at close and
negotiating the wording. A gate you have measured against reality up front is a
gate you can report honestly at the end.

**Driving the real flow on the packaged binary, and reading assertions off
disk.** `scripts/verify_meetings.py` schedules, attests, opens, closes and
ratifies on a real `.deb`, and every check reads `meetings.json` or
`chain.jsonl`. It found things no test could: that the agenda really is
generated from this repo's own sprint files (7 items), and that the frozen hash
`0xba245332e4c9…` is **the same value** computed independently from the Rust
side — the offline-verifiability property demonstrated rather than asserted.

**Writing the hash rules as tests before trusting them.** Length-prefixed
encoding, so `"ab"+"c"` and `"a"+"bc"` cannot collide. The store keeps the
agenda's *items and* its frozen hash, so an agenda edited on disk under a
ratified meeting refuses to load — proven by rewriting the JSON in a test
rather than by reasoning about it.

**Making the parser refuse to guess.** Unparseable rows are counted in
`Agenda.skipped`, not dropped. A missing tree yields an empty agenda that says
it was *not generated*. This mattered immediately — see below.

## What didn't work

**Nine sentences on the surfaces were false, and I found them by looking at the
app, not the code.** In order: a ceremony row hardcoding "1 decision · 1 dissent
recorded"; a ceremony Effect promising "anchors the minutes on chain"; a draft
banner repeating it; a list footer sourcing rows to "MinutesRegistry + relay
archive"; the detail panel claiming "MinutesRegistry" *after* I had fixed the
footer; "A changed agenda creates a linked successor meeting" describing a
feature that exists nowhere; the Journal claiming a "local memory graph" with
"AgentSBT-signed" entries; a hardcoded `brief("claude-code", "m-0723")` naming
fixture ids that exist in no tenant; and the anchor line rendering **only when
truthy**, so ratified-but-unanchored looked identical to anchored.

That last one is the sprint's own R-A risk — "the gate reads as met when it is
half met" — already sitting in the code before I wrote the sentence warning
about it.

**My own risk register landed on me within the hour.** R-C said a parser might
invent plausible agenda items from a malformed file. `is_risk_id` accepted any
short alphanumeric after `R`, which matches the literal word **"Risk"**, so a
table headed `| Risk | Mitigation |` produced the agenda line *"Risk Risk —
Mitigation"*. Caught by a test, in the function written to prevent it.

**A verification step that failed silently and reported green.** Checking S5.7,
I posted an agent intent with `"class"` where the bridge expects `"tool"`. It
was rejected; I did not check the response; the brief then read "no agent has
acted in this tenant yet" — a **true sentence about a state my own verification
had created**. A green run proving nothing. The script asserts the status now.

**The UI driver could not type a colon.** `type_text` mapped four punctuation
characters and silently dropped the rest, so an RFC3339 timestamp arrived as
`2026-07-30T09` and a filesystem path as `""`, while reporting a successful
click over a field that never received the text. A harness that manufactures
false passes is worse than no harness.

## What surprised us

**The signing dialog was lying, and the fix was already written elsewhere in
the same file.** `ceremony.request()` resolves when the operator *dismisses*
the dialog, not when it settles — so the order was: Sign → "On record ·
recorded against your name in this tenant's evidence chain" → Close → *then*
the write. I verified on the binary that `meetings.json` read `awaiting` and
the chain was empty at the moment that sentence was on screen.

Worse, `Agents.revokeGrant` ran its revoke with `.catch(() => {})`. A failed
revocation was discarded in silence while the operator read "the capability is
gone at the next checkpoint". `killAll` had the same shape across every grant.

And answering an escalation **already committed correctly**, inside `onSign`,
with a comment saying *"a stamp that can be wrong is worse than no stamp"*.
Someone — me, in S4 — had fixed exactly this class for one path and not
generalised it. The bug was not a missing idea. It was an idea applied once.

**Honesty work compounds.** Every fix to a false claim made the next one
easier to see, because the surface stopped being uniformly confident. By the
end, the Meetings register said what it was, what it wasn't, and where its rows
came from — and the remaining problems stood out against that.

## Decisions ratified mid-sprint

- **A gate that is half met is reported half met**, in the SCOPE, in every PR,
  and here. Not "met with caveats".
- **The workspace is a tenant fact, not a meeting fact.** The same directory
  feeds every agenda and every brief.
- **Authorship is never guessed.** The author line names a model *and* the
  person directing it; `human` stays undecided and renders verbatim. The bridge
  maps `null` to `undefined`, not `false` — "undecided" and "not a human" are
  different claims, and this is an attributed governance record.
- **A brief for an agent with no journals says so.** §3.2 requires briefs
  grounded in artifacts; a generated one is the exact failure that sentence
  guards against.
- **Every brief names its own gaps.** One that omitted the omission would read
  as complete (R5, applied to briefs rather than to enforcement).
- **The ceremony may only claim what happened.** `settledNote()` is a pure,
  tested function whose most important test asserts the *negative*.

## Action items for the next sprint

- [ ] Anchoring: deploy `AnchorRegistry` to 40204 and write `MeetingRegistry`
      (QRM-S6) — this is what closes the other half of the S5 gate.
- [ ] Re-check the remaining ceremony callers (`Wallet`, `Governance`) once
      their domains are live; both are sim-only today and correctly report
      that the ceremony recorded nothing.
- [ ] Demonstrate egress denial on the packaged binary. The S4 retro named "a
      more robust UI driver" as the blocker; the driver now types punctuation
      and raises on characters it cannot send, so this is unblocked.
- [ ] A meeting cannot yet be *deleted or amended* — decide whether an
      amendment is a successor meeting (the phrase the UI used to claim) or a
      new version, and make the answer real rather than a caption.

## Gate closed (2026-07-26)

The half that was unreachable at close is now delivered. Recorded here rather
than by editing the sections above, because **when** something became true is
part of the record.

### What was missing, and what closed it

| Blocker at close | Closed by |
|---|---|
| `AnchorRegistry` not in the frozen 40204 book | Deployed + booked (citrate-chain #97) |
| `MeetingRegistry` did not exist in source | Written, deployed, booked (citrate-chain #100) |
| The app had no address book | Vendored + resolved at runtime (quorum #27) |
| The app had no wallet — `wallet::create` had **zero** production callers, here or in citrate-core | Wallet create/import surface (quorum #28) |
| The app had no way to unlock a custody vault | Vault surface (quorum #28) |
| The ceremony never called a signer — the kit's `sign_*` commands were registered and never invoked | The ceremony's chain leg (quorum #28) |

The last three were not visible from inside this sprint. Each was only exposed
by finishing the one before it: there was no point wanting a signature until
there was a key, and no point wanting a key until there was a vault to seal it
into. The retro's carry-forward said "anchoring"; anchoring turned out to be
six things.

### The evidence

A meeting ratified through the packaged binary, its commitment registered on
chain by the ratifier's own key:

```
isRegistered  true
agendaHash    0x340dcce5…   the agenda frozen at open
minutesHash   0xe153449f…
ratifier      0xeCCAB07ca8bc9A8cab3DA7098ed32D41B09927E2
block         133808
```

`scripts/verify_meetings.py` asserts this on every run (17/17), so a regression
that silently stops signing fails the run instead of passing quietly.

### What is still NOT claimed

The gate said "ratified, anchored minutes" and that is met. Two things this
retro named as out of scope remain out of scope, and closing the gate does not
quietly absorb them:

- **Minutes composed from a live transcript.** Still composed from the governed
  record, not a room transcript. Rooms is QRM-S3, gated on G1.
- **Attendance attested by AgentSBT proof.** Still an operator's attestation.

### The lesson this adds

Reporting the gate half met cost nothing and bought the ability to say *this*
without hedging. Had it been recorded as "met, with a caveat" at close, there
would be no clean line between the day it was half true and the day it was
whole — and the six blockers above would have been discovered as surprises
rather than as a list someone had already written down.

## Notes

The single most important artifact is `src-tauri/crates/quorum-meetings/` —
`lib.rs` for the four rules and `agenda_source.rs` for a parser that refuses to
guess. The second is `scripts/verify_meetings.py`, which is the only reason
this retro can say "ratified works" without hedging.

Related journals: `2026-07-26T0015_nine-sentences-that-were-not-true.md`,
`2026-07-26T0020_the-idea-applied-once.md`,
`2026-07-26T0025_the-verification-that-proved-nothing.md`.
Related essay: `2026-07-26T0030_half-a-gate-is-not-a-gate.md`.
