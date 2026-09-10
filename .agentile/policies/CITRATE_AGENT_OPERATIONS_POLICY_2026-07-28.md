---
created: 2026-07-28
branch: chore/qrm-dev-launcher
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: draft
classification: COMPANY CONFIDENTIAL
---

# Citrate Agent Operations Policy

**COMPANY CONFIDENTIAL — internal governance, not for distribution.**

## Why this document exists

Citrate builds governance software for agents and is itself operated almost
entirely by agents. Whatever we ask a customer to accept, we should already be
living under. This is the policy that governs our own agent fleet, and it is the
first document put through the citrate-quorum authoring pipeline.

It is deliberately short. A policy that nobody can hold in their head is a
policy that gets bypassed, and every clause below has to survive being compiled
into an audited on-chain template — a rule we cannot express in that vocabulary
is a rule we cannot enforce, and saying so is more useful than writing it down
and pretending.

## What agents actually do here

The fleet is not hypothetical. Agents reach the work through four seams, each
gated by the citrate-quorum adapter before execution:

- Claude Code, as a `PreToolUse` hook
- Codex, through the same hook contract
- MCP servers, through the near-blind stdio and Streamable HTTP proxies
- CI and scripted callers, through the generic `gate` subcommand

Their work is code review, security analysis, sprint execution and
documentation. Two toolsets dominate it, and both are the reason this policy
needs approval gates rather than budget caps:

**The Trail of Bits skill set.** Roughly thirty analysis skills are installed —
`semgrep`, `codeql`, `c-review`, `zeroize-audit`, `constant-time-analysis`,
`building-secure-contracts`, `entry-point-analyzer`, `variant-analysis`,
`supply-chain-risk-auditor`, `differential-review`, `trailmark` and the rest.
These are overwhelmingly **read-only**: they parse, scan and report. They are
the cheapest agent work we have and the least dangerous, and this policy should
not tax them. What makes them worth governing is not the reading but what
follows it — a finding becomes a patch, and the patch is the governed act.

**The Agentile loop.** Rules 0 through 13 are the operating method: read before
writing, no mocks, test counts only rise, audits are immutable, the sprint file
is authoritative, every document carries frontmatter, data sources are traced,
no unwraps in production paths, one source of truth per topic, authorization
before destructive operations. Agents execute this loop continuously and it is
where they touch the repository.

Rule 10 — *authorization before destructive operations* — is the sentence this
whole policy is an implementation of. It has been a convention enforced by
agents reading it. This makes it a control.

## Scope

This policy governs `repo.write`.

That is one action class, named deliberately. Writing to a repository is where
an agent's judgement becomes durable and where a mistake outlives the session
that made it — a bad patch survives, a bad analysis does not. `shell.exec`,
`net.fetch` and the `tool.*` classes are governed by their own protocols and are
out of scope here; a policy that claimed all of them at once would be one
sentence pretending to be four controls.

## Principals

Larry V Klosowski.

The accountable human is named, singular, and that is the honest state of this
company today: one owner, sole maintainer, who answers for what the fleet does.
A policy naming a committee that does not meet would be worse than a policy
naming one person who does.

## Approval threshold

One approval of record is required before an agent may act under this policy.

A 1-of-1 threshold is not theatre and it is not a placeholder for a larger
number. It means every governed write is attributable to a named human who saw
it, which is exactly the evidence a control needs to produce. The number rises
when the board seats are filled, and raising it is itself a governed act.

## Escalation

An escalation that no human has answered is reviewed within 24 hours.

## Expiry

A capability grant expires after 45 days and is renewed by ceremony, never
silently.

## Exceptions

Read-only analysis is excepted from approval. Running `semgrep`, `codeql`,
`c-review` or any other scanner produces a report and changes nothing, so
gating it would spend a human's attention on the safest thing an agent does
while teaching everyone that approvals are noise. The act that needs a human is
the patch the finding argues for, not the finding.

## What this policy cannot say yet

Stated because the pipeline forces it to be, and because a control set that
hides its own edges is not auditable:

- **It cannot bind more than one action class.** The authoring pipeline compiles
  a single scope clause into a single on-chain binding today. Governing
  `shell.exec` means a second protocol, deployed separately.
- **Its enforcement is advisory.** `PolicyBinding` has no on-chain caller: an
  agent that goes around the adapter is bound by nothing here. What the binding
  changes is that an action taken against it is provably a violation rather than
  a disagreement, and that an ungoverned action is visible as such.
- **The approver identity space is ours alone.** Nothing yet writes signatures
  into `MultiSigEnvelope` under the same derivation, so a deployed threshold
  protocol will answer "approval required" and find none recorded. That is a
  truthful verdict, not working approvals.
- **It is not audited.** The templates it compiles to carry a devnet CID whose
  content says so. No external audit of this system exists.
