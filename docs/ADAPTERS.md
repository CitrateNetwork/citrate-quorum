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

## Codex, Hermes, scripts, CI

The generic path, for anything that can shell out before it acts:

```
quorum-adapter gate --agent codex --tool repo.write --classification Proprietary \
  --cost 0 --correlation $SESSION
# exit 0 = proceed · exit 1 = refused (reason on stderr) · exit 2 = usage
```

## Not built yet

- **MCP-native proxying.** The gate is reachable from an MCP server today via
  `gate`, but Quorum does not yet sit *between* an MCP client and server
  transparently gating `tools/call`. That is the remaining S4a transport.
- **Codex and Hermes native integrations.** Both work through `gate` above;
  neither has a vendor-specific hook like Claude Code's.
- **Egress policy by classification** (`02_ARCHITECTURE.md` §4.5) — an agent in
  a CUI room must not be backed by a non-allowlisted model endpoint. The
  adapter records `model_id` but does not yet enforce an allowlist.
