---
created: 2026-07-26T00:15:00Z
branch: docs/qrm-s5-retro-journals
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# Nine sentences that were not true

> Wiring a surface to a real backend is not a plumbing task. It is an audit of
> everything the surface was already claiming while nothing was behind it.

I went into QRM-S5 thinking the work was Rust: a meetings domain, a durable
store, some commands, a bridge adapter. That part went the way work goes. The
part I did not plan for was that turning `Unavailable` into real data made nine
existing sentences on the screen become checkable — and all nine failed.

They were not sloppy. They were *specific*, which is what made them dangerous:

- "1 decision · 1 dissent recorded" in a signing ceremony, where the item count
  beside them was computed and those two were literals.
- "anchors the minutes on chain — they become evidence", in the Effect row of
  the dialog a human signs.
- "Read from meetings.list() → MinutesRegistry + relay archive", naming two
  things that do not exist.
- The same MinutesRegistry claim again, in the detail panel, which I found only
  after fixing the footer and assuming I was done.
- "A changed agenda creates a linked successor meeting — this one has none",
  describing a feature that exists nowhere in the product.
- "local memory graph · agent entries are signed by their AgentSBT key", on a
  surface that reads markdown files.
- `brief("claude-code", "m-0723")` — ids true of the design fixture and of no
  real tenant, so the brief failed silently and rendered "…".
- The anchor line, rendered only when the anchor was truthy, so a ratified
  meeting with no anchor looked *exactly* like an anchored one.
- And the ceremony's own "On record · recorded against your name in this
  tenant's evidence chain", displayed while the chain was provably empty.

## Where they came from

Every one is a residue of the design prototype. In S2D they were captions on a
mock — descriptions of what the thing *would* say, written beside data that was
openly fake. Nobody was deceived, because the whole surface was sim.

Then the surface went live and the sim data left, and the captions stayed. A
caption beside fabricated data is a label. The same caption beside real data is
a claim. Nothing about the sentence changed; everything about its truth value
did.

That is the transition nobody puts on a work-package list. "Wire the Meetings
surface" sounds like it means "make `list()` return real rows". It also means
re-reading every literal string on the surface and asking whether it survived
the change of context.

## How I actually found them

Not by reading the code. I read the code repeatedly while writing the code, and
saw none of them.

I found them by building the `.deb`, launching it under Xvfb, driving it with
synthetic clicks, and *looking at the screenshots*. The false claims are
invisible in a diff because they are unchanged lines. They are obvious on
screen because you read the sentence next to the value it describes, and the
value is now real.

The two I missed on the first pass — the detail panel's MinutesRegistry, and
the successor-meeting line — I found on a *later* screenshot, taken to measure
a button's coordinates for something else entirely.

## The rule I want to keep

**When a surface goes from sim to live, its strings change meaning even though
they do not change.** So the wiring PR owns them. Not a follow-up, not a
cleanup ticket — the same PR, because that is the moment the claim becomes
falsifiable and the moment somebody could start believing it.

And the check is not "does this read well". It is: *what fact would have to be
true for this sentence to be honest, and is that fact true today?* For nine
sentences the answer was no, and for four of them the fact did not exist
anywhere in the product.
