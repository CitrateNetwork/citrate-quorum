---
created: 2026-07-24
branch: feat/qrm-s4a-vendor-adapters
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
---

# Agent adapters — installing the gate

An adapter makes a vendor's agent ask Quorum before it acts. The policy gate
runs **in the adapter, before execution** (`02_ARCHITECTURE.md` §4.3): the tool
call does not run until Quorum has ruled on it and recorded the ruling. A prompt
telling an agent to behave is not a control (CLAUDE.md rule 4).

One binary serves every vendor: `quorum-adapter`.

```
cargo build --release -p quorum-adapter     # target/release/quorum-adapter
quorum-adapter status                        # is Quorum set up and reachable?
```

## What it does before every call

1. Finds the running app's bridge — `<app_data>/ai.citrate.quorum/agent/`
   (`endpoint.json` + a `0600` `token`). `QUORUM_AGENT_DIR` overrides it.
2. Submits an **unsigned intent**: agent, action class, classification, a
   BLAKE3 commitment to the tool's parameters, correlation id.
3. Acts on the verdict — and the verdict is recorded either way.

| Verdict | What the agent is told |
|---|---|
| `allow` | nothing — the call proceeds |
| `ungoverned` | refused, naming the action class to ask a grant for |
| `require-approval` | refused, with the decision id now in the ceremony queue (or waits, with `--wait`) |
| human rejected | refused, and told not to retry |

## Installed vs reachable

- **No endpoint file** → Quorum is not set up here. The adapter stands aside and
  the agent runs exactly as it would without us.
- **Endpoint file present, bridge unreachable** → someone set this up and the
  gate is down. **Every gated call is refused.** An adapter that waves calls
  through when it cannot reach the gate is governance theatre.

There is no fail-open switch, deliberately. An escape hatch on a control is the
first thing an attacker looks for and the thing a hurried operator turns on
permanently.

## Claude Code

```
quorum-adapter install-claude-hook
```

prints the `hooks.json` to add to `~/.claude/settings.json`. It is printed, not
written: governance you did not consent to is not governance. Narrow `matcher`
to gate a subset — `"Bash|Write|Edit"`.

On a refusal the hook returns Claude Code's `PreToolUse` deny decision, so the
model sees the reason and can act on it. On an allow it stays **silent** rather
than returning `"allow"` — saying allow would override the user's own permission
settings, and an adapter that governs agents must not quietly widen what they
may do.

Tool names map to action classes — this mapping *is* a governance decision, so
it is a table you can read (`action_class` in `quorum-adapter`):

| Tool | Action class |
|---|---|
| `Bash`, `BashOutput`, `KillShell` | `shell.exec` |
| `Edit`, `Write`, `NotebookEdit` | `repo.write` |
| `Read`, `Glob`, `Grep` | `repo.read` |
| `WebFetch`, `WebSearch` | `net.fetch` |
| `Task`, `Agent` | `agent.spawn` |
| anything else | `tool.<name>` |

An unknown tool is governed under its own name rather than folded into an
existing class whose grant would then cover it by accident.

Environment: `QUORUM_AGENT_ID` (default `claude-code`), `QUORUM_CLASSIFICATION`
(default `Public`), `QUORUM_MODEL_ID`.

## MCP servers — the gating proxy

```
quorum-adapter mcp-proxy [--agent id] [--classification C] [--model M] -- <server cmd...>
```

Quorum sits between an MCP client and an MCP server. Every `tools/call` passes
the gate **before** it reaches the server; a refusal never reaches it at all.
Everything else is relayed byte-for-byte.

Point your MCP client at the proxy instead of the server:

```jsonc
// before
{ "command": "my-mcp-server", "args": ["--flag"] }
// after
{ "command": "quorum-adapter",
  "args": ["mcp-proxy", "--agent", "codex", "--", "my-mcp-server", "--flag"] }
```

### It is deliberately near-blind

