---
created: 2026-07-26T16:30:00Z
branch: feat/qrm-s3-rooms
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# The blocker that was a curl command

> An entire sprint was gated on "the relay is down, restart it before attempting
> Rooms." The relay was up. It had been up for two weeks and six days. What was
> down was the way we asked.

`wss://comms.citrate.ai` had been written into the completion plan as a hard
blocker, tagged **BLOCKED / NEEDS LARRY**, with a diagnosis attached: Caddy up,
upstream 502 on `/`, on `/health`, and *"on a WebSocket upgrade"*. Three probes,
all failing, one conclusion.

The relay serves WebSocket and nothing else. A plain `GET /` opens a TCP
connection, gets closed on without an HTTP response, and Caddy — correctly —
turns that into a 502. So do `/health` and anything else that is not an upgrade.
Two of the three probes were asking a WebSocket-only server for a web page and
reporting its silence as an outage.

The third probe is the interesting one, because it *looks* right. It sent
`Connection: Upgrade` and `Sec-WebSocket-Key` — and curl sent them over HTTP/2,
where connection-level upgrades do not exist. Caddy could not proxy it, the
backend closed, 502 again. Add `--http1.1` and the same command returns `101
Switching Protocols` and the relay's CBOR challenge frame in the same breath.

Total time to establish this: about four minutes, most of it reading the Caddy
config. Total time the finding had been sitting in a plan as a blocker on a
sprint: unknown, but it survived being written into a planning document, a
handoff, and a status note that reached the owner.

## What made it survive

Nobody re-ran the test. The diagnosis was recorded once, in prose, with enough
technical texture to sound settled — *"Caddy up, upstream 502"* is a real
sentence about a real observation. Once written, it became a fact everyone
downstream inherited. The planset's own Phase 1 section opened with it. The
handoff repeated it. Neither is wrong to have done so: that is what a plan is
for.

The thing that broke it was not scepticism. It was needing the relay for
something. Wanting to build Rooms meant wanting a connection, which meant
finding out how to make one, which took four minutes.

## The general shape

**A negative result about someone else's system is the most perishable thing you
can put in a plan, and the least likely to be re-tested.** It arrives already
sounding like work you have done rather than work you owe. And it has a
particular way of ossifying: an unblocked item gets picked up and its status
corrects itself; a blocked one is skipped, and skipping it is precisely what
stops anyone finding out.

There is a cheap defence, and this sprint adopted it. The SCOPE's first section
is a table with two columns: *what the plan said*, and *what is actually true* —
filled in by re-running each claim before writing a line of code. Three rows this
time. Two of them turned out to be wrong (the relay; whether the comms crates
were consumable at all). One was right (G1 no longer gates Rooms).

Two of three. If that ratio is anything like typical, then "check the
dependencies before scoping" is not diligence, it is the highest-yield hour in
the sprint.

## And the part that was genuinely blocked

The same table's third row: the plan said `comms-client` and `comms-core` "all
exist" and could be used. They exist. `NetSession` lived inside a Slint
application crate and `RelayClient` inside the relay *server* crate, so consuming
either meant linking a UI toolkit or a storage engine into a Tauri app. That was
a real blocker, it was not in the plan, and it cost an upstream extraction before
a line of Rooms could be written.

The plan named a blocker that had evaporated and missed one that was real. Both
errors have the same cause — the claims were about someone else's code, written
down once, and never re-run.
