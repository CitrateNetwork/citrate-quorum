---
created: 2026-07-23
branch: main
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: K1 APPROVED 2026-07-23 (kit stays in citrate-core); freeze scope REVISED — see §3
sprint: QRM-S1 / WP-S1.2
---

# `citrate-core-kit` extraction plan + freeze window proposal

> **Headline finding:** the cut is clean. The eight modules that belong in the kit
> depend only on each other — **there is not a single back-edge from the kit into
> app-specific code.** This is a mechanical move, not a refactor, and the freeze
> window can be short.

---

## 1. Dependency analysis (measured 2026-07-23 from `citrate-core/src-tauri/src`)

Intra-crate `crate::<module>` references:

```
KIT SET (proposed)                    │ APP SET (stays in citrate-core)
──────────────────────────────────────┼──────────────────────────────────────
custody      → (none)                 │ agent    → ceremony custody earnings
supervisor   → (none)                 │             rpc supervisor wallet
rpc          → (none)                 │ node     → custody rpc supervisor
txdecode     → (none)                 │ memory   → custody supervisor
config       → (none)                 │ activity → custody wallet
oidc         → custody                │ staking  → ceremony custody rpc wallet
ceremony     → custody rpc txdecode   │ earnings → agent ceremony custody rpc
               wallet                 │             staking wallet
wallet       → ceremony custody       │ transfer → ceremony custody wallet
                                      │ membership → config
                                      │ grant_status → rpc staking
                                      │ sbt_art  → grant_status rpc
                                      │ model, ai, serve, shell, seam
```

**Two facts that decide the design:**

1. **The kit set is closed.** Every arrow out of a kit module lands inside the kit
   set. No kit module reaches into `agent`, `node`, `staking`, `earnings`, or any
   other app concern. The boundary already exists in the code; we are only making
   it explicit.
2. **`ceremony` ↔ `wallet` is a genuine cycle**, and it should stay one. The gated
   signer lives in `wallet` and is `pub(crate)` precisely so it is reachable only
   from `ceremony::approve`. They must move as a single unit, and inside the kit
   crate that `pub(crate)` boundary becomes *stronger*, not weaker: after the
   move, the signer is private to the kit crate, so nothing in citrate-core **or**
   citrate-quorum can reach it except through the ceremony. **The extraction
   measurably improves the property it must preserve.**

### 1.1 Size
| | Modules | Impl lines | Test lines | `#[test]` fns (static) |
|---|---|---|---|---|
| Kit | custody, supervisor, oidc, ceremony, wallet, rpc, txdecode, config | ~6,300 | ~5,600 | **167** |
| Stays | 15 modules | ~7,100 | ~5,000 | 196 |
| Total | 23 | ~13,400 | ~10,600 | 363 |

So ~46% of the test suite moves. Static counts are provisional; the authoritative
`cargo test --workspace --locked` baseline is being recorded now and goes in the
sprint's Records section.

## 2. Recommendation — where the kit lives

**A workspace crate inside citrate-core: `citrate-core/kit/`**, consumed by
citrate-quorum over an SSH-alias git dependency pinned to a rev.

```toml
# citrate-core/Cargo.toml
[workspace]
members = ["src-tauri", "kit"]

# citrate-quorum/src-tauri/Cargo.toml
citrate-core-kit = { git = "ssh://git@github-citrate-core/CitrateNetwork/citrate-core", rev = "<pinned>" }
```

**Why not a fourth repo.** A separate `citrate-core-kit` repo would mean a fifth
CI pipeline, a fifth audit scope, another deploy key, another release cadence, and
a cross-repo PR every time the ceremony changes. The federation already carries
that cost four times over and it is the top complaint in the split audit.

**Why this is the right boundary anyway.** citrate-core is already a T1 repo with
the audit obligation for exactly this code. Moving the code out of the repo that
owns its audit, into a repo with no audit history, would *weaken* the security
story we are selling. Keeping it in citrate-core means one audit covers the
ceremony for both apps.