MCP **2026-07-28** is the largest revision since launch: no `initialize`
handshake, no protocol-level sessions, no `Mcp-Session-Id`, `ping` and
`logging/setLevel` removed, server-initiated requests replaced by Multi
Round-Trip Requests, a required `resultType` on every result, renumbered error
codes. A proxy that modelled the protocol would break on release day.

This one inspects exactly one method — `tools/call`, whose shape
(`params.name`, `params.arguments`) is stable from `2024-11-05` through the
draft. `server/discover`, `subscriptions/listen`, `tasks/*`, extensions and
methods that do not exist yet are opaque and forwarded intact.

### What the revision gives us

- **Statelessness**: no session to track, so each call is independently
  gateable and the proxy holds no per-connection state to get wrong.
- **Identity on the request**: `_meta`'s `io.modelcontextprotocol/clientInfo`
  names the calling agent, so evidence records who called rather than what we
  were configured to assume. Pre-2026-07-28 clients fall back to the
  `initialize` handshake.
- **W3C trace context**: `_meta`'s `traceparent` becomes the decision's
  correlation id, lining a governed action up with the trace an OpenTelemetry
  backend sees.

### Error code

A refusal is JSON-RPC error **-32010** with
`data.governedBy = "citrate-quorum"`. The revision reserves `-32020..-32099`
for the specification and leaves `-32000..-32019` implementation-defined; a
refusal is ours, so it lives in the lower band.

A JSON-RPC batch containing a `tools/call` is refused outright rather than
partly gated — batching was removed from MCP, and letting one through ungated
to avoid synthesizing a partial batch response would be a silent bypass.

## Egress policy by classification (Q10)

Which model endpoints may serve which classification. Enforced at the gate, in
the path every adapter goes through — never in a prompt.

**Fresh install permits no external egress.** `<app_data>/evidence/egress.json`
is written on first run so the posture is inspectable rather than implied:

```json
{ "controlled": ["CUI", "ITAR"], "allow": {} }
```

- `controlled` — classifications egress is policed at. Uncontrolled ones are
  unaffected.
- `allow` — permitted endpoints per classification. Permitting a model at CUI
  does **not** permit it at ITAR.
- An action at a controlled classification that names **no** model is denied:
  if we cannot say which endpoint backed the work, we cannot assert it stayed
  inside the boundary, and "we did not check" is not a permission.
- An **ungoverned** action stays ungoverned rather than being relabelled
  `deny` — nothing authorised it at all, which is the louder alarm.

Name the endpoint with `--model` (proxy / `gate`) or `QUORUM_MODEL_ID` (Claude
Code hook).

Q10 makes egress a governance protocol the customer deploys and amends, not a
config toggle. This file is its stand-in until the `EgressPolicy` template
lands with the governance contracts (QRM-S6/S7); it is named that way rather
than pretending to be the final mechanism.

## Codex, Hermes, scripts, CI

The generic path, for anything that can shell out before it acts:

```
quorum-adapter gate --agent codex --tool repo.write --classification Proprietary \
  --cost 0 --correlation $SESSION
# exit 0 = proceed · exit 1 = refused (reason on stderr) · exit 2 = usage
```

## Not built yet

- **Streamable HTTP transport.** The proxy speaks stdio, which is what local
  MCP servers use. An HTTP gateway would route on the `Mcp-Method` / `Mcp-Name`
  headers the 2026-07-28 revision requires — the spec explicitly designs for a
  gateway reading those without parsing the body — and must honour
  `cacheScope: "private"` as a shared intermediary.
- **Codex and Hermes vendor hooks.** Both are governed today: through
  `mcp-proxy` when they speak MCP, and through `gate` otherwise. Neither has a
  native in-process hook like Claude Code's.
- **The `EgressPolicy` protocol.** Enforcement exists; the deployed, amendable
  protocol that supplies the allowlist lands with QRM-S6/S7.
