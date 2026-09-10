---
created: 2026-07-26T00:25:00Z
branch: docs/qrm-s5-retro-journals
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# The verification that proved nothing

> My check passed. The screen said the honest thing. Both were true, and the
> combination was worthless — because the state it reported was one my own
> verification had accidentally created.

Verifying the standup briefs meant getting an agent into the tenant. Briefs are
per-agent, and a fresh install has none. So the script posts an intent over the
keyless agent bridge — the same path a real adapter uses — and then opens the
Journal surface and checks it renders.

It passed. The Journal rendered real journals and retros. Green.

Then I looked at the screenshot, because the whole discipline of this repo is
that a UI claim is believed only when something outside the UI proves it. The
brief panel read:

> no agent has acted in this tenant yet — there is nobody to brief

Which is exactly what the surface is *supposed* to say when no agent has acted.
The honest empty state, working perfectly. And completely wrong as a result,
because an agent was supposed to have acted.

## What had happened

My intent body used `"class"` where the bridge expects `"tool"`. It was
rejected. I had not checked the response code. So:

1. the post failed silently,
2. the tenant genuinely had no agents,
3. the surface correctly reported that,
4. and my check — "does the Journal render?" — passed, because it does.

Every individual piece behaved correctly. The product was fine. The
**verification** was broken, and it was broken in the specific way that is
hardest to notice: it produced a true sentence about a state it had created
itself.

## Why the honest empty state made it worse

There is an irony here worth sitting with. I had spent the sprint making empty
states say *why* they are empty — "this register is empty because nothing has
been scheduled, not because a read failed". That work is good and I would do it
again.

But a well-written empty state is also extremely convincing. "No agent has
acted in this tenant yet" reads like a considered, deliberate report. It does
not look like a symptom. If the surface had rendered a blank panel or an error,
I would have investigated in seconds.

The failure was legible *because* the product was honest. Honesty in the
product does not substitute for rigour in the harness.

## The actual rule

**A verification step that can fail quietly is worse than no verification
step**, because it is trusted. No verification and you know you have not
checked. Silent verification and you believe you have.

Concretely, for anything a check *sets up*: assert the setup succeeded, not
just that the thing under test responded. My script now reads:

```python
code, body = s.post_intent(...)
check("an agent acted, so there is somebody to brief", code == 200, ...)
```

Two lines. It turns "the brief said nobody acted" from an ambiguous observation
into either a real finding or an obvious harness bug.

## The same shape, twice more this sprint

This was not an isolated slip. The UI driver's `type_text` silently dropped `:`
and `/`, so an RFC3339 timestamp arrived as `2026-07-30T09` and a path as `""`
— while reporting a successful click over a field that never received the text.
Same shape: a step that could not fail loudly, reporting success.

And the first version of the meeting lifecycle driver clicked Sign, checked the
record, and found `awaiting` — which I nearly wrote off as "the click missed"
before realising it was the real ordering bug in the ceremony. The one time the
silent-failure pattern *didn't* fool me, it was because I looked at a
screenshot rather than at the exit code.

Three instances in one sprint. The pattern is not "I was careless once". It is
that setup steps feel like plumbing, and plumbing does not get assertions unless
you make a rule of it.