**Precedent.** `citrate-agent-runtime` consumes `citrate-wallet-core` from
`citrate-chain` over exactly this SSH-alias + pinned-rev pattern. It works, the
team knows it, and the known failure mode (fresh-machine builds fail until the
`~/.ssh/config` alias exists — the split audit's F-14) is documented with a
bootstrap script.

**The cost, stated honestly.** Quorum's builds pin a citrate-core rev, so a kit
change is a two-step: land in core, then bump the pin in quorum. That is friction
by design — it is what stops the ceremony from being casually forked, and it is
strictly cheaper than the four-repo alternative.

### 2.1 What goes in the kit — and what deliberately does not
**In:** `custody`, `supervisor`, `oidc`, `ceremony`, `wallet` (gated signer),
`rpc`, `txdecode`, `config`.

**Out, on purpose:** `node`, `staking`, `earnings`, `agent` (the *node-agent*
bridge specifically — Quorum needs a generalized multi-vendor adapter layer, and
copying the node-agent bridge would prejudge that design), `membership`,
`sbt_art`, `grant_status`, `activity`, `model`, `ai`, `serve`.

`model`/`ai`/`serve` are a judgement call worth flagging: Quorum needs local
inference too. **Recommendation: leave them in citrate-core for now** and revisit
at QRM-S3b, when the governance-LoRA work tells us what the real shared shape is.
Extracting them now would be guessing, and a wrong shared abstraction is more
expensive than a later second extraction.

## 3. The freeze window — REVISED 2026-07-23

> **Superseded ask.** I originally proposed a full one-day merge freeze on
> `citrate-core/main`. After measuring what is actually in flight, that is more
> than we need. **Revised ask: a scoped freeze on 8 files, not on the repo.**

### 3.0 What I measured (2026-07-23)

| Question | Answer |
|---|---|
| Open PRs on citrate-core | **One** — #81 `harden/package-alignment-node-spawn` |
| Files it touches | `src-tauri/src/node.rs` only — an **app** file, not a kit file |
| Recent `main` commits | docs, app icons, and `node.rs`. No kit files |
| Last commit touching a kit file | 2026-07-21 (`d41e4b8`, `supervisor.rs` + its tests) |
| Kit files with work in flight **right now** | **None** |

The active work is concentrated in `node.rs`, docs, and assets — all in the app
set. The collision risk I was guarding against is currently near zero.

### 3.1 Revised ask

**Do not merge anything that modifies these 8 files until the extraction PR
lands:**

```
src-tauri/src/{ceremony,custody,oidc,supervisor,wallet,rpc,txdecode,config}.rs
                    (+ their *_tests.rs siblings)
```

Everything else on citrate-core — `node.rs`, `agent.rs`, `staking.rs`, docs,
assets, the frontend — **merges as normal throughout.** Expected duration: one
working day from start to PR-open.

### 3.2a ⚠️ PR #81 CANNOT BE MERGED AS-IS (found 2026-07-23)

The owner asked me to merge #81 before the freeze. **I did not, and should not.**

| Fact | Evidence |
|---|---|
| #81 is `CONFLICTING` against `main` | `gh pr view 81 → mergeable: CONFLICTING` |
| Its head `fde5e08` is based on `98ea93b` — **before #80 merged** | `git log origin/main..fde5e08` |
| **PR #80 (`f3fa872`) already landed a competing fix to the same problem** | merged as `8a5328e`, "spawn the node with the fleet's consensus env (fixes 2,580 sync wedge)" |
| Both edit the **same two hunks** of `node.rs` | `@@ -82` and `@@ -232 impl NodeManager` in both diffs |
| #80 adds 1 test (+31 in `node_tests.rs`); **#81 adds 0 tests** (+105 in `node.rs` only) | `git show … \| grep '^+.*#\[test\]'` |

These are two takes on the same live production issue — the 2,580 sync wedge.
Choosing between them (or merging their intent) is a **semantic call about node
consensus behavior**, made by whoever understands the wedge. It is not a
mechanical conflict resolution, and it is not mine to make in a T1 file. Doing it
on the owner's behalf is exactly the failure mode §3.3 exists to prevent.

**Owner action needed:** decide whether #81 is superseded by #80 (→ close it), or
still needed on top (→ rebase onto `origin/main` and resolve the two hunks). Only
then does §3.2's 3-line import rebase question arise.

### 3.2b ⚠️ The 357 baseline was taken on a diverged tree

The owner's local `citrate-core/main` is at `fde5e08` — i.e. it **carries PR #81's
unmerged commit** and is **missing `origin/main`'s #79 and #80**. So the 357 was
measured on a tree that is not `origin/main`.

Arithmetic:
- `fde5e08` adds **0** tests ⇒ plain `98ea93b` also = **357**
- `origin/main` = `98ea93b` + #79 (docs) + **#80 (+1 test)** ⇒ **expected 358**

**The pre-extraction baseline must be re-taken on `origin/main`** and is expected
to be **358**. Using 357 would leave a one-test gap in which a genuinely dropped
test could hide behind a "monotone" pass. Re-run:

```
git checkout main && git pull --ff-only
cargo test --workspace --locked 2>&1 | grep -E "^test result:"
```

### 3.2 The 3-line import interaction (applies to whatever #81 becomes)

`node.rs` imports three kit modules (`custody`, `rpc`, `supervisor` — lines
56–58). After the extraction those become `use citrate_core_kit::…`. So PR #81
needs a **3-line import rebase**, not a conflict resolution.

Two clean options, either is fine:
- **Preferred:** merge #81 before the extraction starts. Then there is zero
  interaction at all.
- Otherwise: extract first, and I rebase #81's three import lines myself.

### 3.3 Why a freeze at all — the reasoning

The risk is not "merge conflicts are annoying." It is specific and it is the
worst failure mode available to this program:

1. The extraction is a ~11,900-line `git mv` across 8 modules. A reviewer looking
   at that diff is reading file *moves*, not logic.
2. If a commit lands on those files mid-move, git hands me a conflict **inside
   `ceremony.rs` or `custody.rs`**, and I resolve it by hand.
3. A hand-resolved conflict is where a one-line semantic change hides perfectly.
   It is indistinguishable from move noise in a 12,000-line diff, and it is in
   the exact code whose correctness is the entire security argument of both apps:
   *there is exactly one signing path, and a test proves it.*
4. The cost of getting that wrong is not a bug. It is a silently weakened
   ceremony that our own source-scan test might still pass, shipped into a
   Fortune-200 environment under a T1 audit claim.

A freeze removes the mechanism entirely rather than relying on review to catch
it. It is cheap insurance against the one class of defect we most need to not have.

### 3.4 Why one day is enough

Because §1 found the cut is clean. No back-edges means no design work during the
window: `git mv`, fix imports, run tests, review. If the cut had been tangled I
would be asking for a week and a design review, not a day.

### Why a freeze is necessary
The extraction touches 8 of citrate-core's 23 modules, including its four
most-edited files, and it moves ~11,900 lines across a crate boundary. Every
concurrent commit to those files becomes a manual conflict resolution in
safety-critical code — which is exactly where a silent behavior change would hide.
citrate-core's `main` is actively moving right now (PIN contract wiring, the DGX
node-sync work, PRs #76–#78 landed this week), so the risk is real, not theoretical.

### 3.5 What happens in the window
| Time | Step |
|---|---|
| T+0 | Record authoritative `cargo test --workspace --locked` baseline on `main` |
| T+0:30 | Create `kit/` crate; move the 8 modules + their test files with `git mv` (history preserved) |
| T+2 | Fix imports in the 15 app modules; make the signer `pub(crate)`-to-the-kit |
| T+4 | `cargo test --workspace --locked` in core — count must be ≥ baseline |
| T+5 | Port the ceremony source-scan test so it runs in **both** crates |
| T+6 | citrate-quorum consumes the kit; `cargo tree` shows no duplicated ceremony/custody |
| T+7 | Behavior-preserving diff review — every non-import change gets justified aloud |
| T+8 | PR opened; owner merges; freeze lifts |

### 3.6 The rules that make it safe
1. **`git mv` only.** History must survive; a reviewer needs `git log --follow`.
2. **Zero logic changes in the extraction PR.** If something *needs* fixing, it
   gets its own PR before or after — never inside. A single behavior change hidden
   in a 12,000-line move is the failure mode we are guarding against.
3. **Test count monotone in both repos** (Rule 2), recorded in the sprint file.
4. **The ceremony source-scan test runs in both crates** and passes in both. This
   is the test that asserts no second signing site exists; after the extraction it
   must be true of the kit *and* of each consuming app.
5. **Rollback is `git revert` of one PR.** Nothing else changes in the window.

### If you'd rather not freeze
The alternative is a rebase-heavy week and a much longer review. I'd take it over
a bad merge, but it roughly triples the WP-S1.2 cost and pushes the S1 exit gate.
A one-day freeze is by far the cheaper purchase.

## 4. What proceeds in parallel — no freeze needed

While the window is being scheduled and the design team builds the prototype:

- **WP-S1.4 tenancy spine** — `TenantContext` types + the `trybuild` compile-fail
  test. Pure new code, no core contact.
- **WP-S1.5 RBAC bindings** — codegen from the chain ABIs (fresh, *not* imported
  from the partner shell), plus the CI drift check.
- **WP-S1.6 license seam** — trait + honest no-op.
- **WP-S1.7 CI + ratchets** — Agentile scripts, frontmatter, no-mocks, no-unwrap,
  claim-grader, test-ratchet baseline.
- **WP-S1.8 branding + installer skeleton** — inert unsigned updater.
- **QRM-S2D prep** — the intake procedure is written and ready to run the day the
  prototype PR lands.

## 5. Decisions needed from the owner

| # | Decision | My recommendation |
|---|---|---|
| K1 | Kit location | ✅ **APPROVED 2026-07-23** — `citrate-core/kit/` workspace crate + pinned git dep (§2) |
| K2 | Freeze window | ✅ **SET 2026-07-23 — scoped freeze on the 8 kit files begins Monday 2026-07-27.** Weekend work on citrate-core proceeds unfrozen; only the 8 files pause, and only from Monday |
| K5 | PR #81 disposition | ✅ **CLOSED 2026-07-23 as moot** — superseded by #80 (same wedge, same hunks). Branch retained on origin |
| K6 | Baseline on `origin/main` | ✅ **358** confirmed 2026-07-23 |
| K3 | `model`/`ai`/`serve` in the kit? | **No** — revisit at S3b when the LoRA work reveals the real shared shape |
| K4 | Does `agent.rs` (node-agent bridge) go in the kit? | **No** — Quorum needs a generalized adapter layer; copying it would prejudge that design |
