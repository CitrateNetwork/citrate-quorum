---
created: 2026-07-24T21:00:00Z
branch: docs/qrm-journals-retros-essays
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# The compiler found two governance bugs

> I set out to wire four crates into a Tauri backend. Two `dead_code` warnings
> turned out to be a tenancy leak and a missing honesty number.

## Context

QRM-S2 backend wiring. The pure-logic crates (session, clearance, audit,
policy) were built and tested in isolation across PRs #3/#5/#6/#7. This session's
job was the last mile of the client side: a `QuorumBackend` managed state and
the `#[tauri::command]` surface that composes `policy.evaluate → DecisionRecord
→ per-tenant HashChain`, plus the frontend adapter that reads it. Owner
direction: CI is local, don't let GitHub be the blocker, wire the crates.

## What happened

The composition itself was straightforward — the crates were written clockless
and I/O-free precisely so they would drop into a command surface that supplies
`SystemTime` and a `Mutex`-guarded state at the boundary. The tests came easily
for the same reason.

Then the build threw two `#[warn(dead_code)]` warnings, and the workspace denies
warnings, so I had to deal with them. The easy path was `#[allow(dead_code)]`.
The hardened path was to ask *why* each field was dead. Both answers were bugs.

The first: `GrantInput.tenant` was never read. Grants were keyed by agent alone.
That means agent `sbt-41`'s grant issued in tenant A would have authorized the
same agent's action in tenant B — a cross-tenant authorization leak, a direct
violation of Rule 6. The field was dead because the isolation it implied had not
been implemented. Fix: key grants by `(tenant, agent)`, plus a test that a grant
in tenant A does not govern the same agent in tenant B.

The second: `HashChain::ungoverned_count` was never called. The method existed
in the audit crate but no command exposed it. That means the Ledger surface had
no honest source for its headline "N ungoverned actions" number — the exact gap
Rule 5 says must never be hidden. Fix: a `ledger_ungoverned_count` command
wired through to the surface.

Two warnings, two governance rules, both now enforced by tests. The one genuine
detour was self-inflicted: I aliased the Tauri managed-state handle as
`State<'static, …>`, which the command macro's generated borrow rejects with an
error that points at the *call site*, not the alias. The fix is the elided
per-invocation lifetime. Twenty minutes.

GitHub chose that moment to have a write-API incident — reads fine, PR creation
(GraphQL and REST both) returning empty bodies for about half an hour. The
branch was pushed and passing the local gate the whole time, so I left a
background retry loop to open the PR on recovery and kept moving. It opened
PR #8 on its own once the API came back.

## What I learned

A `dead_code` warning in governance code is a question, not a nuisance: "why did
you define a capability you never use?" More often than I expected, the answer is
"because I forgot to enforce the thing it was for." Suppressing the warning
would have shipped a tenancy leak and a hidden ungoverned count into a T1 audit
product. The lint was doing threat modeling.

The second, smaller lesson: the local gate is the real signal of soundness, and
CI is paperwork on top of it. When GitHub's write path fell over, nothing about
whether the work was *correct* changed — `scripts/check.sh` still said 18/0/0.
The outage cost me a PR URL, not confidence.

## What I'd do differently

Reach for the `(tenant, X)` composite key by default in any tenant-scoped map,
so the isolation is there from the first line and there is no dead `tenant` field
to notice later. The warning caught it, but "structurally correct from the
start" beats "caught by a lint before merge."

## Open questions

- `session_resolve`'s live chain-read path still fails closed to Public because
  there is no readable chain. That is safe, but the enterprise clearance axis is
  not yet proving anything real. When does the reroll land a chain to point it at?
- The backend state is in-memory. Before any real evidence is recorded, the
  per-tenant HashChains need durable, crash-atomic persistence — mirror the
  gateway money-store pattern?

## Pointers

- Sprint: `.agentile/sprints/completed/sprint-qrm-s2/RETRO.md`
- Commits: `6a8a38c` (PR #8, backend wiring); crate PRs `8d55b6a`/#3, `167c024`/#5, `cccb2a6`/#6, `c9924be`/#7
- Key artifact: `src-tauri/src/backend.rs`
- Related essays: `2026-07-24T2103_honest-by-construction.md`, `2026-07-24T2104_build-on-devnet.md`
