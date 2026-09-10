---
created: 2026-07-24T20:50:00Z
branch: docs/qrm-journals-retros-essays
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# Two apps, one signer

> The extraction only counts as clean if the compiler, not a code review,
> forbids the second app from signing on its own.

## Context

QRM-S1, mid-sprint. citrate-quorum needs the same SignatureCeremony, custody
vault, OIDC RP, and app-config machinery that citrate-core already has. The
tempting move is to copy the eight modules over. The rule (Rule 9: link, don't
copy) says no. The question is how to *physically* prevent the drift the rule
warns about.

## What happened

We extracted eight modules — ceremony, custody, oidc, supervisor, wallet, rpc,
txdecode, config — plus seven test files into a new `citrate-core/kit/`
workspace crate. Crucially this was a `git mv`, so history followed the code
and there was never an interval where two copies of the ceremony existed to
diverge. citrate-core's `lib.rs` swapped `mod custody;` for `pub use
citrate_core_kit::custody;`, and all fifteen app modules kept resolving
`crate::custody::…` unchanged. The baseline moved 358 → 359 — up by one, not
down by any — which is what a lossless move looks like on the test ledger.

Then the part that mattered. Inside citrate-core the gated signers were
`pub(crate)` — private to the app crate. Once they moved into the kit, they
became `pub(crate)` to the *kit* crate. citrate-quorum depends on the kit but
is not part of it, so quorum code physically cannot name a signer. The
"only the ceremony signs" property stopped being something a reviewer has to
watch for and became something `rustc` rejects.

We kept the source-scan test anyway — the one that greps for a competing
`sign_message(` / `sign_transaction(` call site — but its role changed. It is
no longer the fence; it is the tripwire behind the fence, there to catch
someone who tries to reintroduce a signer *inside* the kit's trust boundary.

## What I learned

The strongest form of a safety rule is one the module system enforces for free.
We went in thinking the test was the guarantee and the crate boundary was
plumbing. It is the reverse: the crate boundary is the guarantee, and the test
is the backstop. When you can move a capability behind a visibility boundary
that the consuming code is *outside* of, do that before you reach for a lint or
a test — the compiler is a cheaper and more total enforcer than either.

## What I'd do differently

Less time on the freeze window. We spent two documents proposing and revising a
scoped file-freeze to protect the extraction target, and the owner's instinct —
"merge the open PR first, don't freeze until we have to" — was simply right. The
`git mv` was atomic enough that the freeze protected against a risk that never
materialized. Next time: default to "don't freeze," and only freeze when two
parties are provably about to touch the same files at the same time.

## Open questions

- When the production SSH git-dep replaces the local path dep, does the
  `pub(crate)`-to-the-kit property survive a *pinned rev* the same way it
  survives a path dep? (It should — visibility is a source property, not a
  resolution one — but worth confirming at the first pinned build.)
- Is there a second capability in the app (beyond signing) that would be safer
  living behind the kit boundary?

## Pointers

- Sprint: `.agentile/sprints/completed/sprint-qrm-s1/RETRO.md`
- Commits: `aeef708` (extraction done), `bb24f1b` (app skeleton on the kit); citrate-core PR #82 @ `5a1b5c3`
- Related essay: `.agentile/docs/essays/2026-07-24T2102_a-prompt-is-not-a-control.md`
