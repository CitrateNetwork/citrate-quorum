---
created: 2026-07-26T15:00:00Z
branch: feat/qrm-phase0-live-surfaces
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# A check that cannot fail

> **The claim.** A verification whose failing branch is unreachable is not a
> weak control. It is an anti-control: it manufactures the confidence that a
> real check would have had to earn, and it spends that confidence on nothing.

The Ledger surface shipped with this:

```tsx
const run = () => {
  setState("checking");
  setTimeout(() => setState("ok"), 900);
};
```

Ninehundred milliseconds, then "✓ verified". The component defined a `bad`
state and a `pending` state, styled them in red and amber, and gave no code path
that could reach either. It had a comment explaining what the real thing would
do — *"recompute the hash locally and compare to the anchored root"* — which is
how I know the author knew. It was a placeholder. Everyone who has written
software has written one.

What makes this one worth an essay is what it was placed *under*. The line
beneath it read: *"verify recomputes the hash locally and compares to the
anchored root — the product's core claim, touchable."* The core claim. The
sentence naming the thing the customer is buying sat directly above a timer.

## The asymmetry that makes it worse than nothing

An unbuilt feature that says it is unbuilt costs you a conversation. An unbuilt
*check* that says "verified" costs you the conversation you would otherwise have
had about whether the thing it checks is true.

The failure is asymmetric in a specific way. A dashboard number that is invented
gets compared against reality eventually — someone knows the real headcount,
someone remembers the real spend. But a verification's whole purpose is to be
consulted *instead of* checking by hand. Nobody re-derives the Merkle root after
the button says yes; that is what the button is for. Every fabricated stat has a
natural predator. A fabricated check has none, because it eats the predator.

And it is load-bearing exactly when it is least examined. Nobody clicks Verify
on a decision they are relaxed about. It gets clicked on the record that matters,
by the person who needs it to be true, on the day it is contested.

## "It will be real before anyone relies on it" is a schedule, not a control

The honest defence of a placeholder check is that it is a sketch of a real one,
and the real one is coming. That defence has a hole: nothing in the codebase
records that the promise is outstanding. A `TODO` is not a control, because
nothing fails when it is not honoured. In this repo the gate was green — 20/0/1
— for every day that button lied. It was green *because* nothing about it was
wrong in a way a gate can see: the types checked, the tests passed, the pixels
rendered, the state machine was well-formed. It was a correct implementation of
a fiction.

The check-shaped placeholder is a particularly seductive form because it is
easy. Nothing forces you to write it, no reviewer objects, and the surface looks
finished. The cost lands on someone else, later, in a room where the answer
matters.

## What replacing it actually required

The real thing was not a bigger `setTimeout`. It needed a Merkle inclusion proof
API in `quorum-audit` (`merkle_proof` / `verify_inclusion`), which needed the
leaf-hashing rule extracted so that a proof and a root could not be built by two
subtly different domain separations, which needed a test sweeping every chain
length up to nine, because the odd-node duplication rule is where a proof and a
root drift apart and it only bites at odd levels.

That is the actual size of the thing the button claimed to be doing. The gap
between `setTimeout(900)` and that is the entire content of the product's core
claim.

It also forced a distinction the placeholder had erased. There are two questions,
and they have different answers:

- **Is my copy intact?** Replay the chain from genesis and check every stored
  hash. Local, offline, always answerable.
- **Does the world agree with my copy?** Ask the chain what it holds.

A single "✓ verified" answers neither, by appearing to answer both. The real
button now reports them separately — *"chain replayed from genesis: intact · 3
records / inclusion in the merkle root: proved (2 siblings)"* — and points at
the anchor row for the third question it deliberately does not answer.

## The rule this leaves behind

**A check ships with a reachable failing branch, or it does not ship.**

Not "a check ships when it is real" — that is the same schedule dressed
differently. The operational form is the one you can enforce on a Tuesday: before
you write the affordance, write the input that makes it say no, and watch it say
no. If you cannot construct that input, you have not built a check; you have
built a claim, and it belongs in prose where the reader can weigh it, not behind
a button where they cannot.

The corollary is cheap and worth having: the failing branch is what you
negative-control. This repo already applies that to its own gate — every new
`check.sh` guard is proven by breaking something and watching it fail. A verify
affordance in the product deserves the same standard as a check in CI. It is
making the same kind of promise, to someone with less ability to audit it.
