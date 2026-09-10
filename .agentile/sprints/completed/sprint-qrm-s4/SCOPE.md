---
created: 2026-07-25T05:22:00Z
branch: docs/qrm-s4-retro-journals
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: complete
sprint: QRM-S4
---

# Sprint QRM-S4 — Agent adapter layer

> **Sprint goal.** Make the gate real for agents: a keyless intake an agent must
> pass before it acts, a human who is actually asked when it escalates, an
> operator who can issue the grant that governs it, and adapters that put all
> three in the path of Claude Code, Codex and any MCP server.

Executed against the federation planset's S4 work-package table
(`citrate-federation/.agentile/planset/2026-07-22-citrate-quorum/`, not in this
checkout). The planset's S4 exit gate is: *"an agent cannot obtain a signature
without a ceremony (source-scan + integration test)."*

## Work packages (all closed)

| WP | What | PR | Tests |
|----|------|----|-------|
| S4.1 · bridge | `agent_bridge.rs` — loopback-only, bearer-authed intake over a `0600` token file, `Zeroizing` in memory. `POST /intent` runs the gate BEFORE the agent acts and records the verdict; `GET /health` open. No route signs | #14 | 110 → 125 |
| S4.2 · queue | The human half of HIC-1. `Verdict::Approved`, a durable pending queue, approve/reject clearing it, `GET /decision/{id}` so a blocked agent learns the answer, and `useCeremony().review()` — a second ceremony entry path that does NOT re-run the gate | #15 | → 138 |
| S4.3 · grants | Grant issuance through the ceremony; `issue_grant`/`revoke_grant` record their own HIC-1 acts and refuse an anonymous issuer; the Agents surface goes live from real evidence | #16 | → 145 |
| S4a · Claude Code | `quorum-adapter` crate + `claude-hook` PreToolUse integration; `params_hash` becomes a real BLAKE3 commitment to tool arguments | #17 | → 155 |
| S4a · MCP + egress | `mcp-proxy` (stdio) built for MCP `2026-07-28`; Q10 egress control enforced at the gate, fail-closed | #18 | → 172 |
| S4a · HTTP + Codex | `mcp-proxy-http` (Streamable HTTP) and `codex-hook`; the coverage table | #19 | → 177 |

### Enabling work absorbed into this sprint

| What | PR | Why it was needed |
|------|----|-------------------|
| The policy-gate seam + durable evidence | #10 | The loop was live in Rust but unreachable from the app, and the evidence lived only in RAM |
| Budget consumption + refunds | #11 | `consume()` existed, was tested, and was never called — budgets never depleted |
| Unwired domains must reject, not throw | #12 | A synchronous throw from a `Promise`-typed method white-screened the packaged app |
| The S2D.4 honesty pass | #13 | S2D's exit gate was recorded met; the packaged app was rendering fabricated numbers |

## Acceptance (met)

- **The S4 exit gate.** The single-signing-path source scan now covers every
  quorum-authored module — it previously claimed to and read only `lib.rs` — plus
  a scan asserting the bridge grows no route beyond `/health`, `/intent` and the
  read-only `/decision/{id}`. Nothing in the adapter path can sign.
- **The gate runs in the adapter, before execution**, for Claude Code, Codex,
  MCP over stdio, MCP over Streamable HTTP, and anything that can shell out.
- **An escalation reaches a human** and the agent learns the answer; approve
  keeps the charge, reject refunds it, both clear the queue, both survive a
  restart.
- **Every decision is evidence**, including the ones nothing authorised.
  `params_hash` is a real commitment now that adapters supply parameters.
- **Q10 fail-closed egress**: a fresh install permits no external egress at a
  controlled classification and writes that posture to disk so it is inspectable.
- Local gate `scripts/check.sh`: 20 pass / 0 fail / 0 skip (18 → 20 checks).
- Rust tests 79 → 177; frontend 0 → 21.

## Verified on the packaged binary

Every work package was exercised against a real `.deb` build from a clean
install, not only in tests. The full loop, end to end, on one evidence chain:

```
0: spend        ungoverned        HIC-X  claude-code   (no grant — flagged)
1: grant.issue  approved          HIC-1  R. Ortiz      (issued in a ceremony)
2: spend        require-approval  HIC-1  claude-code   (governed — escalates)
3: spend        approved          HIC-1  claude-code   (a human said yes)
```

and the agent's poll on decision #2 flipping `pending → approved`.

## Explicitly carried forward

| Item | Where it goes | Why |
|------|---------------|-----|
| The deployed `EgressPolicy` protocol | QRM-S6/S7 | Enforcement exists and is fail-closed; the amendable protocol that supplies the allowlist is chain-gated |
| A Hermes in-process hook | `citrate-agent-runtime` | Governed today via `gate`/`mcp-proxy`; a native integration belongs in that repo |
| The operator's own clearance vs the ceiling they grant | Post-reroll | Needs the live `ClearanceReader`, which fails closed to Public with no chain |
| Egress denial demonstrated on the binary | Next UI pass | Proven by tests; driving a native `<select>` defeated the test harness |
| `session_resolve` live chain-read, revocation timer | Post-reroll | Unchanged from S2 |
