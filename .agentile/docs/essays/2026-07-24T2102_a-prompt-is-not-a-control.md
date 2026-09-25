---
created: 2026-07-24T21:02:00Z
branch: docs/qrm-journals-retros-essays
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: final
---

# A prompt is not a control

> If the only thing stopping an agent from signing is an instruction telling it
> not to, you have documentation, not a control.

## Frame

The whole premise of citrate-quorum is that heterogeneous AI agents — Claude
Code, Codex, MCP tools, A2A peers — act inside a governed venue, and that their
actions are safe to permit *because they are governed*, not because the agents
are well-behaved. That premise fails instantly if the governance is itself made
of instructions to the agent. An agent that can be told "don't do X" is an agent
that can be prompt-injected, fine-tuned, or simply mistaken into doing X. Prompt-
level guidance is a UX affordance. It is never a security boundary.

citrate-quorum's architecture takes this literally, in two places: agents never
hold keys, and every tool call is gated by code the agent does not run.

## Keyless by construction

An agent in this system cannot sign, because it never possesses anything to sign
with. The custody vault and every signature live in citrate-quorum, behind the
SignatureCeremony — the single HIC signing path, shared with
citrate-core through `citrate-core-kit`. The gated signers are `pub(crate)` to
the kit crate, and quorum is *outside* that crate, so quorum code — including any
agent-facing code — physically cannot name a signer. This is not enforced by a
policy that an agent might route around; it is enforced by the Rust module
system, at compile time, for the whole application.

The agent's role is to emit an *unsigned intent* — "I would like to open this
PR," "I would like to cast this vote." The intent travels to a human, or to a
budgeted autonomous policy, and only there does it become a signature. The agent
proposes; it never disposes. A compromised agent produces compromised
*proposals*, which is a threat the ceremony and the policy gate are designed to
catch, rather than compromised *signatures*, which nothing could catch after the
fact.

## The gate is in the adapter, before execution

The second half is the policy gate. Every tool invocation an agent makes passes
through `quorum-policy::evaluate` *in the adapter, before the tool runs*, and
produces a `DecisionRecord` either way. The verdict is computed against the
agent's `CapabilityGrant`s — scope, action class, classification ceiling,
budget, expiry, HIC level — and the possible answers are `Allow`,
`RequireApproval`, `Deny`, and the one that matters most: `Ungoverned`.

`Ungoverned` is what the system returns when no live grant covers the action. It
is not an error and it is not a silent allow. It is recorded, flagged, counted,
and alerted. The design decision that "an action with no authority behind it is a
first-class, visible event" is what makes the absence of a control observable.
Most systems fail open into a gap nobody sees; this one fails into a number on
the Ledger that an auditor samples.

Crucially, the gate is code the agent does not execute. The agent cannot decline
to call it, cannot edit the verdict, cannot suppress the record — because the
adapter, not the agent, holds the gate. A prompt telling the agent "always check
policy first" would be exactly the non-control this essay is about. The adapter
checking policy first, unconditionally, is the control.

## Why the record is part of the control, not an afterthought

It is tempting to file audit logging under "observability" and treat it as
downstream of enforcement. In a governance product it is part of enforcement.
The `DecisionRecord` — principal, grant, HIC level, model id, params hash,
correlation id — is what lets a human answer "on whose authority did this agent
do this?" after the fact, and the per-tenant BLAKE3 hash chain is what makes that
answer non-repudiable. An enforcement decision that leaves no tamper-evident
trace is a decision you cannot later stand behind. The gate and the ledger are
one mechanism.

## What this implies

- **Every new agent adapter follows the keyless, gated pattern**: loopback-only,
  bearer-authed over a `0600` token file, `Zeroizing` secrets, emitting unsigned
  intents, with the policy gate in the adapter before execution. No exceptions
  for "trusted" agents; trust is what the pattern replaces.
- **`Ungoverned` must stay loud.** Any future change that makes an ungoverned
  action quieter — batched, sampled, downgraded — is a regression in the control,
  not an optimization of the log.
- **Prompt-level guidance is allowed, but never counted.** Telling an agent how
  to behave is fine UX; it must never appear in a threat model as a mitigation.

## What this does NOT imply

- It does not imply agents are adversaries. Most agent actions are benign and
  most grants will `Allow`. The point is that safety does not *depend* on the
  agent being benign — the same mechanism holds whether the agent is helpful,
  buggy, or hijacked.
- It does not imply the human is always in the signing loop. HIC-2 (budgeted
  autonomy) and HIC-3 (post-hoc) deliberately let agents act without a per-action
  signature. What is non-negotiable is that they act within a *grant* the human
  set, recorded per action — control by envelope, not control by instruction.

## References

- HIC model (normative): `04_HIC_MODEL.md` in the federation planset
- Code: `citrate-core-kit` ceremony/custody; `quorum-policy::evaluate`; `quorum-audit::DecisionRecord`
- Journal: `.agentile/docs/journals/2026-07-24T2050_kit-extraction-two-apps-one-signer.md`
- Companion essay: `.agentile/docs/essays/2026-07-24T2103_honest-by-construction.md`
