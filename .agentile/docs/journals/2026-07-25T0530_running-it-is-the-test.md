---
created: 2026-07-25T05:30:00Z
branch: docs/qrm-s4-retro-journals
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# Running it is the test

> The owner asked me to launch the packaged app to prove the store wrote to
> disk. It did. It also white-screened on the first click, and eight more real
> bugs followed over the sprint — none of which `cargo test` or `tsc` could see.

## Context

Through QRM-S2 and most of S4 I verified work the way the repo's gate does: 177
Rust tests, a type-checked frontend, twenty local checks, and a headless browser
run against the **sim** adapter. Every PR was honest about what it had and had
not verified. I still shipped nine bugs that only existed in the packaged
application.

The turn came when the owner said: *launch the packaged Tauri app to verify the
store actually writes on a real install.* Not "write a test for it."

## What happened

The build worked. The app started. `<app_data>/evidence/` appeared, which
already proved `app_data_dir()` resolved and the store opened. Then I clicked
"Continue with Meridian SSO" and got a blank window.

`na()` — the helper backing every unwired bridge domain — threw *synchronously*
from functions the contract types as returning a `Promise`. Every surface writes
`bridge.agents.list().then(setFleet)` in a `useEffect`; on the Tauri adapter that
throws during the effect, before any `.catch` exists, and React unmounts the
tree. Eleven of twelve surfaces. Invisible in sim, because sim resolves.

That set the pattern for the rest of the sprint:

- The Dashboard rendered "1,204 actions recorded" and "62% budget burn" against
  a brand-new empty tenant — hardcoded literals that the "no sim data outside
  bridge" check could not see, because it greps for fixtures by *name*.
- Its counts were read once at mount and never again, so it showed "Governed 0"
  while an agent's decision was already on the chain.
- Issuing a grant showed a red **UNGOVERNED** plate, because operators hold no
  grant and I was running them through the agent gate — polluting the one number
  the product exists to keep honest.
- Approvals could never work at all on a packaged install: the approver came
  from `session.current()`, which needs an IdP, so `approve(id, "")` was refused
  — and a `.catch(() => {})` swallowed it while the ceremony stamped **"On
  record"**.
- The accountable principal came from whatever the agent claimed about itself.
- The classification-ceiling picker rendered dark text on a dark surface, so an
  operator could not read the ceiling they were about to sign.

Two more came from reviewing my own branch before merge rather than from
running: a `useMemo` below an early return in two surfaces, and a
`decision_status` heuristic that would tell an agent "approved" for an action a
human had **rejected**.

## What I learned

**Tests check the propositions you thought of. Running it checks the ones you
didn't.** Each of those bugs sat in a place no unit test was pointed at: the
boundary between a Promise-typed contract and a synchronous throw; a component's
mount lifecycle; a code path that only executes when a read fails; a CSS rule the
platform overrides. The type-checker was satisfied. The tests were green. The
application was broken.

**The sim adapter is a comfortable lie.** Everything resolves, so half of these
bugs cannot exist there. A prototype adapter is invaluable for building
surfaces and worthless for proving them. The moment a real adapter exists, the
sim run stops being evidence about the product and becomes evidence about the
prototype.

**A verification method has a blast radius.** After the white screen I fixed
`na()` and re-ran — but only visited Dashboard and Wallet, and declared it
verified. Two surfaces later had a hooks-order crash. The fix was not a better
test; it was visiting **all twelve** and measuring screenshot variance to detect
a blank render. When a check misses something, ask what class it missed, not
just what instance.

**"Pre-existing" is a statement about when, not whether.** I dismissed a
duplicate-React-key warning as pre-existing sim noise in two separate PRs. It
was a real merge bug — the same decision rendered twice when it arrived from
both the query and the stream. Age is not a defence.

## What I'd do differently

Run the packaged binary at the *start* of a slice, not at the end of one. Three
of these bugs would have been found before I built on top of them, and the
approval path would not have shipped in a state where it could never succeed.

I would also stop letting a green gate stand in for confidence. The gate proves
no *known* regression. It says nothing about whether the thing works.

## Open questions

- Can any of this be automated? Screenshot-variance-as-crash-detector worked
  well and cost almost nothing. A smoke run that boots the `.deb` headless,
  visits every surface, and fails on a blank render would have caught three of
  the nine — but it cannot catch "the number is fabricated" or "the approver is
  empty", which needed a human reading the screen.
- Driving a native `<select>` with synthetic X input defeated me, which is why
  egress denial is proven by tests rather than on the binary. Is there a better
  driver, or should the app expose a scriptable diagnostic surface for exactly
  this? The second option has its own risk: a test surface is an attack surface.

## Pointers

- Sprint: `.agentile/sprints/completed/sprint-qrm-s4/RETRO.md`
- The bugs: PRs #12 (white screen), #13 (fabricated stats, stale counts, hooks
  order), #15 (`decision_status`), #16 (human-as-agent, silent approval
  failure), #17 (principal from the agent), #18 (unreadable select)
- Recipe: build the `.deb`, run headless under `Xvfb`, drive with XTEST via
  `uv run --with python-xlib`, screenshot with ImageMagick, exercise the agent
  path over the loopback bridge with `curl` or `quorum-adapter`
