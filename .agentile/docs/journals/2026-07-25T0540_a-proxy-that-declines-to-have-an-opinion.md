---
created: 2026-07-25T05:40:00Z
branch: docs/qrm-s4-retro-journals
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# A proxy that declines to have an opinion

> I was four days from writing an MCP gateway against a protocol that was about
> to stop existing. The owner said: search first, and make sure we're on the
> next standard.

## Context

The remaining S4a work was an MCP proxy — Quorum sitting between an agent and
an MCP server, gating every `tools/call` before it runs. I knew the protocol
well enough to start typing. The owner's instruction was to check the planned
breaking changes first.

MCP `2026-07-28` turned out to be the largest revision since launch, with the
final spec shipping four days after that conversation.

## What happened

The list is not a list of tweaks. It removes the `initialize` handshake and
protocol-level sessions entirely, drops `Mcp-Session-Id`, deletes `ping` and
`logging/setLevel`, replaces server-initiated requests with Multi Round-Trip
Requests, adds a required `resultType` to every result, deprecates Roots,
Sampling and Logging, and repartitions the JSON-RPC error space so that
`-32020..-32099` is now reserved for the specification.

A proxy that modelled the protocol would have broken on release day. Worse, a
proxy that *had* been correct on the 24th and broke on the 28th would break
silently in the middle of somebody's agent run.

So the design became: **inspect exactly one method and refuse to understand
anything else**. `tools/call` — whose shape (`params.name`, `params.arguments`)
is stable across every revision from `2024-11-05` through the draft — is parsed
and gated. `server/discover`, `subscriptions/listen`, `tasks/*`, extensions and
methods that do not exist yet are opaque bytes and go straight through. The
response is never parsed at all, which is what lets SSE streams, `resultType`
and MRTR's `input_required` pass untouched.

Then the new spec started giving things back. Statelessness means there is no
session to track, so each call is independently gateable and the proxy holds no
per-connection state it could get wrong. `_meta`'s
`io.modelcontextprotocol/clientInfo` names the calling agent, so the ledger
records *who called* rather than what we were configured to assume — I watched
`agent=codex` land in the evidence chain without having configured it. And
`_meta`'s `traceparent` became the decision's correlation id, so a governed
action lines up with the same trace an OpenTelemetry backend sees.

One detail mattered more than it looks. The revision requires `Mcp-Method` on
every Streamable HTTP POST *specifically so a gateway can route without parsing
the body*, and the HTTP proxy uses it as the fast path. But routing on the
header **alone** would let a client skip the gate by omitting a header it is
merely obliged to send. So the body is checked too, and either signal gates. I
verified it: a `tools/call` sent with no `Mcp-Method` header was still refused.

## What I learned

**"Search first" was worth more than any amount of care in the implementation.**
I would have written a competent proxy against `2025-11-25`. It would have
passed its tests, shipped, and quietly stopped gating things a few days later —
which for a governance component means calls flowing through ungated while the
UI still looked healthy.

**An intermediary's robustness comes from what it refuses to understand.** Every
method the proxy parses is a coupling to a protocol version. The blast radius of
a spec change is exactly the surface you chose to have an opinion about. Keeping
that surface to one method is not laziness; it is the design.

**A required field is a promise, not a control.** The spec says `Mcp-Method`
MUST be present. Requirements bind conforming implementations; they do not bind
an attacker or a buggy client. Anything a security decision rests on has to be
verified from something the caller cannot simply omit.

**Reserved namespaces deserve respect even when nothing enforces them.** Picking
a refusal code from `-32000..-32019` rather than the newly reserved
`-32020..-32099` costs nothing today and avoids a collision with a
spec-allocated error later. Squatting is easy and invisible until it isn't.

## What I'd do differently

Nothing about the approach — but I would have searched before building the
*first* adapter too, not just this one. The Claude Code hook contract I read off
a real installed hook in the environment rather than guessing, which was right;
the Codex one I only checked when I got to it, and it turned out its hooks are
off by default and published accounts of its parser contradict each other. That
ambiguity is now handled (deny on both channels, and the coverage question
answered empirically from the ledger), but I could have known it a day earlier.

## Open questions

- The final spec ships on the 28th. The proxy is built against the release
  candidate and designed to be indifferent, but "designed to be indifferent"
  should be *tested* against the released schema — is there a conformance suite
  worth running the proxy through?
- MRTR replaces server-initiated requests with an `input_required` result the
  client retries. Our escalation is a different shape — it needs a *human*, not
  the client — so a refusal is right today. But is there a future where an
  escalation is expressed as `input_required` and the client waits properly
  instead of being told to retry later?

## Pointers

- Sprint: `.agentile/sprints/completed/sprint-qrm-s4/RETRO.md`
- PRs: #18 (stdio proxy + egress), #19 (Streamable HTTP + Codex)
- Key artifact: `src-tauri/crates/quorum-adapter/src/mcp.rs`
- Spec: [key changes](https://modelcontextprotocol.io/specification/draft/changelog),
  [2026-07-28 release candidate](https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/)
