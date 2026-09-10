---
created: 2026-07-25T17:10:00Z
branch: chore/qrm-s4-retro-hygiene
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# Audit — every "pre-existing" dismissal in this repo's PR history

> QRM-S4's retro carried the action item: *"Revisit every 'pre-existing'
> dismissal in this repo's PR history. The one I made twice was a real bug."*
> This is that pass. Nine dismissals across PRs #1–#12 were re-read and each
> one re-tested against `main`, rather than taken at its word.

## Method

`gh pr view` over all 23 merged PRs, grepping bodies for `pre-existing`,
`preexisting`, `NOT fixed`, `not verified`, `cosmetic`, `known issue`,
`unrelated to this`, `left alone`, and `follow-up`. Every hit was then checked
against the current tree or the live system — never against the PR text that
described it.

## Result

**Seven of nine are genuinely resolved. Two remain open, and one of those had
an attribution that turns out to be wrong.**

| # | Dismissal | Source | Verdict |
|---|-----------|--------|---------|
| 1 | GitHub Actions `startup_failure` "pre-existing and unrelated" | PR #1 | **OPEN — attribution wrong.** See below. |
| 2 | duplicate-React-key warning = "pre-existing sim noise" | PR #10 | **Was a real bug.** Fixed in #15 (dedupe on id). The retro's own example. |
| 3 | `CapabilityGrant::consume()` never called — budgets never deplete | PR #10 | Resolved. `backend.rs:596` now calls `g.consume(charge)`. |
| 4 | `app_data_dir()` on a real install is compile-checked only | PR #10, #11 | Resolved. The packaged smoke installs to `~/.local/share/ai.citrate.quorum` and asserts the evidence store opens. |
| 5 | Dashboard renders fabricated numbers in the packaged app | PR #12 | Resolved in #13, plus the `no fabricated stats in surfaces` gate check. |
| 6 | status bar hardcodes `surface: live (sim)` | PR #12 | Resolved. `Shell.tsx:154` reads `BRIDGE_MODE` and renders `surface: being ported` when the surface is not built. |
| 7 | surfaces with an unavailable domain load forever | PR #12 | Resolved. All 12 data surfaces route through `useDomain` + `DomainErrorPlate`. |
| 8 | `append_record` / `chain.jsonl` not proven on a real install | PR #12 | Resolved. The smoke asserts the chain grows 0 → 1 and that 4 records survive a restart. |
| 9 | Governance deploy beat not verified at runtime | PR #10 | **OPEN.** The smoke renders the surface but does not drive the interview step, so the deploy ceremony is still typechecked-only. |

## Finding 1 — the CI dismissal was wrong, and it is the important one

PR #1 wrote: *"GitHub Actions is returning `startup_failure` on every run across
the CitrateNetwork org... Pre-existing and unrelated to this change."* Nobody
revisited it for three months. Re-tested:

- **citrate-quorum has never had a CI run start. 100 of 100 runs, every one
  `startup_failure` at 0s**, including the latest push to `main`. Not one green
  run has ever existed in this repo.
- The workflow itself is fine: `docs.yml` is valid YAML, is registered with
  GitHub (`state: active`), and Actions is enabled at both org and repo level
  with `allowed_actions: all`. The failure is environmental, not a file bug.
- **The "org-wide" attribution does not hold.** `citrate-native` shows
  `success` on 2026-07-17 and `startup_failure` from 2026-07-19 onward — there
  is a cutover date, somewhere around 2026-07-18. Actions worked, then stopped.
- citrate-quorum's first commit is 2026-07-22 — **after** the cutover. That is
  the whole explanation for 100/100: the repo was born into a broken state and
  every run since has failed for a reason that has nothing to do with its code.

The org is on the GitHub **Free** plan with 43 private repos. Actions minutes
consumed: 13,746 (May), 47,659 (June), 15,838 (July) against a 2,000-minute
included allowance for private repos, with small net charges each month. The
precise knob is a billing/quota setting on the org, which is an owner action —
not a code change and not something this repo can fix.

**Why it matters here and not just as an annoyance.** citrate-quorum is a T1
repo — money, keys, identity, governance, binary distribution. Its only gate is
`scripts/check.sh` run by hand on one workstation. There is no second machine
verifying anything, no check on a PR, and no protection against a green local
run on a dirty tree. That is a real gap in a repo whose deliverable is audit
evidence, and calling it "unrelated" is what kept it invisible for three months.

## The lesson, restated

The retro said *"'Pre-existing' is a statement about when, not about whether."*
This pass adds a second half: **an attribution attached to a dismissal is also a
claim, and it also goes untested.** PR #1's error was not deciding to move on —
that was reasonable. It was the word *unrelated*, which converted an open
question into a closed one, and no later PR reopened it because the answer was
already written down.

Dismissals should record what was observed and what was **not** checked, not a
diagnosis that will be read later as settled.
