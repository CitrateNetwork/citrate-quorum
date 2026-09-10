---
created: 2026-07-26T00:30:00Z
branch: docs/qrm-s5-retro-journals
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# Half a gate is not a gate

**The claim:** when an exit criterion has two clauses and you can only satisfy
one, the honest report is "half met" — and the discipline that matters is
deciding that *before* you build, not at the demo.

> ⚠️ Essays are historical context, not governance. The operating rules live in
> `CLAUDE.md` and the planset.

## The situation

QRM-S5's exit gate, from the federation planset:

> a real internal Citrate standup runs in Quorum and produces ratified,
> **anchored** minutes

Two clauses. Ratified is a local property: a human signs a hash, and the
signature is recorded in the tenant's evidence chain. Anchored is a chain
property: that hash reaches a contract on 40204 where a third party can check
it without trusting us.

Before writing any code I checked whether anchoring was reachable. It was not,
for two independent reasons. `AnchorRegistry` exists in `citrate-chain` source
but is not in the frozen 40204 address book — never redeployed after the
2026-07-23 reroll. `MeetingRegistry` is not written at all; it is a QRM-S6
deliverable.

So the gate was unreachable on day one, and no amount of good work inside this
repo could change that.

## The tempting move

There is an obvious way to make this go away, and it is not fraud. It is
*emphasis*.

You build the ratification path — which is real, and good, and hard — and you
demo it. The minutes are signed. The hash is displayed. A board member can
verify it offline against the rendered agenda. Everything you show is true.

And then the gate quietly becomes "ratified minutes, anchoring to follow", and
six weeks later someone reads "S5: met" in a status table and believes the
fourth demo beat is done. Nobody lied. The word "anchored" simply stopped being
load-bearing, one document at a time.

This is how a compliance product becomes untrustworthy: not through a false
statement, but through a true statement that is allowed to stand in for a
larger one.

## Why the timing is the whole thing

Reporting "half met" at the end of a sprint is an argument. Everyone has seen
the work; the demo went well; the missing half is an external dependency nobody
in the room controls. Under those conditions "met, with a caveat" is the path of
least resistance, and it will usually win, because the people arguing for it are
tired and correct about everything except the word.

Reporting "half met" at the *start* is not an argument. It is a description.
Nobody is invested yet, the dependency is a fact you just looked up, and writing
it into the SCOPE costs nothing.

So the discipline is not honesty at the end. It is **measuring the gate against
reality before the work starts**, so that the honest report is already written
down by the time it becomes inconvenient.

In this sprint that meant `SCOPE.md` opening with a section titled "The gate
splits — say so up front", a table naming both missing contracts, and the
sentence: *this sprint's retro must record the gate as half met, not met.* Every
PR repeated it. By the time the retro was written there was nothing to decide.

## The corollary in the UI

The same claim has a user-interface form, and this sprint had it as a live bug.

The Meetings detail rendered its anchor like this:

```tsx
{isRatified && detail.anchor && <span>anchored {detail.anchor}</span>}
```

If a meeting was ratified but not anchored, `detail.anchor` was falsy and the
line simply did not render. A ratified-and-anchored meeting and a
ratified-but-unanchored one were **pixel-identical**.

That is the same failure as the status table, expressed in JSX: the absence of a
property renders as the absence of a *statement about* the property, and a
reader fills the gap with the optimistic reading. Nobody wrote "anchored" —
that is precisely the problem, because nobody wrote "not anchored" either.

The fix is that "not anchored" is a state to render, not an absence to skip:

```tsx
{isRatified && <span>anchor: {detail.anchor ?? "unknown"}</span>}
```

with the backend supplying the reason. And the reason is resolved at call time
rather than baked in as a constant, because Rule 8 forbids a hardcoded address
and — the extension this sprint added — it equally forbids a **hardcoded
excuse**. A build that says "AnchorRegistry is not deployed" will keep saying it
after the day it is.

## What this is really about

A governance product's deliverable is a set of claims an auditor can check. Its
value is not the feature list; it is the reliability of the mapping between what
the system says and what is true. Every place that mapping is allowed to slip —
a status table, a caption, a missing UI branch — spends the thing the product
exists to sell.

Which is why "half met" is not modesty or process theatre. It is the same
property as the length-prefixed hash and the agenda that refuses to load when it
has been edited: **a system that cannot overstate itself, by construction rather
than by discipline.**

Half a gate is not a gate. It is half a gate, and saying so is the feature.
