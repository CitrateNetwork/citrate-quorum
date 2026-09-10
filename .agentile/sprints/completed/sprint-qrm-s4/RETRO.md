---
created: 2026-07-25T05:24:00Z
branch: docs/qrm-s4-retro-journals
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# Sprint QRM-S4 — Retrospective (agent adapter layer)

> The sprint where the governance stopped being a library and started being a
> control — and where running the packaged application, rather than testing it,
> found nine real bugs that tests and the type-checker could not.

## Scope note

This retro covers the agent adapter layer (PRs #14–#19) plus the enabling work
it turned out to require (#10–#13). They are recorded as one sprint because the
adapters are worthless without the things underneath them: a reachable gate, a
durable chain, a budget that depletes, and a human who can answer.

## Outcome

| Field | Value |
|-------|-------|
| **Goal achieved?** | YES. The full HIC-1 loop runs on the packaged binary, across four transports. |
| **WPs planned / closed** | S4.1 bridge · S4.2 escalation queue · S4.3 grant issuance · S4a Claude Code · S4a MCP stdio + egress · S4a HTTP + Codex — all closed |
| **Carry-forward** | The `EgressPolicy` protocol (chain-gated), a Hermes in-process hook (other repo), operator-clearance-vs-ceiling (post-reroll) |
| **Closing branch** | `main` at `297a56f` (PR #19) |

## Metrics delta

| Axis | Start | End | Δ |
|------|-------|-----|----|
| Rust tests | 79 | **177** | +98 |
| Frontend tests | 0 | **21** | +21 |
| Gate checks | 18 | **20** | +2 |
| Live Tauri commands | 13 | 21 | +8 |
| Adapters | 0 | 5 paths | +5 |
| Bugs found by running the binary | — | **9** | — |

## What worked

**Running the packaged application as a first-class verification step.** This is
the sprint's real finding and it deserves the top slot. Every slice was
exercised against a real `.deb` from a clean install — headless under Xvfb,
driven with synthetic X input, screenshots read back, and the agent path
exercised over the actual loopback socket. It found, in order: a white screen on
eleven of twelve surfaces; a Dashboard that showed "Governed 0" with a decision
already on the chain; a modelling error that recorded operators as ungoverned; an
approval path that could never work and failed silently while stamping "On
record"; a `<select>` an operator could not read; and an accountable-principal
that came from the agent's own claim. None of these were catchable by `cargo
test` or `tsc`. Several were *invisible in the sim adapter* — they only existed
on the real one.

**Adversarial self-review before merge, twice with a real catch.** Reviewing my
own branch by asking "what is this change most likely to have broken?" rather
than re-reading it found a `useMemo` below an early return in two surfaces
(React renders fewer hooks and crashes, only on the failing path), and a
`decision_status` heuristic that would tell an agent "approved" for an action a
human had explicitly **rejected**. The second is the worst bug of the sprint and
it was mine, written an hour earlier.

**Reading the protocol instead of remembering it.** MCP `2026-07-28` — the
largest revision since launch — shipped four days after the proxy was written. A
proxy built from memory would have broken on release day. The design answer was
to make the proxy *decline to have an opinion*: it inspects one method whose
shape is stable across every revision and relays everything else byte-for-byte.

**Turning each bug class into a gate check.** `no fabricated stats in surfaces`
and `no hooks after early return` both exist because something got past. Both
were negative-controlled in each direction before being trusted.

## What didn't work

**I declared S2D's exit gate met when it was not.** S2D.4 required "no surface
renders fabricated data" and S2D.6 required all seven states per surface. The
packaged app was showing "1,204 actions recorded" and "62% budget burn" against
an empty tenant. The check that was supposed to catch this greps for fixtures by
*name*, so it structurally could not see literals inline in a surface, and it was
green the whole time. A gate you have not tried to defeat is a gate you have not
tested.

**I wrote off a real bug as cosmetic noise, twice.** The duplicate-React-key
warning was dismissed as "pre-existing sim noise" in two separate PR write-ups.
It was a genuine merge bug — the same decision rendered twice when it arrived
from both the query and the stream. Deduping on id fixed it and the console went
silent. "Pre-existing" is a statement about *when*, not about *whether*.

**My own tripwire shipped broken on the first attempt.** The
hooks-after-early-return checker's indentation regex let `\s*` swallow deeper
indents, so it flagged returns nested inside callbacks and produced eleven false
positives. A check that cries wolf gets switched off, which is worse than no
check. Caught only because I ran the negative control.

**The test harness, not the product, blocked one acceptance.** Egress denial
in-action could not be demonstrated on the binary because driving a native
`<select>` plus a reflowing form with synthetic X input proved unreliable. It is
proven by tests, and that gap is stated in the PR rather than papered over — but
it is a gap.

## What surprised us

**Statelessness in MCP 2026-07-28 made the governance *easier*, not harder.**
Removing protocol-level sessions means each `tools/call` is independently
gateable and the proxy holds no per-connection state it could get wrong. Two of
the new `_meta` fields turned out to be exactly what the evidence needed:
`io.modelcontextprotocol/clientInfo` names the calling agent, so the ledger
records *who called* rather than what we were configured to assume, and
`traceparent` becomes the correlation id, lining a governed decision up with the
same trace an OpenTelemetry backend sees. A spec revision designed for load
balancers happened to be designed for governance intermediaries too.

**The bugs clustered at the seams between honest components.** Every part was
individually correct and tested. The gate worked; the queue worked; the ceremony
worked. What failed was the ceremony calling the queue with an approver that no
component was responsible for producing, and then swallowing the refusal. Unit
tests cannot see a gap that exists *between* two things that each pass.

## Decisions ratified mid-sprint

- **A human acting directly is not an agent.** Operators hold no grant, so
  running them through the agent gate returned `ungoverned` — and HIC-X is "an
  alert state, never a configuration". Human-origin intents skip the agent gate
  and record as `Approved` at HIC-1, naming the human. `runGate`'s `origin` is
  required and undefaulted so forgetting it is a compile error, not a silent
  bypass.
- **The accountable human comes from the grant, not the agent.** An agent
  reporting its own principal is authority by assertion.
- **Every governed action charges `max(1, cost)`.** If some actions were free an
  agent could run unattended forever inside a "budgeted" envelope.
- **Escalation consumes; rejection refunds.** An escalation that cost nothing
  would let an agent queue unlimited approvals.
- **`Approved` and `Rejected` are distinct from `Allow` and `Deny`.** "A person
  decided" and "the rules decided" are different facts about an organisation,
  and showing which one happened is the entire point of HIC-1.
- **No fail-open switch in the adapter.** An escape hatch on a control is the
  first thing an attacker looks for and the thing a hurried operator turns on
  permanently. Not installed = stand aside; installed but unreachable = refuse.

## Action items for the next sprint

- [ ] Demonstrate egress denial on the packaged binary — needs either a more
      robust UI driver or a non-UI path to issue a CUI-ceiling grant.
- [ ] Check the operator's own clearance against the ceiling they grant
      (post-reroll; needs the live `ClearanceReader`).
- [ ] Revisit every "pre-existing" dismissal in this repo's PR history. The one
      I made twice was a real bug.
- [ ] Adopt eslint and delete `scripts/check_hooks_after_return.py`, which says
      so in its own docstring.

## Notes

The single most important artifact is `src-tauri/crates/quorum-adapter/` — read
`lib.rs` for the shared gate and `mcp.rs` for a proxy deliberately built to
survive a protocol revision it was written four days before. The second is
`docs/ADAPTERS.md`, whose coverage table ends on the row that matters: *an agent
run outside an adapter is ungoverned by construction.* Risk R5 warns against
mistaking binding enforcement for total coverage, and a table without that row
would do exactly that.

Related journals: `2026-07-25T0530_running-it-is-the-test.md`,
`2026-07-25T0535_the-half-built-control.md`,
`2026-07-25T0540_a-proxy-that-declines-to-have-an-opinion.md`.
