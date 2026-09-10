---
created: 2026-07-26T15:05:00Z
branch: feat/qrm-phase0-live-surfaces
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: proposed
---

# Decision — the NodeDomain bridge contract changes shape

> **Status: proposed, awaiting owner ratification.** `src/bridge/domains.ts`
> carries the S2D.3 freeze note: *"Changing it after the S2D.3 freeze costs
> backend rework — owner decision, not a PR comment."* This is the PR comment
> promoted to a decision record so it can be refused. The change is already in
> PR #32 because Phase 0 could not be built around it; if the owner refuses, the
> revert is the Node surface, not the whole slice.

## What changed

| Frozen at S2D.3 | Now | Why |
|---|---|---|
| `peers(): Promise<NodePeer[]>` | *removed*; the count lives in `status()` | There is no peer LIST to return. `net_peerCount` yields a number and chain 40204's public RPC exposes no peer enumeration. |
| `logs: Subscribe<LogLine>` | `activity(): Promise<ActivityLine[]>` | There are no node logs to stream: citrate-quorum supervises no node. |
| `blocks(height: number): Block[]` — **synchronous** | `blocks(count: number): Promise<Block[]>` | Every row is an `eth_getBlockByNumber`. A synchronous signature cannot be served by a chain. |
| — | `status(): Promise<NodeStatus>` | Height, peer count, client version, sync state, measured round trip, head base fee, blue score. |
| `Block { blue, checkpoint, gas: string, age: string }` | `Block { blueScore, mergeParents, gasUsed, gasLimit, timestamp }` | The node reports `blueScore`, `selectedParentHash` and `mergeParentHashes`, and nothing that says "checkpoint". `age` is derived at render time from a real timestamp. |

`WalletDomain.summary`, `SettingsDomain.tenancy` and `LedgerDomain.decision` /
`correlation` also changed shape for the same reason — their old types described
data no source produces — but those were `Unavailable` stubs with no
implementation to break. `NodeDomain` is the one that was frozen with a
*synchronous* method, which is the part that could not be preserved under any
implementation.

## Why this is not a preference

The freeze exists to stop the frontend redesigning the contract while Rust is
written against it. That is the right rule and this is not that: no Rust existed
for this domain, and the shapes are not opinions about ergonomics. Each is a
statement about what chain 40204 answers, checked against the live node before
the surface was written:

```
$ cast rpc net_peerCount --rpc-url https://rpc.citrate.ai      → "0x9"
$ cast block 141720 --rpc-url https://rpc.citrate.ai --json    → blueScore,
    selectedParentHash, mergeParentHashes, miner, gasUsed, gasLimit, timestamp
    (no checkpoint field, no per-block blue flag)
```

The alternative to changing the contract was keeping it and filling the removed
fields with something — a synthesised peer list, a `blue` boolean derived from
a heuristic, a checkpoint marker every 50 blocks. That is the defect this repo
keeps finding, and the prototype's own rendering made it vivid: non-blue rows
drew in red, so an invented boolean would have painted a **finding** onto the
operator's screen at whatever rate the heuristic happened to fire.

## What it costs

Nothing downstream, today. The Node surface is the only consumer; the sim
adapter was updated in the same PR; the tauri adapter's mapping is pinned by
tests in `src/bridge/tauri/index.test.ts`. The cost is that the frozen contract
is now one domain less frozen, and a future reader of `domains.ts` needs to know
that a freeze was broken once, for a stated reason, rather than eroded.

## What is asked of the owner

Ratify, or refuse and say which shape to restore. If refused, the honest
fallback is not the old contract — it cannot be implemented — but a Node surface
that stays dark with a plate saying the contract and the chain disagree, which
is worse for the product and better than inventing peers.

## Related

- `.agentile/planset/COMPLETION_PLAN.md` — Phase 0, where this landed.
- `.agentile/sprints/completed/sprint-qrm-phase0/RETRO.md`.
- `src/bridge/domains.ts` — the `NodeDomain` doc comment states the same three
  reasons at the point of use.
