---
created: 2026-07-24T21:04:00Z
branch: docs/qrm-journals-retros-essays
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# Build on devnet

> The chain being down is a reason the software cannot be *deployed*. It is not
> a reason the software cannot be *built* — unless you let it be.

## Frame

For the entire span of citrate-quorum's backend sprints, chain 40204 was wedged
(the SRP state-root saga) and there was no live IdP to authenticate against. A
blockchain governance product with no reachable blockchain and no identity
provider sounds like a product that cannot be worked on. It was, in fact, the
period in which most of the backend got built. The reason is a stance worth
naming, because it recurs across the whole federation and it is easy to get
wrong in either direction: **build everything the chain would only confirm, and
stub — honestly — only the confirmation.**

## Two kinds of dependency on a chain

Code that touches a blockchain depends on it in one of two ways, and they are
routinely conflated.

The first is *computational*: the code computes a value that a chain will later
store, settle, or witness — a policy verdict, a grant resolution, a hash chain, a
Merkle root. None of this needs a live chain to be correct. It needs a
specification of what the chain expects, which we have. This code can be written,
tested to exhaustion, and finished while the chain is a smoking crater.

The second is *confirmational*: the code reads current on-chain state, or submits
a transaction, or waits for finality. This genuinely cannot proceed without a
live chain. There is no honest way to fake it — a mocked `getClearance` that
returns "CUI" is precisely the fabricated data Rule 1 forbids, dressed as
infrastructure.

The build-on-devnet stance is to separate these two ruthlessly, do all of the
first, and represent the second as an honest, fail-closed stub. citrate-quorum's
backend is almost entirely the first kind. `resolve_effective_grant` computes the
least-of-ceilings join; `evaluate` computes the verdict; `HashChain::append`
computes the tamper-evident record. The governed-action loop — evaluate, record,
chain — runs today, with no chain in it. The one confirmational seam,
`session_resolve`'s live clearance read, is a stub that fails closed to Public.
That is not a gap pretending to be filled; it is a gap saying, in code, "I will
be Public until a real `ClassificationRegistry` tells me otherwise."

## Fail-closed is what makes the stub honest

The move that keeps this from being a euphemism for "we mocked it" is that every
confirmational stub fails *closed*. An unread clearance is not assumed
permissive; it collapses to the least privilege — Public, non-foreign-national
gating on, no grant, `Ungoverned`. A mock returns a convenient answer; a
fail-closed stub returns the *safe* answer, which is almost always the
inconvenient one. The difference is observable in the tests: the clearance suite
asserts that an unreadable chain yields Public, that an unpaid tier yields
Public, that an unread foreign-national flag is treated as foreign-national. You
cannot write those tests against a mock designed to make the demo look good; they
only pass against a stub designed to be safe when it knows nothing.

## Clockless, I/O-free logic is what makes it fast

Building on devnet is only cheap if the computational core is genuinely
independent of the environment, and that independence has to be designed in.
Every quorum backend crate takes `now_ms` as a parameter and performs no I/O.
There is no ambient clock, no network, no filesystem. That is why 79 tests run in
milliseconds, why the same functions drop straight into Tauri commands that
supply `SystemTime` at the boundary, and why "the chain is down" never touched
the inner loop. Purity is not an aesthetic here; it is the property that lets the
work continue when the world outside the function is broken.

## What this implies

- **Separate computational from confirmational dependencies explicitly, per
  seam.** For each place the code "needs the chain," ask which kind. If it is
  computational, build it now. If it is confirmational, stub it fail-closed and
  carry it forward with a named gate.
- **Every confirmational stub fails closed and says so in a test.** The test that
  the safe answer is returned under ignorance is the thing that distinguishes the
  stub from a mock.
- **Keep the computational core clockless and I/O-free.** It is what makes
  build-on-devnet fast and what makes the same code trivially wireable later.

## What this does NOT imply

- It does not imply the product is done, or that the confirmational half is
  small. Live clearance reads, transaction settlement, and on-chain anchoring are
  real, load-bearing work that genuinely awaits the reroll. Build-on-devnet moves
  the *movable* work forward; it does not make the immovable work disappear.
- It does not license "we'll wire it up later" as a way to skip design. The
  computational core is only finishable now because the confirmational contract
  (what the chain expects) is specified now. No spec, no devnet build.

## References

- The wedge this was built around: [[project-srp-state-root-purity]] (chain reroll pending)
- Code: `quorum-session`, `quorum-clearance` (fail-closed reads), `src-tauri/src/backend.rs`
- Sprint QRM-S2 retro: `.agentile/sprints/completed/sprint-qrm-s2/RETRO.md`
- Companion essay: `.agentile/docs/essays/2026-07-24T2103_honest-by-construction.md`
