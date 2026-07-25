---
created: 2026-07-25T15:40:00Z
branch: feat/qrm-smoke-packaged
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
---

# The packaged-binary smoke run

```bash
npm run tauri build -- --bundles deb      # once, when the app changed
uv run python3 scripts/smoke_packaged.py  # ~2 minutes

QUORUM_SMOKE=1 scripts/check.sh           # or as part of the gate
```

## Why

Every slice of QRM-S4 shipped with a green gate — 178 Rust tests, a
type-checked frontend, twenty local checks — and nine real bugs were still
found by launching the `.deb` and clicking:

| Bug | Why the gate could not see it |
|---|---|
| A synchronous throw from a `Promise`-typed method white-screened eleven of twelve surfaces (#12) | The types were correct; the sim adapter resolves, so it could not happen there |
| The Dashboard showed "1,204 actions recorded" against an empty tenant, then never refreshed its counts (#13) | A source scan grepped fixtures by *name*; literals inline in a surface were invisible to it |
| A `useMemo` below an early return crashed two surfaces (#13) | Only on the failing read path, which no test walked |
| A grant revoked in an earlier session came back looking live (#21) | The backend was right; the surface only ever ran in sim, where a session never ends |

This turns the hand-driving into one command.

## What it checks

| | |
|---|---|
| **startup** | the app runs; the evidence store opens; the agent bridge listens; the token file is `0600` |
| **the gate refuses first** | an unauthenticated intent → 401; a wrong bearer → 401; no tenant scope → agents cannot act at all |
| **onboarding** | driving the UI establishes a scope *and* an operator, proven by `scope.json` |
| **every surface renders** | all twelve, by pixel variance over the surface region |
| **the agent path** | an ungranted action is refused, **recorded** rather than dropped, and the refusal carries no key material |
| **the dashboard is live** | its tiles move after a decision is recorded — a snapshot taken at mount fails here |
| **evidence outlives the process** | records survive the app exiting; the chain replays and the scope resumes; the gate still rules |

## What it cannot see

There is no DevTools protocol for the packaged WebView and no OCR on this box,
so **it cannot read rendered text**. It works from the filesystem, the bridge's
loopback socket, and image comparison.

That means it can tell you a pane never updated, but not that a number is
*wrong*. "No fabricated stats in surfaces" stays a source scan in `check.sh`.
If OCR ever lands, the obvious next assertions are that an empty tenant's
Governed tile reads `0` and that it reads `1` after one decision.

## The rule every UI step follows

A click is believed only when something outside the UI proves it happened —
`scope.json` appears, the chain grows, a crop changes. Coordinates drift when
layout changes; without this rule a drifted click produces a confusing failure
three steps later instead of "onboarding did not complete".

If the run aborts at onboarding, that is almost always a coordinate drift, not
a product bug. The screenshots and `app.log` are kept under
`/tmp/quorum-smoke-<pid>/` on failure (`--keep` keeps them always).

## It has been tried against the bugs it claims to catch

A check nobody has attempted to defeat is a check nobody has tested. Each was
reintroduced into the real source, rebuilt, and run:

| Regression | Result |
|---|---|
| Dashboard counts read once at mount | `FAIL counts refresh after a decision is recorded — unchanged, the tiles are a snapshot taken at mount` |
| `na()` throwing synchronously again | `FAIL scope + operator are established — {}`, and the run stops rather than cascading |
| One surface rendering `null` | `FAIL all 12 surfaces render — blank: Node(0)` |

The third one **initially passed**, which is the reason the crop constants are
measured rather than guessed: the first version included the sidebar and topbar,
which render perfectly well while the surface behind them is blank, and it
scored a `null` surface at 4552 instead of 0. It missed the exact bug it exists
to catch until the negative control exposed it.

## Why it is opt-in

It needs a release build and an X server and takes minutes, so it is not in the
default gate. `check.sh` reports it as SKIP with the reason — never PASS —
which is the honesty rule that file opens with: a check that cannot run must
not look like one that did.
