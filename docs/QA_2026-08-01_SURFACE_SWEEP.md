---
title: "citrate-quorum — full surface sweep, 2026-08-01"
created: 2026-08-01
branch: docs/qa-quorum-sweep-closeout
author: Claude (Opus 5, 1M) for SaulBuilds
status: complete — ready for blind internal audit
scope: all 14 surfaces, the bridge seam behind them, and the backend invariants they depend on
---

# citrate-quorum surface sweep — 2026-08-01

Third app in the Commissary QA cadence, and the deepest. Every surface was read.

**Verdict: this is the best-built repo in the federation, and the defects that
survived are the ones its own ratchets are structurally unable to see.** Four PRs,
six defects, all mutation-verified.

The repo's culture is doing most of the work already. `scripts/check.sh` runs 21
checks and is honest about its own blind spots. Multiple surfaces carry comments
documenting a fabrication that was found and removed in an earlier pass. That is why
the remaining findings cluster where they do: not in the code anyone was looking at,
but in the **seams between correct pieces**.

---

## 1. The defects

| # | Where | What | PR |
|---|---|---|---|
| Q-1 | Calendar | A fabricated incident — a "Sync conflict" naming CCB #13 and a 15:00 Outlook edit, in static JSX with no state behind it | [#56](https://github.com/CitrateNetwork/citrate-quorum/pull/56) |
| Q-2 | Calendar | The grid pinned to July 2026 by literals; already showing the wrong month | #56 |
| Q-3 | Calendar | A swallowed `events()` read rendering as "no meetings" rather than "could not find out" | #56 |
| Q-4 | Calendar | Three enabled-but-inert buttons, while `Journal.tsx` two files away shows the correct `disabled` + `title` pattern | #56 |
| Q-5 | Agents / backend | **A grant could be revoked by nobody** — and the grant was revoked and persisted before the ledger write failed, so the operator was told it had not happened | [#57](https://github.com/CitrateNetwork/citrate-quorum/pull/57) |
| Q-6 | Ledger, Dashboard, Rooms | Liveness indicators that could not go out — a pulsing "live" dot over a silently frozen feed | [#58](https://github.com/CitrateNetwork/citrate-quorum/pull/58) |

Plus [#55](https://github.com/CitrateNetwork/citrate-quorum/pull/55): the RBAC drift
check was instructing operators to **delete** the two-step governance-transfer
bindings, because its input artifacts were five weeks stale.

### The one that matters most

**Q-5.** The codebase already refuses to *issue* authority as nobody —
`a_grant_cannot_be_issued_by_nobody` has been there all along. Revocation, the HIC-1
act, had no such guard, and `session.operator()` returns `string | null`.

The ordering made it worse than an unattributed row: `revoke_grant` mutated and
persisted **before** calling `record_principal_action`, which is where the blank-actor
check lives. So a nameless revocation removed the capability, recorded nothing, and
returned an error that told the human it had not happened. Grant store and ledger
disagree permanently.

That is the precise inverse of the bug the comment at `Agents.tsx:146` documents
fixing. Same seam, opposite direction, and the earlier fix did not reach it.

---

## 2. What came out clean, and why that is worth recording

- **Governance** (856 lines, the newest). Its header documents QRM-S7.8 already
  removing a hardcoded template name, a fake audit CID described as "audited", a
  fabricated block number labelled "anchored", and a CREATE2 address literal used as a
  fallback. All 19 buttons wired. Nothing of that weight left.
- **Ledger's Verify.** Genuinely replays the BLAKE3 chain from genesis *and*
  recomputes the Merkle inclusion proof, reporting the two **separately**, with a
  comment explaining that collapsing them "is exactly how a verify button becomes
  theatre." Strict `=== true` on both, so it fails closed.
- **Rooms / MR-4.** Admission is grant-derived and well-tested, and the doc explicitly
  names what it does **not** bound — the operator's own clearance — rather than faking
  it. The surface says so on screen. The monotonic half is enforced *structurally*:
  there is no admit-to-existing-room command and nothing mutates a room's
  classification after construction.
- **Meetings.** `AnonymousRatifier` is a real error with a test, quorum is a
  precondition rather than a label, and ratification is bound to a content hash so
  minutes cannot change between ceremony and signature. **It already got right the
  invariant the grants path was missing** (Q-5).
- **Wallet, Node, Journal, Repos, Settings.** Rule 11 observed throughout —
  `eth_getBalance · exact, to the wei`, `—` for absent values, sources named.

---

## 3. The structural finding

**Every defect found was invisible to the existing checks, and predictably so.**

`check.sh` says this about itself:

> *"This catches the stat-shaped literals ... It CANNOT catch every invented value (a
> bare `4` is indistinguishable from a real one), so it is a tripwire for the common
> case, not a proof of honesty. The proof is running the packaged app against an empty
> tenant and reading what it claims."*

That is exactly right, and Q-1 is what it describes: a fabricated incident carrying no
thousands separator and no percentage, so every ratchet passed it.

**The repo had no surface tests at all.** All 51 frontend tests lived in `bridge/`,
`ceremony/` and `shell/`; the 14 surfaces — 3,742 lines, and the only thing a customer
ever sees — had none. This sweep added the first ones.

The recommendation is not more ratchets. It is that **a surface making a claim should
have a test that can falsify it**, and that the `QUORUM_SMOKE=1` packaged run
(currently opt-in and skipped) is the check most likely to find the next Q-1.

---

## 4. On the evidence in these PRs

Two of my own checks were broken before the code was.

- On #56, the first sha256 tripwire pinned a single download state, stopped reaching
  the markup when the render moved, and **survived a mutant that restored the false
  claim verbatim**. Rewritten to sweep every state.
- On #58, the mutation patterns targeted `onHealth?(` where the source reads
  `onHealth?.(`, so **neither mutant ever applied** and I briefly had two "survivors"
  against a suite that was fine.

Both were caught by checking whether the mutation had actually landed rather than
trusting the result. Recorded because the green ticks in these PRs are worth exactly
as much as the mutation checks behind them, and an auditor should know two of those
needed a second pass.

---

## 5. Not fixed — for the owner

- **`{operator ?? "operator"}`** labels an interview turn with a generic role when
  identity is unresolved. On a surface whose purpose is attribution, "unknown" is truer.
- **Four `.catch(() => {})`** on Governance's spec/interview reads, and one each in
  Settings and Repos. All are *secondary* reads behind a primary that does have an
  error plate, so severity is low — but they render "nothing here" for "could not
  find out", which is the Q-3 class.
- **Room classification monotonicity is unguarded.** It holds today because no
  mutation path exists. Nothing fails if someone adds one. A source-scan tripwire
  would pin it, in the style CLAUDE.md already uses for the signing rule.
- **`CATALOG.services`-style tier modelling** — not applicable here, but the
  equivalent gap in core-membership is recorded in that repo's report.

---

## 6. Commissary readiness

Quorum is listed in the catalog (core-membership#30) as `kind: app`,
`minTier: commercial.kyc`, `maturity: beta`, `unreleased: true`.

Beta rather than release-candidate is deliberate: S0–S7 are merged and the gate is
green, but **maturity describes the artifact**, and `version` is `0.0.0` with no tag,
no signed installer, and no T1 external audit — which `AUDIT_TIER.md` mandates before
release.

Nothing in this sweep changes that. The blocker is release engineering, not code.
