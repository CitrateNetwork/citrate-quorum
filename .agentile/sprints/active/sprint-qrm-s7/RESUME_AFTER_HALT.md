---
created: 2026-07-29
updated: 2026-07-29
branch: fix/qrm-s7-live-run-findings
author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
corrected_by: Claude Opus 5 (1M context), directed by @SaulBuilds
status: active
sprint: QRM-S7
correction: >
  Original premise (a reroll would wipe chain state) did NOT occur. The GhostDAG
  halt was fixed in place; all QRM-S7 on-chain assets survived and were verified
  live. The redeploy path is superseded by a verification path. See the
  correction block at the top.
---

# QRM-S7 — resuming after the GhostDAG halt, and where the live run stopped

> ## ⚠️ CORRECTION 2026-07-29 — THERE WAS NO REROLL. DO NOT REDEPLOY.
>
> This document was written expecting a reroll out of the GhostDAG halt, and told
> you to *"assume everything on chain below is gone and re-do it."* **That did not
> happen.** The halt was fixed in place (citrate-chain #134, plus #135 and #136 for
> the follower-sync wedge the same incident exposed) and block production resumed
> from 54600 on the SAME chain. No state was reset.
>
> Verified against `https://rpc.citrate.ai` on 2026-07-29, chain still `0x9d0c`:
>
> | Check | Result |
> |---|---|
> | `TenantHierarchy` `0xeeb55aa8…` | 5,286 bytes of code |
> | `GovernanceTemplateRegistry` `0xe9dd5756…` | 3,310 bytes |
> | `GovernanceProtocolFactory` `0x260ffedd…` | 4,538 bytes |
> | `PolicyBinding` `0x1dc147b7…` | 4,847 bytes |
> | `getNode(0x16b1608b…)` | `("Citrate", …, exists=true)` — root tenant **intact** |
> | `get(0x5d6ddb0f…)` | `("SegregationOfDuties", 1, …, "bafkreibc3llznx…", active=true)` |
>
> **The redeploy path below is therefore WRONG and is superseded by
> [Verification order](#verification-order).** Re-running
> `post-reroll-quorum-restore.sh --broadcast` or `init-tenant-root.sh` against
> live contracts is at best wasted gas; `initRoot` is one-shot and the root
> admin set is immutable, so a blind re-run is exactly the kind of thing that
> cannot be undone. **Verify first; deploy only what verification shows missing.**
>
> The one thing that IS still required is unchanged: the child tenant
> `Citrate/Quorum` was never created (that gate was the admin key, not the halt).
>
> Pick this up now — chain 40204 is producing at ~2 s/block.

## One-paragraph state

QRM-S7 is code-complete. PR #53 is merged; **PR #54 is open** and carries seven
defects the live run exposed plus the interview inputs the pipeline needed. The
exit gate — *a doc dump becomes a deployed, bound protocol* — is proven through
**COMPILE and SIMULATE** and stops at the signature, because chain 40204 halted
at block 54600 (GhostDAG blue-set OOM) with mining disabled. DEPLOY and BIND
have never executed. Nothing about them is known-good beyond their tests.

**That halt is resolved** (citrate-chain #134/#135/#136, fleet redeployed
2026-07-29; producing at ~2 s/block). The chain was NOT rerolled — see the
correction at the top. So the only thing that changed for QRM-S7 is that the
chain is available again; every on-chain asset the sprint depended on is still
where it was, and the outstanding work is the child tenant plus DEPLOY/BIND.

## What was proven live, and how

Each was checked from OUTSIDE the UI. None of it depended on chain state
surviving — and in the event none had to, since there was no reroll.

| Stage | Result | Independent check |
|---|---|---|
| INGEST | `spec-81c39fe5a65d`, **Proprietary**, from the marking `COMPANY CONFIDENTIAL` | the app's recorded `blake3:718346eb41cd` matches the file's real hash computed separately |
| INTERVIEW | 7 topics answered, 3 corrected | on disk with `by: Larry V Klosowski`, `source: human`, timestamps; corrections carry `superseded` with the originals |
| COMPILE | 2 mapped + 1 waived → deployable | template ids `0x5d6ddb0f…` / `0x0803e33c…` match `templateId()` on chain for SegregationOfDuties / ThresholdApproval |
| SIMULATE | 0/0/0 with the reason stated | *"no recorded decisions in range… these zeros mean 'no evidence', not 'no impact'"* |
| CEREMONY | refused: `custody vault locked or unavailable` | rule 3 failing closed, correctly |

## Chain state: what survived (verified), and what is genuinely still missing

**No reroll happened, so this is a verification list, not a restore list.** Confirm
each before acting; deploy ONLY what a check actually shows absent.

| Asset | Status 2026-07-29 | Action |
|---|---|---|
| Governance contracts (9) | **LIVE** — booked in `40204.json`, bytecode confirmed at all four spot-checked addresses | none. Do **not** run `post-reroll-quorum-restore.sh --broadcast` |
| 8 registered templates | **LIVE** — `SegregationOfDuties v1` reads back `active=true` under the same devnet CID `bafkreibc3llznx…` | spot-check the other 7 with `get(id)`; re-pin only if a version changed |
| Root tenant `Citrate` | **LIVE** — `getNode` returns `exists=true`, admins unchanged | none. Do **not** run `init-tenant-root.sh` — `initRoot` is one-shot |
| **Child tenant `Citrate/Quorum`** | **STILL MISSING** — never created; the halt was not why | **the real remaining step** — needs the owner's admin key, see the gate below |
| Operator gas | check `0x9f5B156C…` balance | top up only if short of ~10,000 SALT |
| `0xF4FE9B2c…` balance | check | top up only if short |

Spot-check commands (read-only, safe to run):

```bash
cast call 0xeeb55aa8d6016eec782ff07b7174ab925fa251ab \
  "getNode(bytes32)((bytes32,bytes32,string,uint8,bytes32,uint8,uint8,bool))" \
  0x16b1608be531254541765b57c0b06290310200c486e9aa08440301524bd20e8a \
  --rpc-url https://rpc.citrate.ai     # last field true => root tenant exists

cast call 0xe9dd5756d2c01a262caed433f67f36ae2a73d3be \
  "get(bytes32)((bytes32,string,uint32,bytes32,bytes32,string,bool))" \
  0x5d6ddb0f68f411f5f5a5bd58532589b715bed638cec6f19341e8dd6406b0c762 \
  --rpc-url https://rpc.citrate.ai     # last field true => template registered+active
```

Note `templateId(string,uint32)` is `pure` — it only proves the id derivation. Use
`get(id)` to prove a template is actually REGISTERED.

### The gate: creating the child tenant needs an admin key

`GovernanceProtocolFactory.deployProtocol` (GF-4) and `PolicyBinding.bind` both
require `msg.sender` to be a tenant admin. The root tenant's admin set is
**immutable** (no `setAdmins`, `initRoot` is one-shot), and the three admins'
private keys are **not in the workspace** — they were passed as
`CIT_AGENT_TIMELOCK_OWNER_{0,1,2}` at deploy time.

The owner holds one: it is in the Foundry keystore as **`citrate-tenant-admin`**
(derives `0xF4FE9B2c6441Ff7c081B60716a78193127919783`, opens with
`CAST_PASSWORD`). It is used **once**, to create the child tenant whose admin is
quorum's own operator wallet. Every later deploy and bind is signed by quorum
through the ceremony.

⚠️ **`CAST_PASSWORD` and the deployer `0x4fAB35c8` are both still pending
rotation** (owner's own list). Rotate before spending, and note the
`CAST_PASSWORD` passphrase is **quoted** in its env source — a naive field split
yields a string that fails to decrypt until the quotes are stripped.

The exact call, with the ids that do not change (they are keccak of names):

```bash
cast send <TenantHierarchy from bfr-40204.json> \
  "createNode(bytes32,bytes32,string,uint8,bytes32,address[],uint8,uint8)" \
  0x16b1608be531254541765b57c0b06290310200c486e9aa08440301524bd20e8a \
  0x71fb253f1a98ec04a018737b33011184818be4b545b866ef42342c7df48757c5 \
  "Citrate/Quorum" 1 0x$(head -c 32 /dev/urandom | xxd -p -c 64) \
  "[0x9f5B156C53305D4b20c94ca08E3219D1C0e7401a]" 1 2 \
  --legacy --rpc-url https://rpc.citrate.ai \
  --account citrate-tenant-admin --password-file <path>
```

`--legacy` is not optional — 40204 rejects EIP-1559. Verify with `getNode`
afterwards rather than trusting the receipt.

## Verification order

Supersedes the "Redeployment order" this document originally carried. Steps 1 and
2 were `--broadcast` deploys; they are now read-only checks, because the state they
would have recreated is still there.

1. **Verify chain state** with the two `cast call`s above. Both should already
   return `true`. If either does not, STOP and work out why before broadcasting
   anything — an unexpected miss means something other than a reroll happened.
2. **Re-vendor the address books.** `cd citrate-quorum && bash scripts/sync-addresses.sh`.
   Still worth doing — it is a no-op if nothing moved, and it proves the book the
   app reads matches the chain. Commit the result in the same sitting; address
   books rot between opening a PR and merging it.
3. **Re-pin the template artifacts.** `bash scripts/vendor-template-artifacts.sh`.
   Rewrites `src-tauri/src/template_hashes.rs` and
   `src-tauri/artifacts/templates/*.hex` from the LIVE registry and refuses to
   write bytes whose hash does not match. If
   `every_embedded_artifact_hashes_to_what_the_registry_pinned` is red, this is
   why. Expected to be a no-op now (same CID, same versions) — but run it, because
   a no-op result is itself the evidence that the pins are still correct.
4. **Create the child tenant** (above) — **the one genuinely outstanding action.**
5. **Fund** `0x9f5B156C…` for gas *if* the balance check shows it short.
6. **Run the pipeline** (below) — DEPLOY and BIND still have never executed.

## Re-running the pipeline

The app runs against an isolated custody keyring — the developer's
`ai.citrate.core` vault is never touched:

```bash
cd citrate-quorum
npm run tauri build -- --bundles deb
scripts/run-dev.sh            # keeps the identity
scripts/run-dev.sh --fresh    # true clean install (wipes ONLY the isolated home)
scripts/run-dev.sh --where    # print paths, launch nothing
```

Isolated home: `~/.local/share/citrate-quorum-dev/`. Do **not** try to reset by
deleting keyring entries — see the script header for why that destroys
citrate-core's vault.

**The owner must do two things by hand; neither can be automated:**

- **Import the operator identity.** The vault takes a **mnemonic** only
  (`wallet_import`); a raw private key cannot be imported. Settings → Identity.
  Address is `0x9f5B156C53305D4b20c94ca08E3219D1C0e7401a`.
- **Unlock the vault after every restart.** The unlock is in-process; a restart
  re-locks it and the ceremony then refuses with `custody vault locked or
  unavailable`. **This is what stopped the run.**

Never screenshot the root window while the phrase is on screen — capture only
the app window (`import -window <id>`). Find the id with
`xwininfo -root -children | grep '"Citrate Quorum"'`.

Driving the UI: use a private `Xvfb :99` rather than the owner's `:1` — it puts
the window at `+0+0` (no window manager, matching `smoke_packaged.py`'s
assumptions) and does not hijack their pointer. **XTEST clicks need
`set_input_focus` first**, or they are silently swallowed; the first click after
launch is dropped regardless. Helper: `scratchpad/drive.py` pattern in PR #54's
history.

Pipeline inputs that reproduce the run:

- Document: `.agentile/policies/CITRATE_AGENT_OPERATIONS_POLICY_2026-07-28.md`
- Tenant scope, typed at onboarding, **exactly**: `Citrate/Quorum`
  (the node id is `keccak256` of this string — `line-4-automation`,
  `line 4 automation` and `Citrate/Quorum` are three different tenants)
- Answers that compile: scope `repo.write` · principals `Larry V Klosowski` ·
  roles `…One approver.` · thresholds `One approval of record…` · escalation
  `within 24hrs` · expiry `45 days` · exceptions `none`

Deploy target will be **SegregationOfDuties** — `target()` takes the first
mapped clause and the interview asks Roles before Thresholds. For
ThresholdApproval to be the target, the policy must have no roles clause.

## Known gaps, carried forward not fixed

- **`action_class_of` binds the whole scope string.** A multi-class answer like
  `repo.write, shell.exec` would hash the entire sentence as one action class.
  The policy names one class deliberately, which hides this. Real bug.
- **Only 2 of 8 templates can be constructed.** `ctor.rs` supports
  ThresholdApproval and SegregationOfDuties; the other four are refused BY NAME
  with the missing argument named. ClassificationGate needs a
  `foreignNationalFloor` the interview never asks for; TimeBoundedElevation
  needs a `requiredRole`; IncidentEscalation needs a responder set and a
  `maxScan`; BudgetedAutonomy maps to no topic at all.
- **The template library cannot express the owner's real answers.**
  "Escalates to the Head of AI" and "expires on employment
  termination or role change" are better policy than the durations that
  compile. `IncidentEscalation` takes a duration and hashed responder ids, not a
  role; `TimeBoundedElevation` takes a window, not a predicate. **The owner
  should decide whether the templates change or the policy stays narrower** —
  do not quietly reshape their policy to fit the tool.
- **The approver identity space is ours alone.** `keccak256(lowercased name)`;
  nothing writes signatures into `MultiSigEnvelope` under it, so a deployed
  ThresholdApproval answers `RequireApproval / NOT_PROPOSED` and finds none. A
  truthful verdict, not working approvals.
- **`PolicyBinding` has no on-chain caller** — every binding is advisory
  (S6.4's enforcement table, unchanged).
- **The devnet `auditCID` is not an audit** (D-2). Nothing registered under it
  may be described to a customer as audited.
- **Kit-level: `KEYRING_SERVICE` is a constant**, so quorum and citrate-core
  share `custody-master-key` and `custody-generation`. Two T1 apps cannot be
  reset independently. Wants fixing in the kit (service should follow the bundle
  identifier).

## Do not touch

**Updated 2026-07-29.** The incident that this warning described is closed: the
GhostDAG work is no longer uncommitted on `feat/m2-membership-bond-lock` — it
merged as citrate-chain **#134**, followed by **#135** (sync completion judged
against the node's own applied height) and **#136** (the sync peer-selection
deadlock that left every follower frozen at 54600). All four fleet nodes run the
#136 binary and the chain is producing normally.

`citrate-chain-wp11` (`reroll2/exec`) and `citrate-chain-mpfix` are still other
sessions' worktrees — leave them alone. `citrate-chain-mpfix` in particular holds
`main` checked out, which is why `gh pr merge --delete-branch` cannot switch
branches there.

Nothing in QRM-S7 touches chain code — the coupling is ordering only, and that
ordering constraint is now satisfied.
