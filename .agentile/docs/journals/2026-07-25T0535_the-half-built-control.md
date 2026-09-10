---
created: 2026-07-25T05:35:00Z
branch: docs/qrm-s4-retro-journals
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# The half-built control

> The gate recorded questions that nobody was ever asked. Then, once someone
> could be asked, their answer silently failed to record while the screen said
> "On record".

## Context

QRM-S4.1 shipped the keyless agent bridge: an agent submits an unsigned intent,
the policy gate rules on it, the verdict is recorded, and the agent is told
whether it may proceed. `require-approval` came back with a decision id. The PR
was honest, the tests passed, the packaged binary proved it end to end.

It was also, on inspection, a control that did nothing for the case it existed
to handle. An agent that escalated stopped and waited — forever. No queue entry,
no surface, no way to answer. The question was recorded and never asked.

## What happened

S4.2 built the human half: a durable pending queue, `Verdict::Approved`
alongside `Rejected`, `GET /decision/{id}` so a blocked agent learns the answer,
and a second entry into the ceremony that reviews an existing decision rather
than gating a new one.

Then two things went wrong in a way I have started to recognise.

**First, my own review caught a bug worse than the missing feature.**
`decision_status` identified the record that answered an escalation by matching
action class plus correlation id. Two escalations from the same agent for the
same tool are identical on both. Approve one, reject the other, and the rejected
one reports **"approved"** — an agent told it may proceed on an action a human
had explicitly refused. I wrote it and I caught it an hour later by asking "can
this API ever return the wrong answer?" rather than by re-reading it.

**Second, the packaged app proved the whole path dead.** The approver came from
`session.current()`, which needs an IdP. There is no IdP. So `approve(id, "")`
was refused by the backend — correctly, since I-4 requires an approval to name a
human — and the frontend's `.catch(() => {})` swallowed the refusal while the
ceremony went on to stamp **"On record"**. The operator saw success. The agent
stayed blocked. The queue never cleared. Everything reported fine.

Both fixes were structural rather than local. The outcome is now recorded
against the exact decision id instead of inferred, so it is a lookup and not a
guess. Approvals commit *before* anything claims to be recorded, and a refused
commit shows "Not recorded — this was not written to the evidence chain, so
nothing about it has changed."

## What I learned

**A control with a missing half is worse than a missing control**, because it
reports success. The bridge without a queue looked complete from every angle I
had been checking: the agent got a correct verdict, the decision was on the
chain, the tests passed. The thing that was missing did not fail — it was simply
absent, and nothing in the system was responsible for noticing.

**`.catch(() => {})` is where honesty goes to die.** I wrote that empty catch
deliberately, with a comment explaining that the error would "surface through
the ledger's own honest error path". It did not. The comment made a claim about
another component's behaviour that I never checked. An empty catch next to a
success stamp is a lie with a delay.

**The bugs clustered at the seams.** The gate was right. The queue was right.
The ceremony was right. What failed was the ceremony calling the queue with an
approver that no component had been made responsible for producing. Unit tests
cannot see a gap that exists *between* two things that each pass.

**"Can this ever return the wrong answer?" is a better review question than
"is this correct?"** The first sends you hunting for the input that breaks it.
The second invites you to re-read your own reasoning and agree with it.

## What I'd do differently

When a slice records a question, build the thing that asks it in the same slice
— or write down, in the PR, that the loop is open and what closes it. I did say
"nothing yet surfaces it", which is why the next slice existed at all. But I
shipped a `require-approval` verdict to an agent-facing API knowing no human
could answer it, and that ordering was a choice I would reverse.

And I would treat every empty catch as a claim requiring evidence. If the error
really does surface elsewhere, name where and check it. If it does not, do not
write the catch.

## Open questions

- The escalation reaches the Dashboard queue, but nothing *interrupts* the
  operator. An agent blocked at 2am waits until someone opens the app. The
  escalation toast exists but is sim-only and its live feed is unbuilt. What is
  the real notification path — and what is the SLA the customer expects, given
  deprovisioning SLA is already pinned to kill-switch SLA?
- An approval names the human at the keyboard, taken from the operator name
  captured at sign-in. That is honest but weak: it is a self-asserted name, not
  an identity. It becomes real when the IdP does, post-reroll.

## Pointers

- Sprint: `.agentile/sprints/completed/sprint-qrm-s4/RETRO.md`
- PRs: #14 (bridge), #15 (queue + the `decision_status` fix at `1dffa28`),
  #16 (the silent approval failure)
- Key artifacts: `src-tauri/src/backend.rs` (`approve_decision`,
  `decision_status`), `src/ceremony/Ceremony.tsx` (`review`, `onSign`)
- Related essay: `2026-07-24T2103_honest-by-construction.md`
