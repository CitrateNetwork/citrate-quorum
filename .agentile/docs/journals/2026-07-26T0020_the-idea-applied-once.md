---
created: 2026-07-26T00:20:00Z
branch: docs/qrm-s5-retro-journals
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# The idea applied once

> I found the ceremony announcing a record that did not exist. Then I found the
> fix for it, correct and commented, forty lines above — applied to one code
> path and never generalised. By me. Last sprint.

The bug: `ceremony.request()` resolves when the operator **dismisses** the
dialog, not when it settles. Callers awaited it and then did their write. So
the real order was:

```
Sign → the dialog says "On record · recorded against your name
       in this tenant's evidence chain" → operator clicks Close
     → then the write runs
```

I confirmed it on the packaged binary: at the moment that sentence was on
screen, `meetings.json` still read `awaiting` and `chain.jsonl` was empty.

Worse was `Agents.revokeGrant`, which ran `bridge.agents.revoke(...)` with
`.catch(() => {})` — a failed revocation discarded in silence while the
operator read "the capability is gone at the next checkpoint". `killAll` had
the same shape across every grant at once: a kill switch that could drop half
its revocations and report success.

## The part that stings

Answering an escalation already did this correctly. In `onSign`:

```ts
// Answering an escalation COMMITS here, before anything claims to be on
// record. Previously the approval was fired at close and its failure
// swallowed: the operator saw "On record" while the agent stayed blocked
// and the queue never cleared. A stamp that can be wrong is worse than no
// stamp.
```

I wrote that comment in QRM-S4. I hit the bug, understood it precisely enough
to state the principle in one sentence, fixed the path in front of me — and
did not ask whether any *other* path had the same shape. There were four.

So this was not a knowledge failure. The knowledge was in the file, in prose,
in the same function. It was a **scope** failure: I treated a bug as a defect
in one code path rather than as a property of the interface.

## What made it invisible

`request()` and the escalation path look different at the call site. One is
"open a dialog and tell me what the human said"; the other is "answer this
specific pending decision". They feel like different features. But they share
the thing that matters: *a modal that announces a terminal state, and a caller
that has to make that state true.*

Once you say it that way, "who commits, and when relative to the announcement"
is obviously one question with one answer. I did not say it that way, because I
was looking at a call site rather than at the contract.

## The fix, and why it is shaped like this

`SignatureIntent` gained an optional `commit`, which the ceremony awaits
between signing and settling. A rejection returns the dialog to review with the
reason shown; only a successful commit may say "on record". It returns a string
so the dialog reports what actually happened instead of a template.

That is deliberately the *same* mechanism the escalation path already used,
rather than a second mechanism beside it. A second mechanism is how you get a
third one.

And the settled sentence moved out of the component into a pure `settledNote()`
with five tests. The important one asserts the negative: with no commit and no
chain head, the note must **not** contain "recorded against your name". The
sentence that was false is now the sentence a test forbids.

## The rule I want to keep

**When I fix a bug, ask what shape it is — then grep for the shape, not the
symptom.** "The approval fired at close and its failure was swallowed" is a
symptom. "A dialog announces a terminal state that a caller is responsible for
making true" is a shape, and shapes have more than one instance.

The cheapest version of this: after fixing, write the one-sentence principle,
and then search the codebase for other callers of the same seam. If I had done
that in S4, this journal would not exist.
