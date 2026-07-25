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

### Streamable HTTP

```
quorum-adapter mcp-proxy-http --listen 127.0.0.1:8970 --upstream mcp-host:8971 [--agent id]
```

Same gate, HTTP transport. The 2026-07-28 revision requires `Mcp-Method` on
every POST (and `Mcp-Name` for `tools/call`, `resources/read`, `prompts/get`) so
a gateway can route without parsing the body — this reads it as the fast path.

**It also checks the body.** `Mcp-Method` is required, but routing on it *alone*
would let a client skip the gate by omitting a header it is merely obliged to
send. Either signal is enough to gate; a lying header cannot hide a tool call.

The response is copied through as **opaque bytes**, headers and body, whether
that body is one JSON document or an SSE stream. The proxy never parses a
response, so `resultType`, MRTR `input_required` results, extensions and
whatever the protocol grows later pass through untouched.

It **caches nothing**, which satisfies `cacheScope: "private"` by construction:
a shared intermediary that never stores a response cannot leak one.

## Codex

```
quorum-adapter install-codex-hook
```

Codex shares Claude Code's payload field names (`tool_name`, `tool_input`,
`session_id`, `cwd`), so the gating is identical. Two differences:

1. **Hooks are off by default.** Enable with `[features] codex_hooks = true` in
   `~/.codex/config.toml`; without it the hook is a silent no-op.
2. **A refusal uses both denial channels** — the `hookSpecificOutput` JSON *and*
   `exit 2` with the reason on stderr. Published accounts of Codex's parser
   disagree about which shape it accepts, and its own documentation offers exit
   2 as an alternative. For a denial both together are safe: if one is ignored
   the other still blocks. An allow never exits non-zero, where a spurious
   failure would break every call.

### Coverage — do not assume

Published accounts disagree about which tools fire `PreToolUse` on which Codex
version: some say the shell tool only, others include `apply_patch` and MCP tool
calls. **The Quorum ledger answers it for your version.** If you only ever see
`shell.exec` decisions, the rest is not reaching the hook.

Govern Codex's MCP tool calls with `mcp-proxy` regardless — that path does not
depend on the hook at all.

## Hermes, scripts, CI

The generic path, for anything that can shell out before it acts:

```
quorum-adapter gate --agent codex --tool repo.write --classification Proprietary \
  --cost 0 --correlation $SESSION
# exit 0 = proceed · exit 1 = refused (reason on stderr) · exit 2 = usage
```

## Coverage, stated plainly

Risk **R5** in the planset warns against mistaking binding enforcement for total
coverage. What is actually covered:

| Path | Gated by | Status |
|---|---|---|
| Claude Code tool calls | `claude-hook` | verified end to end |
| Codex shell / apply_patch | `codex-hook` | verified; per-version coverage is yours to confirm from the ledger |
| MCP over stdio | `mcp-proxy` | verified end to end |
| MCP over Streamable HTTP | `mcp-proxy-http` | verified end to end |
| Hermes, scripts, CI | `gate` | verified |
| **An agent run outside an adapter** | **nothing** | **ungoverned by construction** |

That last row is the honest limit: Quorum governs what passes through it.
Detecting the rest is what the `ungoverned` verdict is for.

## Not built yet

- **The `EgressPolicy` protocol.** Enforcement exists and is fail-closed; the
  deployed, amendable protocol that supplies the allowlist lands with the
  governance contracts (QRM-S6/S7), which are chain-gated.
- **A Hermes in-process hook.** Hermes lives in `citrate-agent-runtime`; it is
  governed today through `gate` and `mcp-proxy`, and a native integration
  belongs in that repo.
