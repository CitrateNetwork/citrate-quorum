---
created: 2026-07-22
branch: main
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
status: active
---

# CLAUDE.md — citrate-quorum hard rules

Start at `.agentile/AGENT_ENTRY.md`. Canonical truth is the federation planset
(`citrate-federation/.agentile/planset/2026-07-22-citrate-quorum/`), and within it
`09_DECISIONS_LOCKED.md` supersedes everything else.

## Hard rules (non-negotiable)

1. **No mocks (Rule 1).** No mocked data, fake fixtures presented as live, or
   placeholder features that pretend to work. Every surface states what is real.
   Chain reads hit a live RPC or show an honest error. An unbuilt surface ships
   named honestly or does not ship.

2. **Test count monotone (Rule 2).** `cargo test --workspace --locked` and
   `npm test` counts never decrease. Record the count when it changes.

3. **All signatures via the SignatureCeremony (Rule 3).** The ceremony is the
   single human-in-the-loop signing path, inherited from `citrate-core-kit`. The
   gated signer is `pub(crate)` and reachable ONLY from
   `SignatureCeremony::approve`; **signing anywhere else is forbidden** and a
   source-scan test asserts it. No `#[tauri::command]` signs or returns key /
   seed / entropy. Approval is bound to an explicit `CeremonyId` — no
   auto-approve, no "approve latest"; one approval yields exactly one signature;
   undecodable calldata is blocked until an explicit raw-mode ack; a locked vault
   fails closed. **No agent, sidecar, daemon, or remote service ever holds a key
   or signs directly.**

4. **Agents are keyless and gated.** Every agent adapter follows the node-agent
   pattern: loopback-only, bearer-authed over a `0600` token file, `Zeroizing`
   secrets, emitting *unsigned* intents. Every tool invocation passes the policy
   gate **in the adapter**, before execution, and produces a decision record.
   A prompt instructing an agent to behave is not a control.

5. **HIC levels are per-action, not global.** Every recorded action carries the
   principal, grant id, and HIC level in force. An action without a live grant is
   recorded as `ungoverned` and alerted — never silently allowed, never silently
   dropped.

6. **Multi-tenancy from day one.** No global singletons for anything
   tenant-scoped. Every domain seam takes a `TenantContext`; storage is keyed by
   `(tenant, principal)`. Retrofitting tenancy is not an option (Q20).

7. **Never commit to main.** All work on a feature branch + PR via `gh`. Commit
   with explicit paths only (`git add <paths>`, never `-A` / `.`). Never merge —
   the owner merges. (The initial scaffold commit is the sole exception.)

8. **No hardcoded chain addresses.** Read the frozen address book at runtime; a
   mismatch is a hard error, never a silent fallback. The next reroll shifts most
   addresses.

9. **Every doc gets YAML frontmatter** (created, branch, author, status).

10. **Link, don't copy (Rule 9).** The planset lives in citrate-federation; this
    repo points at it. Safety-critical code lives in `citrate-core-kit`; this repo
    depends on it. Copy-paste of either is a defect.

11. **Data-source tracing (Rule 11).** Every UI surface names the command, and
    every command names the contract method, RPC call, or local store it reads.
    No surface renders a number whose origin cannot be stated.

12. **T1 repo.** Money, keys, identity, governance, distribution. @rule8 items
    (custody, updater keys, gated downloads, export-controlled surfaces) need
    security sign-off before deploy. Full audit before release.

## Export control (G1 — pending legal opinion)

Export-controlled data is **in scope** for this product (Q13). Until the written
legal opinion lands (gate G1, blocks QRM-S3), do **not**: add features that
process CUI/ITAR material, document handling procedures for controlled technical
data, or make claims about export-control compliance. Build the classification
ladder generically and treat the constraint as configuration.

## Compliance language

Never write or say "SOC 2 certified" or "SOC 2 compliant" about this product. The
accurate claim is: **"designed to generate SOC 2 Type 2 control evidence in your
environment."** We ship software into the customer's control environment; they are
the audited entity (Q18).

## Commits

End commits with:
`Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
