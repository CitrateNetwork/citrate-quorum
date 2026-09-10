---
created: 2026-07-24T20:45:00Z
branch: docs/qrm-journals-retros-essays
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# Sprint QRM-S1 — Retrospective

> The sprint that decided citrate-quorum would *share* its safety-critical
> spine with citrate-core instead of forking it — and paid for that decision
> up front.

## Outcome

| Field | Value |
|-------|-------|
| **Goal achieved?** | YES |
| **WPs planned / closed** | 8 / 8 (S1.1 bootstrap, S1.2 kit extraction, S1.3 app skeleton, S1.4 tenancy, S1.5 RBAC, S1.6 license, S1.7 local gate, S1.8 branding/installer) |
| **Carry-forward WPs** | none |
| **Closing branch** | `main` at `f569c3d` (QRM-S1 complete marker) |

## Metrics delta

| Axis | Start | End | Δ |
|------|-------|-----|----|
| Rust tests (quorum crates) | 0 | ~40 | +~40 |
| citrate-core baseline (kit extraction) | 358 | 359 | +1 (monotone held across the `git mv`) |
| Formal specs | 0 | 0 | 0 |
| CI tripwires (local gate ratchets) | 0 | 5 | +5 |
| Frontmatter coverage | n/a | 100% of `.agentile` docs | — |

No axis went down. The single number worth staring at is the citrate-core
baseline: extracting eight modules into a new `kit/` crate and re-pointing an
entire app at it moved the count by **+1**, not −N. That is the signature of a
clean extraction — nothing was lost in the move.

## What worked

- **Extracting the kit as a `git mv`, not a copy.** Eight modules (ceremony,
  custody, oidc, supervisor, wallet, rpc, txdecode, config) plus seven test
  files were *moved* into `citrate-core/kit/`, so the entire history followed
  and there was never a moment where two copies could drift. citrate-quorum
  then consumed the crate. Rule 9 ("link, don't copy") stopped being an
  aspiration and became the physical layout of the code.
- **The gated signers got *stronger* by moving.** Inside citrate-core they were
  `pub(crate)` to the app crate; inside the kit they are `pub(crate)` to the
  *kit* crate. citrate-quorum therefore **cannot** call a signer directly — the
  compiler forbids it. A property we hoped to assert with a test is now
  enforced by the module system, and the test is a backstop rather than the
  fence.
- **The local gate (`scripts/check.sh`) landed in S1, not S9.** Building the
  honesty ratchets (no-mocks, no-unwraps, test-monotone, spec-monotone,
  tripwire-monotone) before there was much code meant every later WP was born
  under the gate instead of being retrofitted to pass it.

## What didn't work

- **The freeze-window plan was over-engineered and then thrown away.** We
  proposed a scoped 8-file freeze to protect the extraction target, revised it
  twice, and in the end the owner's "merge #81 first, don't freeze until we
  have to" was simply correct — the freeze was never needed. Cost: two
  documents and a detour. Lesson recorded in the S1 journal.
- **citrate-core carries pre-existing rustfmt-1.93 drift** that we could not
  clean inside this sprint because org-wide CI is down. We left it untouched and
  documented it rather than sweep-format the whole crate (which `rustfmt`
  on a crate root will do by following `mod` declarations). Real, still open,
  owned by a future `cargo fmt` PR.

## What surprised us

- **The kit boundary made the app *smaller to reason about*, not larger.**
  The intuition going in was that a shared crate adds indirection. In practice,
  `mod custody` → `pub use citrate_core_kit::custody` left all fifteen app
  modules resolving `crate::custody::…` unchanged, and the app's own surface
  shrank to the things that are genuinely quorum-specific.
- **`rustfmt` on a crate-root file reformats the entire crate.** Formatting
  `lib.rs` followed every `mod` and rewrote files we never touched. The fix —
  format individual files, never the root — is the kind of thing you only learn
  by watching a diff balloon.

## Carry-forward

| Item | Where it goes | Why deferred |
|------|---------------|--------------|
| Production SSH git-dep form for the kit | Owner infra | Needs an owner-provisioned `github-citrate-core` deploy key + alias; local path dep works today |
| citrate-core rustfmt drift | A separate `cargo fmt` PR | Out of scope; entangled with org-wide CI being down |

## Decisions ratified mid-sprint

- **Keep the kit inside citrate-core**, consumed as a path dep during dev and a
  pinned SSH git-dep in production — rather than promoting it to a standalone
  repo. Owner decision, ratified 2026-07-23. Mirrors how
  citrate-agent-runtime consumes citrate-wallet-core.
- **The SignatureCeremony is the one signing path, asserted by a source-scan
  test that "runs in both repos."** The kit-side test scans kit source; the
  app-side test scans app source for any competing signing site.

## Action items for next sprint

- [x] Pin the kit rev in WP-S1.3 acceptance once citrate-core PR #82 merged (done — `5a1b5c3`).
- [ ] Provision the citrate-core deploy key so the production git-dep form can replace the path dep (owner).

## Notes

Velocity calibration: S1 was front-loaded — the kit extraction (S1.2) was the
expensive, irreversible WP and everything after it was additive. The tenancy /
license / RBAC crates (S1.4/1.5/1.6) went fast precisely *because* the gate and
the kit boundary were already in place. Related journal:
`.agentile/docs/journals/2026-07-24T2050_kit-extraction-two-apps-one-signer.md`.
Related essay: `.agentile/docs/essays/2026-07-24T2102_a-prompt-is-not-a-control.md`.
